//! FC resolution, warp-vs-combat→payout alignment, and dead-missile math.
//!
//! See docs/superpowers/specs/2026-08-02-gamelog-enrichment-v15-design.md
//! ("Domain algorithms") for the authoritative rules this module implements.

use chrono::{DateTime, Utc};

use crate::analytics_types::{
    Diagnostic, EnrichmentSite, EnrichmentSnapshot, EnrichmentSource, EnrichmentTotals,
    MissileStat,
};
use crate::gamelog_parse::{GamelogEvent, GamelogEventKind};
use crate::gamelog_scan::ListenerLog;

/// Events that end a warp segment for timing purposes.
/// `CombatHit` doubles as both a missile hit and a `combat_any` timing signal.
fn is_end_marker(kind: GamelogEventKind) -> bool {
    matches!(
        kind,
        GamelogEventKind::Regrouping | GamelogEventKind::CombatHit | GamelogEventKind::CombatAny
    )
}

/// Events that count as "combat" for the combat→payout clock. Unlike
/// `is_end_marker`, neither `Regrouping` nor incoming `CombatAny` count:
/// only an actual outgoing hit (`CombatHit`) starts the payout countdown.
fn is_combat_kind(kind: GamelogEventKind) -> bool {
    matches!(kind, GamelogEventKind::CombatHit)
}

/// Minimum gap between two `FollowingWarp` events for both to count as
/// distinct warp-start signals. Warps within this window of an already
/// accepted start are treated as the same fleet jump (e.g. two listeners
/// warping together) and debounced away.
const WARP_START_DEBOUNCE_SECS: i64 = 10;

/// Sort `segments` by start and merge any whose start falls at or before the
/// current merged segment's end.
fn coalesce_intervals(
    mut segments: Vec<(DateTime<Utc>, DateTime<Utc>)>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    segments.sort_by_key(|(start, _)| *start);
    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (start, end) in segments {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                if end > last.1 {
                    last.1 = end;
                }
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

/// Resolve the fleet commander's Listener name.
///
/// Order: settings name (if it matches a Listener in range) → wallet
/// description hint (if it matches) → Listener with fewest `following_warp`
/// events in the window.
pub fn resolve_fc(
    logs: &[ListenerLog],
    fc_character: Option<&str>,
    wallet_fc_hint: Option<&str>,
) -> Option<String> {
    if let Some(name) = fc_character {
        let name = name.trim();
        if !name.is_empty() {
            if let Some(log) = logs.iter().find(|l| l.listener == name) {
                return Some(log.listener.clone());
            }
        }
    }
    if let Some(hint) = wallet_fc_hint {
        let hint = hint.trim();
        if !hint.is_empty() {
            if let Some(log) = logs.iter().find(|l| l.listener == hint) {
                return Some(log.listener.clone());
            }
        }
    }
    logs.iter()
        .min_by_key(|l| {
            l.events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::FollowingWarp)
                .count()
        })
        .map(|l| l.listener.clone())
}

/// Events from `log` occurring in the open-start/closed-end interval `(gap_start, gap_end]`.
fn events_in_gap<'a>(
    log: &'a ListenerLog,
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
) -> Vec<&'a GamelogEvent> {
    log.events
        .iter()
        .filter(|e| e.occurred_at > gap_start && e.occurred_at <= gap_end)
        .collect()
}

/// First `CombatHit` across the fleet's pooled events with
/// `clear_start <= occurred_at < gap_end`, converted to seconds-until-payout.
fn combat_to_payout(
    fleet_gap_events: &[&GamelogEvent],
    clear_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
) -> Option<i64> {
    fleet_gap_events
        .iter()
        .filter(|e| is_combat_kind(e.kind) && e.occurred_at >= clear_start && e.occurred_at < gap_end)
        .min_by_key(|e| e.occurred_at)
        .map(|e| (gap_end - e.occurred_at).num_seconds().max(0))
}

/// Align a single payout gap into warp seconds and combat→payout.
///
/// Warp **starts** are fused across the fleet (debounced). Warp **ends** only
/// on end-markers from the jump cohort — listeners who also logged
/// `FollowingWarp` within the debounce window of that accepted start — so a
/// straggler still fighting the previous grid cannot zero the fleet clock.
/// Combat→payout still reads the earliest fleet `CombatHit` after clear.
///
/// `resolved_fc` is still computed by the caller for Summary display but is
/// not consulted here.
fn align_gap(
    logs: &[ListenerLog],
    _resolved_fc: Option<&str>,
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
    break_threshold_minutes: u32,
) -> (i64, Option<i64>, bool, EnrichmentSource) {
    // Flat pool of every listener's events in the gap, tagged with listener.
    let all_gap_events: Vec<(&str, &GamelogEvent)> = logs
        .iter()
        .flat_map(|l| {
            events_in_gap(l, gap_start, gap_end)
                .into_iter()
                .map(move |e| (l.listener.as_str(), e))
        })
        .collect();

    let raw_starts: Vec<(DateTime<Utc>, &str)> = {
        let mut v: Vec<(DateTime<Utc>, &str)> = all_gap_events
            .iter()
            .filter(|(_, e)| e.kind == GamelogEventKind::FollowingWarp)
            .map(|(listener, e)| (e.occurred_at, *listener))
            .collect();
        v.sort_by_key(|(at, _)| *at);
        v
    };

    // Debounce warp-starts within WARP_START_DEBOUNCE_SECS of the last
    // accepted start: multiple listeners following the same jump must not
    // each start a new warp segment.
    let mut accepted_starts: Vec<DateTime<Utc>> = Vec::new();
    for &(s, _) in &raw_starts {
        let debounced = accepted_starts
            .last()
            .is_some_and(|&last| (s - last).num_seconds() < WARP_START_DEBOUNCE_SECS);
        if !debounced {
            accepted_starts.push(s);
        }
    }

    let fleet_events: Vec<&GamelogEvent> = all_gap_events.iter().map(|(_, e)| *e).collect();

    if accepted_starts.is_empty() {
        let gap_seconds = (gap_end - gap_start).num_seconds().max(0);
        let break_secs = i64::from(break_threshold_minutes) * 60;
        if gap_seconds > break_secs {
            return (0, None, true, EnrichmentSource::Heuristic);
        }
        let clear_start = gap_start;
        let combat = combat_to_payout(&fleet_events, clear_start, gap_end);
        return (0, combat, false, EnrichmentSource::Heuristic);
    }

    let segments: Vec<(DateTime<Utc>, DateTime<Utc>)> = accepted_starts
        .iter()
        .enumerate()
        .map(|(idx, &start)| {
            let next_start = accepted_starts.get(idx + 1).copied();
            // Jump cohort: anyone who FollowingWarp'd within the debounce
            // window of this accepted start may end the segment.
            let cohort: Vec<&str> = raw_starts
                .iter()
                .filter(|(at, _)| {
                    *at >= start && (*at - start).num_seconds() < WARP_START_DEBOUNCE_SECS
                })
                .map(|(_, listener)| *listener)
                .collect();

            let marker = all_gap_events
                .iter()
                .filter(|(listener, e)| {
                    cohort.contains(listener)
                        && is_end_marker(e.kind)
                        && e.occurred_at > start
                })
                .map(|(_, e)| e.occurred_at)
                .min();

            let mut candidates: Vec<DateTime<Utc>> = vec![gap_end];
            if let Some(m) = marker {
                candidates.push(m);
            }
            if let Some(ns) = next_start {
                candidates.push(ns);
            }
            let end = *candidates.iter().min().unwrap();
            (start, end)
        })
        .collect();

    let coalesced = coalesce_intervals(segments);
    let warp_seconds: i64 = coalesced
        .iter()
        .map(|(start, end)| (*end - *start).num_seconds().max(0))
        .sum();
    let clear_start = coalesced.last().map(|(_, end)| *end).unwrap_or(gap_start);

    let combat = combat_to_payout(&fleet_events, clear_start, gap_end);

    (warp_seconds, combat, false, EnrichmentSource::Fleet)
}

/// Copy of `logs` keeping only events inside the sealed run's window.
///
/// `scan_gamelogs` selects whole files by session-start overlap, so a file
/// can carry hours of events from before or after the run. Missile math and
/// FC resolution must see only the run itself.
fn clip_to_window(
    logs: &[ListenerLog],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<ListenerLog> {
    logs.iter()
        .map(|l| ListenerLog {
            listener: l.listener.clone(),
            path: l.path.clone(),
            events: l
                .events
                .iter()
                .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
                .cloned()
                .collect(),
        })
        .collect()
}

/// Compute per-listener dead missiles over the run window.
fn missile_stats(logs: &[ListenerLog], missiles_per_cycle: u32) -> Vec<MissileStat> {
    logs.iter()
        .map(|l| {
            let reload_cycles =
                l.events.iter().filter(|e| e.kind == GamelogEventKind::Reload).count() as u32;
            let hits = l
                .events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::CombatHit)
                .count() as u32;
            let expended = reload_cycles.saturating_mul(missiles_per_cycle);
            let dead = expended.saturating_sub(hits);
            MissileStat {
                listener: l.listener.clone(),
                reload_cycles,
                hits,
                missiles_per_cycle,
                dead,
            }
        })
        .collect()
}

/// Enrich a sealed run's wallet sites with gamelog-derived warp/combat→payout
/// splits and per-character dead-missile counts.
pub fn enrich_run(
    logs: &[ListenerLog],
    site_times: &[DateTime<Utc>],
    break_threshold_minutes: u32,
    run_start: Option<DateTime<Utc>>,
    missiles_per_cycle: u32,
    fc_character: Option<&str>,
    wallet_fc_hint: Option<&str>,
) -> EnrichmentSnapshot {
    let mut diagnostics = Vec::new();
    let listeners: Vec<String> = logs.iter().map(|l| l.listener.clone()).collect();

    if logs.is_empty() {
        diagnostics.push(Diagnostic {
            level: "warn".to_string(),
            message: "No listeners with gamelogs in range; enrichment skipped.".to_string(),
        });
    }

    // Everything below (alignment, FC resolution, missiles) is scoped to the
    // sealed run's window: run_start (or the first payout) → last payout.
    let window = site_times.first().map(|first| {
        let start = run_start.map(|rs| rs.min(*first)).unwrap_or(*first);
        (start, *site_times.last().unwrap())
    });
    let clipped: Vec<ListenerLog> = match window {
        Some((start, end)) => clip_to_window(logs, start, end),
        None => logs.to_vec(),
    };
    let logs: &[ListenerLog] = &clipped;

    let resolved_fc = resolve_fc(logs, fc_character, wallet_fc_hint);

    let mut sites = Vec::with_capacity(site_times.len());
    // (warp_seconds, combat_to_payout_seconds) for sites that count toward averages.
    let mut counted: Vec<(i64, Option<i64>)> = Vec::new();

    for (i, &occurred_at) in site_times.iter().enumerate() {
        let gap_start = if i == 0 {
            run_start
        } else {
            Some(site_times[i - 1])
        };

        let Some(gap_start) = gap_start else {
            // v1 parity: first site with no run_start has no alignable open
            // bound. It still counts for ISK/LP upstream, but is excluded
            // from warp/combat→payout averages here.
            sites.push(EnrichmentSite {
                occurred_at,
                warp_seconds: 0,
                combat_to_payout_seconds: None,
                is_break: false,
                source: EnrichmentSource::Heuristic,
            });
            continue;
        };

        let (warp_seconds, combat_to_payout_seconds, is_break, source) = align_gap(
            logs,
            resolved_fc.as_deref(),
            gap_start,
            occurred_at,
            break_threshold_minutes,
        );

        if !is_break {
            counted.push((warp_seconds, combat_to_payout_seconds));
        }

        sites.push(EnrichmentSite {
            occurred_at,
            warp_seconds,
            combat_to_payout_seconds,
            is_break,
            source,
        });
    }

    let missiles = missile_stats(logs, missiles_per_cycle);
    let fleet_dead: u32 = missiles.iter().map(|m| m.dead).sum();

    let total_warp: i64 = counted.iter().map(|(w, _)| w).sum();
    let combat_values: Vec<i64> = counted
        .iter()
        .filter_map(|(_, c)| *c)
        .collect();
    let total_combat: Option<i64> = if combat_values.is_empty() {
        None
    } else {
        Some(combat_values.iter().sum())
    };
    let avg_combat_to_payout_seconds = if combat_values.is_empty() {
        None
    } else {
        Some(combat_values.iter().sum::<i64>() as f64 / combat_values.len() as f64)
    };

    EnrichmentSnapshot {
        resolved_fc,
        listeners,
        diagnostics,
        sites,
        missiles,
        totals: EnrichmentTotals {
            warp_seconds: total_warp,
            combat_to_payout_seconds: total_combat,
            avg_combat_to_payout_seconds,
            fleet_dead,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 2, h, m, s).unwrap()
    }

    fn ev(h: u32, m: u32, s: u32, kind: GamelogEventKind) -> GamelogEvent {
        GamelogEvent {
            occurred_at: ts(h, m, s),
            kind,
            raw_hint: String::new(),
        }
    }

    fn listener_log(name: &str, events: Vec<GamelogEvent>) -> ListenerLog {
        ListenerLog {
            listener: name.to_string(),
            path: std::path::PathBuf::from(format!("{name}.txt")),
            events,
        }
    }

    /// Worked example: Alt warps and lands (Regrouping); FC's first combat
    /// after clear starts Combat→payout. Warp end markers come from the jump
    /// cohort (who FollowingWarp'd), not from a non-warping listener.
    #[test]
    fn worked_example_fleet_warp_130_330() {
        let fc = listener_log(
            "FC Pilot",
            vec![ev(20, 2, 30, GamelogEventKind::CombatHit)],
        );
        let alt = listener_log(
            "Alt Pilot",
            vec![
                ev(20, 0, 20, GamelogEventKind::FollowingWarp),
                ev(20, 2, 30, GamelogEventKind::Regrouping),
            ],
        );
        let logs = vec![fc, alt];
        let sites = vec![ts(20, 0, 0), ts(20, 8, 0)];

        let snap = enrich_run(
            &logs,
            &sites,
            25,
            Some(ts(19, 50, 0)),
            156,
            Some("FC Pilot"),
            None,
        );

        assert_eq!(snap.resolved_fc.as_deref(), Some("FC Pilot"));
        assert_eq!(snap.sites.len(), 2);
        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 130);
        assert_eq!(site.combat_to_payout_seconds, Some(330));
        assert_eq!(site.source, EnrichmentSource::Fleet);
        assert!(!site.is_break);
    }

    #[test]
    fn no_markers_long_gap_is_break() {
        let fc = listener_log("FC Pilot", vec![]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 40, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, None, None);

        let site = &snap.sites[1];
        assert!(site.is_break);
        assert_eq!(site.warp_seconds, 0);
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    #[test]
    fn no_markers_short_gap_not_break_null_combat_without_fc_events() {
        let fc = listener_log("FC Pilot", vec![]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 10, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, None, None);

        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.warp_seconds, 0);
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    /// No warp markers in the gap, but a listener log has a `CombatHit`:
    /// combat is measured from `gap_start` (the heuristic `clear_start`)
    /// using the pooled fleet events.
    #[test]
    fn no_markers_short_gap_combat_from_log_only() {
        let fc = listener_log("FC Pilot", vec![ev(20, 6, 0, GamelogEventKind::CombatHit)]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 10, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, Some("FC Pilot"), None);

        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.warp_seconds, 0);
        assert_eq!(site.combat_to_payout_seconds, Some(240));
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    #[test]
    fn dead_missiles_two_reloads_100_hits_cycle_156() {
        let listener = listener_log(
            "Gunner",
            vec![
                ev(20, 1, 0, GamelogEventKind::Reload),
                ev(20, 5, 0, GamelogEventKind::Reload),
            ]
            .into_iter()
            .chain((0..100).map(|i| ev(20, 2, i % 60, GamelogEventKind::CombatHit)))
            .collect(),
        );
        let logs = vec![listener];
        let sites = vec![ts(20, 8, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(20, 0, 0)), 156, None, None);

        assert_eq!(snap.missiles.len(), 1);
        let m = &snap.missiles[0];
        assert_eq!(m.reload_cycles, 2);
        assert_eq!(m.hits, 100);
        assert_eq!(m.missiles_per_cycle, 156);
        assert_eq!(m.dead, 212);
        assert_eq!(snap.totals.fleet_dead, 212);
    }

    #[test]
    fn first_site_without_run_start_excluded_from_avg() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 10, 0, GamelogEventKind::FollowingWarp),
                ev(20, 12, 0, GamelogEventKind::CombatHit),
            ],
        );
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 20, 0)];

        let snap = enrich_run(&logs, &sites, 25, None, 156, Some("FC Pilot"), None);

        let first = &snap.sites[0];
        assert_eq!(first.warp_seconds, 0);
        assert_eq!(first.combat_to_payout_seconds, None);
        assert!(!first.is_break);

        let second = &snap.sites[1];
        assert_eq!(second.warp_seconds, 120);
        assert_eq!(second.combat_to_payout_seconds, Some(480));

        // Averages must only reflect the second (alignable) site.
        assert_eq!(snap.totals.avg_combat_to_payout_seconds, Some(480.0));
        assert_eq!(snap.totals.warp_seconds, 120);
        assert_eq!(snap.totals.combat_to_payout_seconds, Some(480));
    }

    /// Events from earlier/later sessions in the same gamelog file must not
    /// leak into missile math or FC resolution.
    #[test]
    fn events_outside_run_window_are_ignored() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                // Before the run: would otherwise make FC look like a member.
                ev(18, 0, 0, GamelogEventKind::FollowingWarp),
                ev(18, 0, 5, GamelogEventKind::FollowingWarp),
                // Before the run: would otherwise add 156 expended missiles.
                ev(18, 1, 0, GamelogEventKind::Reload),
                ev(20, 3, 0, GamelogEventKind::CombatAny),
            ],
        );
        let member = listener_log(
            "Member Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                // After the last payout: also out of window.
                ev(22, 0, 0, GamelogEventKind::Reload),
            ],
        );
        let logs = vec![fc, member];
        let sites = vec![ts(20, 0, 0), ts(20, 8, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, None, None);

        assert_eq!(
            snap.resolved_fc.as_deref(),
            Some("FC Pilot"),
            "pre-run warps must not count toward fewest-warps FC resolution"
        );
        assert_eq!(snap.totals.fleet_dead, 0);
        for m in &snap.missiles {
            assert_eq!(m.reload_cycles, 0, "listener {}", m.listener);
        }
    }

    /// Two FC warps inside one gap: both warp legs count, and the stretch
    /// between the first leg's end marker and the second warp is in-site.
    #[test]
    fn multi_warp_gap_sums_both_legs() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::CombatAny),
                ev(20, 5, 0, GamelogEventKind::FollowingWarp),
                ev(20, 6, 30, GamelogEventKind::CombatHit),
            ],
        );
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 8, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, Some("FC Pilot"), None);

        let site = &snap.sites[1];
        // Warp legs: 20:01→20:02 (60s) and 20:05→20:06:30 (90s).
        assert_eq!(site.warp_seconds, 150);
        // clear_start = end of last warp segment (20:06:30); the CombatHit
        // that ended that segment is itself the first combat → payout 90s.
        assert_eq!(site.combat_to_payout_seconds, Some(90));
        assert_eq!(site.source, EnrichmentSource::Fleet);
        assert!(!site.is_break);
    }

    /// A gap longer than the break threshold is still a real site when the
    /// FC's markers show a warp into it.
    #[test]
    fn long_gap_with_markers_is_not_a_break() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 5, 0, GamelogEventKind::FollowingWarp),
                ev(20, 8, 0, GamelogEventKind::CombatHit),
            ],
        );
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 40, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, Some("FC Pilot"), None);

        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.warp_seconds, 180);
        assert_eq!(site.combat_to_payout_seconds, Some(32 * 60));
    }

    /// Warp ends via the warping listener's Regrouping; Combat→payout from
    /// the first fleet CombatHit after clear.
    #[test]
    fn combat_to_payout_from_first_fc_combat_after_warp() {
        let fc = listener_log(
            "FC Pilot",
            vec![ev(20, 3, 0, GamelogEventKind::CombatHit)],
        );
        let alt = listener_log(
            "Alt Pilot",
            vec![
                ev(20, 0, 20, GamelogEventKind::FollowingWarp),
                ev(20, 2, 30, GamelogEventKind::Regrouping),
            ],
        );
        let logs = vec![fc, alt];
        let sites = vec![ts(20, 0, 0), ts(20, 8, 0)];

        let snap = enrich_run(
            &logs,
            &sites,
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("FC Pilot"),
            None,
        );

        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 130);
        assert_eq!(site.combat_to_payout_seconds, Some(300));
        assert_eq!(site.source, EnrichmentSource::Fleet);
        assert!(!site.is_break);
    }

    /// Plan Task 2, test 2: combat is null both when the gap is a break and
    /// when the gap has warp markers but the FC never logs combat after
    /// clear_start.
    #[test]
    fn combat_null_on_break_and_when_no_combat() {
        let fc = listener_log("FC Pilot", vec![]);
        let break_logs = vec![fc];
        let break_sites = vec![ts(20, 0, 0), ts(20, 40, 0)];
        let break_snap = enrich_run(&break_logs, &break_sites, 25, Some(ts(19, 55, 0)), 156, None, None);
        let break_site = &break_snap.sites[1];
        assert!(break_site.is_break);
        assert_eq!(break_site.combat_to_payout_seconds, None);

        let fc_no_combat = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::Regrouping),
            ],
        );
        let logs = vec![fc_no_combat];
        let sites = vec![ts(20, 0, 0), ts(20, 8, 0)];
        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, Some("FC Pilot"), None);
        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.warp_seconds, 60);
        assert_eq!(site.combat_to_payout_seconds, None);
    }

    /// Plan Task 2, test 3: the first site with no leading `run_start` has no
    /// alignable open bound, so it gets null combat and zero warp regardless
    /// of what the logs contain.
    #[test]
    fn first_site_without_run_start_has_null_combat() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 10, 0, GamelogEventKind::FollowingWarp),
                ev(20, 12, 0, GamelogEventKind::CombatAny),
            ],
        );
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 20, 0)];

        let snap = enrich_run(&logs, &sites, 25, None, 156, Some("FC Pilot"), None);

        let first = &snap.sites[0];
        assert_eq!(first.warp_seconds, 0);
        assert_eq!(first.combat_to_payout_seconds, None);
        assert!(!first.is_break);
    }

    /// Straggler still fighting the previous grid must not zero the fleet warp.
    #[test]
    fn straggler_combat_does_not_end_warper_segment() {
        let warper = listener_log(
            "Warper",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 30, GamelogEventKind::Regrouping),
                ev(20, 3, 0, GamelogEventKind::CombatHit),
            ],
        );
        let straggler = listener_log(
            "Straggler",
            vec![ev(20, 1, 1, GamelogEventKind::CombatAny)],
        );
        let snap = enrich_run(
            &[warper, straggler],
            &[ts(20, 0, 0), ts(20, 8, 0)],
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("Warper"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 90);
        assert_eq!(site.combat_to_payout_seconds, Some(300));
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn fleet_same_jump_two_listeners_counts_one_warp() {
        // Both warp at 20:00:20; FC lands via Regrouping 20:02:30; Alt CombatHit 20:03:00; payout 20:08:00
        // Prev site 20:00:00. Warp = 130s (not 260). Source Fleet. Combat→payout from Alt hit = 300s.
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 0, 20, GamelogEventKind::FollowingWarp),
                ev(20, 2, 30, GamelogEventKind::Regrouping),
            ],
        );
        let alt = listener_log(
            "Alt Pilot",
            vec![
                ev(20, 0, 21, GamelogEventKind::FollowingWarp), // within 10s → debounce
                ev(20, 3, 0, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[fc, alt],
            &[ts(20, 0, 0), ts(20, 8, 0)],
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 130);
        assert_eq!(site.combat_to_payout_seconds, Some(300));
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn combat_any_does_not_start_combat_to_payout() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::Regrouping),
                ev(20, 2, 30, GamelogEventKind::CombatAny), // incoming — must NOT start clock
            ],
        );
        let snap = enrich_run(
            &[fc],
            &[ts(20, 0, 0), ts(20, 8, 0)],
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 60);
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn combat_from_non_fc_hit_when_fc_quiet() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::Regrouping),
            ],
        );
        let alt = listener_log(
            "Alt Pilot",
            vec![ev(20, 3, 0, GamelogEventKind::CombatHit)],
        );
        let snap = enrich_run(
            &[fc, alt],
            &[ts(20, 0, 0), ts(20, 8, 0)],
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.warp_seconds, 60);
        assert_eq!(site.combat_to_payout_seconds, Some(300));
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn warp_start_outside_debounce_is_second_segment() {
        // Accepted starts 20:01:00 and 20:01:15 (≥10s) — two segments if each ends before the next
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 1, 5, GamelogEventKind::Regrouping),
                ev(20, 1, 15, GamelogEventKind::FollowingWarp),
                ev(20, 1, 40, GamelogEventKind::Regrouping),
                ev(20, 2, 0, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[fc],
            &[ts(20, 0, 0), ts(20, 5, 0)],
            25,
            Some(ts(19, 55, 0)),
            156,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        // 5s + 25s = 30s warp
        assert_eq!(site.warp_seconds, 30);
        assert_eq!(site.combat_to_payout_seconds, Some(180)); // 20:05 - 20:02
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn resolve_fc_prefers_settings_name() {
        let a = listener_log("Alice", vec![]);
        let b = listener_log("Bob", vec![]);
        let logs = vec![a, b];
        assert_eq!(
            resolve_fc(&logs, Some("Bob"), None).as_deref(),
            Some("Bob")
        );
    }

    #[test]
    fn resolve_fc_falls_back_to_wallet_hint_then_fewest_warps() {
        let a = listener_log(
            "Alice",
            vec![ev(20, 0, 0, GamelogEventKind::FollowingWarp)],
        );
        let b = listener_log("Bob", vec![]);
        let logs = vec![a.clone(), b.clone()];

        assert_eq!(
            resolve_fc(&logs, None, Some("Bob")).as_deref(),
            Some("Bob")
        );
        assert_eq!(
            resolve_fc(&logs, None, Some("Nobody")).as_deref(),
            Some("Bob"),
            "falls back to fewest following_warp events"
        );
    }
}
