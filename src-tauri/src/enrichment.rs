//! FC resolution, approach/combat→payout timing, and dead-missile math.
//!
//! See docs/superpowers/specs/2026-08-03-approach-residual-timing-design.md
//! for the authoritative rules this module implements. Approach time is a
//! residual: `site_duration - combat_to_payout`, measured only once the
//! fleet has actually warped in (first `FollowingWarp` in the gap). Combat
//! itself is timed from the first `CombatHit` at or after that warp anchor,
//! not from any warp *end* marker — there is no more segment/warp-end math.

use chrono::{DateTime, Utc};

use crate::analytics_types::{
    Diagnostic, EnrichmentSite, EnrichmentSnapshot, EnrichmentSource, EnrichmentTotals,
    MissileStat,
};
use crate::gamelog_parse::{GamelogEvent, GamelogEventKind};
use crate::gamelog_scan::ListenerLog;

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

/// Pooled fleet events (across every listener) in `(gap_start, gap_end]`.
fn pooled_events_in_gap<'a>(
    logs: &'a [ListenerLog],
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
) -> Vec<&'a GamelogEvent> {
    logs.iter()
        .flat_map(|l| events_in_gap(l, gap_start, gap_end))
        .collect()
}

/// Align a single payout gap that has a known wallet `duration` into
/// approach/combat→payout seconds.
///
/// The warp anchor is the earliest `FollowingWarp` anywhere in the gap; if
/// none exists the gap can't be aligned to a fleet warp at all, so both
/// approach and combat are null and the row falls back to `Heuristic`.
/// Once a warp anchor exists, combat→payout is the earliest `CombatHit` at
/// or after that anchor (across the pooled fleet), converted to
/// seconds-until-payout, and approach is the residual `duration - combat`
/// (null if that residual would be negative).
fn align_gap(
    logs: &[ListenerLog],
    gap_start: DateTime<Utc>,
    payout: DateTime<Utc>,
    duration: i64,
) -> (Option<i64>, Option<i64>, EnrichmentSource) {
    let events = pooled_events_in_gap(logs, gap_start, payout);

    let warp_anchor = events
        .iter()
        .filter(|e| e.kind == GamelogEventKind::FollowingWarp)
        .map(|e| e.occurred_at)
        .min();

    let Some(warp_anchor) = warp_anchor else {
        return (None, None, EnrichmentSource::Heuristic);
    };

    let combat = events
        .iter()
        .filter(|e| {
            e.kind == GamelogEventKind::CombatHit
                && e.occurred_at >= warp_anchor
                && e.occurred_at < payout
        })
        .min_by_key(|e| e.occurred_at)
        .map(|e| (payout - e.occurred_at).num_seconds().max(0));

    let approach = combat.and_then(|c| {
        let residual = duration - c;
        if residual >= 0 {
            Some(residual)
        } else {
            None
        }
    });

    (approach, combat, EnrichmentSource::Fleet)
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
fn missile_stats(
    logs: &[ListenerLog],
    missiles_per_cycle: u32,
    launchers: u32,
) -> Vec<MissileStat> {
    logs.iter()
        .map(|l| {
            let reload_cycles =
                l.events.iter().filter(|e| e.kind == GamelogEventKind::Reload).count() as u32;
            let hits = l
                .events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::CombatHit)
                .count() as u32;
            let ammo_per_launcher = if launchers == 0 {
                0
            } else {
                missiles_per_cycle / launchers
            };
            let expended = reload_cycles.saturating_mul(ammo_per_launcher);
            let dead_volleys = expended.saturating_sub(hits);
            let dead = dead_volleys.saturating_mul(launchers);
            MissileStat {
                listener: l.listener.clone(),
                reload_cycles,
                hits,
                missiles_per_cycle,
                launchers,
                dead,
            }
        })
        .collect()
}

fn missile_stats_in_gap(
    logs: &[ListenerLog],
    missiles_per_cycle: u32,
    launchers: u32,
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
) -> Vec<MissileStat> {
    logs.iter()
        .map(|l| {
            let events = events_in_gap(l, gap_start, gap_end);
            let reload_cycles = events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::Reload)
                .count() as u32;
            let hits = events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::CombatHit)
                .count() as u32;
            let ammo_per_launcher = if launchers == 0 {
                0
            } else {
                missiles_per_cycle / launchers
            };
            let expended = reload_cycles.saturating_mul(ammo_per_launcher);
            let dead_volleys = expended.saturating_sub(hits);
            let dead = dead_volleys.saturating_mul(launchers);
            MissileStat {
                listener: l.listener.clone(),
                reload_cycles,
                hits,
                missiles_per_cycle,
                launchers,
                dead,
            }
        })
        .collect()
}

/// Enrich a sealed run's wallet sites with gamelog-derived approach/combat→payout
/// splits and per-character dead-missile counts.
///
/// `site_durations` is parallel to `site_times`: the wallet-reported
/// `duration_seconds` for each site (`None` marks a break or otherwise
/// non-countable site).
pub fn enrich_run(
    logs: &[ListenerLog],
    site_times: &[DateTime<Utc>],
    site_durations: &[Option<i64>],
    break_threshold_minutes: u32,
    run_start: Option<DateTime<Utc>>,
    missiles_per_cycle: u32,
    launchers: u32,
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
    // (approach_seconds, combat_to_payout_seconds) for sites that count toward averages.
    let mut counted: Vec<(Option<i64>, Option<i64>)> = Vec::new();

    for (i, &occurred_at) in site_times.iter().enumerate() {
        let gap_start = if i == 0 {
            run_start
        } else {
            Some(site_times[i - 1])
        };
        let duration = site_durations.get(i).copied().flatten();

        let (approach_seconds, combat_to_payout_seconds, is_break, source) = match gap_start {
            None => {
                // v1 parity: first site with no run_start has no alignable
                // open bound. It still counts for ISK/LP upstream, but is
                // excluded from approach/combat→payout averages here.
                (None, None, false, EnrichmentSource::Heuristic)
            }
            Some(gap_start) => match duration {
                None => {
                    // Break or otherwise non-countable site: only the gap
                    // length (against the break threshold) decides is_break.
                    let gap_seconds = (occurred_at - gap_start).num_seconds().max(0);
                    let break_secs = i64::from(break_threshold_minutes) * 60;
                    let is_break = gap_seconds > break_secs;
                    (None, None, is_break, EnrichmentSource::Heuristic)
                }
                Some(duration) => {
                    let (approach, combat, source) = align_gap(logs, gap_start, occurred_at, duration);
                    (approach, combat, false, source)
                }
            },
        };

        if !is_break {
            counted.push((approach_seconds, combat_to_payout_seconds));
        }

        let site_missiles = match gap_start {
            Some(gs) if !is_break => {
                missile_stats_in_gap(logs, missiles_per_cycle, launchers, gs, occurred_at)
            }
            _ => Vec::new(),
        };

        sites.push(EnrichmentSite {
            occurred_at,
            approach_seconds,
            combat_to_payout_seconds,
            is_break,
            source,
            missiles: site_missiles,
        });
    }

    let missiles = missile_stats(logs, missiles_per_cycle, launchers);
    let fleet_dead: u32 = missiles.iter().map(|m| m.dead).sum();

    let approach_values: Vec<i64> = counted.iter().filter_map(|(a, _)| *a).collect();
    let total_approach: Option<i64> = if approach_values.is_empty() {
        None
    } else {
        Some(approach_values.iter().sum())
    };
    let combat_values: Vec<i64> = counted.iter().filter_map(|(_, c)| *c).collect();
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
            approach_seconds: total_approach,
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

    #[test]
    fn approach_is_duration_minus_combat_after_following_warp() {
        // gap 20:00 → 20:10 (duration 600). FollowingWarp 20:01:00. CombatHit 20:02:30.
        // combat = 450. approach = 150.
        let pilot = listener_log(
            "Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 30, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[pilot],
            &[ts(20, 0, 0), ts(20, 10, 0)],
            &[None, Some(600)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            Some("Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, Some(450));
        assert_eq!(site.approach_seconds, Some(150));
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    #[test]
    fn combat_hit_before_following_warp_ignored() {
        let pilot = listener_log(
            "Pilot",
            vec![
                ev(20, 0, 30, GamelogEventKind::CombatHit),
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[pilot],
            &[ts(20, 0, 0), ts(20, 10, 0)],
            &[None, Some(600)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, Some(480)); // 20:10 - 20:02
        assert_eq!(site.approach_seconds, Some(120));
    }

    #[test]
    fn no_following_warp_null_combat_and_approach() {
        let pilot = listener_log(
            "Pilot",
            vec![ev(20, 2, 0, GamelogEventKind::CombatHit)],
        );
        let snap = enrich_run(
            &[pilot],
            &[ts(20, 0, 0), ts(20, 10, 0)],
            &[None, Some(600)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.approach_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    #[test]
    fn break_duration_none_nulls_approach_and_combat() {
        let pilot = listener_log(
            "Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[pilot],
            &[ts(20, 0, 0), ts(20, 40, 0)],
            &[None, None],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.approach_seconds, None);
    }

    #[test]
    fn negative_residual_yields_null_approach() {
        // payout 20:10, hit 20:01:40 → combat 500; duration 100 → approach null
        let pilot = listener_log(
            "Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 1, 40, GamelogEventKind::CombatHit),
            ],
        );
        let snap = enrich_run(
            &[pilot],
            &[ts(20, 0, 0), ts(20, 10, 0)],
            &[None, Some(100)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, Some(500));
        assert_eq!(site.approach_seconds, None);
    }

    #[test]
    fn no_markers_long_gap_is_break() {
        let fc = listener_log("FC Pilot", vec![]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 40, 0)];

        let snap = enrich_run(
            &logs,
            &sites,
            &[None, None],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );

        let site = &snap.sites[1];
        assert!(site.is_break);
        assert_eq!(site.approach_seconds, None);
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    #[test]
    fn no_markers_short_gap_not_break_null_combat_without_fc_events() {
        let fc = listener_log("FC Pilot", vec![]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 10, 0)];

        let snap = enrich_run(
            &logs,
            &sites,
            &[None, None],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );

        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.approach_seconds, None);
        assert_eq!(site.combat_to_payout_seconds, None);
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

        let snap = enrich_run(
            &logs,
            &sites,
            &[None],
            25,
            Some(ts(20, 0, 0)),
            156,
            6,
            None,
            None,
        );

        assert_eq!(snap.missiles.len(), 1);
        let m = &snap.missiles[0];
        assert_eq!(m.reload_cycles, 2);
        assert_eq!(m.hits, 100);
        assert_eq!(m.missiles_per_cycle, 156);
        // expended = 2×26 = 52; dead_volleys = 0; dead = 0×6 = 0
        assert_eq!(m.dead, 0);
        assert_eq!(snap.totals.fleet_dead, 0);
    }

    #[test]
    fn dead_missiles_is_dead_volleys_times_launchers() {
        let listener = listener_log(
            "Gunner",
            vec![
                ev(20, 1, 0, GamelogEventKind::Reload),
                ev(20, 5, 0, GamelogEventKind::Reload),
            ]
            .into_iter()
            .chain((0..35).map(|i| ev(20, 2, i % 60, GamelogEventKind::CombatHit)))
            .collect(),
        );
        let snap = enrich_run(
            &[listener],
            &[ts(20, 8, 0)],
            &[None],
            25,
            Some(ts(20, 0, 0)),
            156,
            6,
            None,
            None,
        );
        let m = &snap.missiles[0];
        // dead_volleys = 52 − 35 = 17; dead = 17 × 6 = 102
        assert_eq!(m.dead, 102);
        assert_eq!(snap.totals.fleet_dead, 102);
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

        let snap = enrich_run(
            &logs,
            &sites,
            &[None, Some(600)],
            25,
            None,
            156,
            6,
            Some("FC Pilot"),
            None,
        );

        let first = &snap.sites[0];
        assert_eq!(first.approach_seconds, None);
        assert_eq!(first.combat_to_payout_seconds, None);
        assert!(!first.is_break);

        let second = &snap.sites[1];
        assert_eq!(second.approach_seconds, Some(120));
        assert_eq!(second.combat_to_payout_seconds, Some(480));

        // Averages must only reflect the second (alignable) site.
        assert_eq!(snap.totals.avg_combat_to_payout_seconds, Some(480.0));
        assert_eq!(snap.totals.approach_seconds, Some(120));
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

        let snap = enrich_run(
            &logs,
            &sites,
            &[None, None],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );

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

    /// Plan Task 2: combat is null both when the gap is a break and when the
    /// gap has a warp anchor but the fleet never logs a `CombatHit` after it.
    #[test]
    fn combat_null_on_break_and_when_no_combat() {
        let fc = listener_log("FC Pilot", vec![]);
        let break_logs = vec![fc];
        let break_sites = vec![ts(20, 0, 0), ts(20, 40, 0)];
        let break_snap = enrich_run(
            &break_logs,
            &break_sites,
            &[None, None],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            None,
            None,
        );
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
        let snap = enrich_run(
            &logs,
            &sites,
            &[None, Some(480)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.approach_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    /// The first site with no leading `run_start` has no alignable open
    /// bound, so it gets null approach/combat regardless of what the logs
    /// contain.
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

        let snap = enrich_run(&logs, &sites, &[None, None], 25, None, 156, 6, Some("FC Pilot"), None);

        let first = &snap.sites[0];
        assert_eq!(first.approach_seconds, None);
        assert_eq!(first.combat_to_payout_seconds, None);
        assert!(!first.is_break);
    }

    /// `CombatAny` (incoming) never satisfies the combat anchor — only an
    /// outgoing `CombatHit` starts the payout countdown.
    #[test]
    fn combat_any_does_not_start_combat_to_payout() {
        let fc = listener_log(
            "FC Pilot",
            vec![
                ev(20, 1, 0, GamelogEventKind::FollowingWarp),
                ev(20, 2, 0, GamelogEventKind::Regrouping),
                ev(20, 2, 30, GamelogEventKind::CombatAny), // incoming — must NOT count
            ],
        );
        let snap = enrich_run(
            &[fc],
            &[ts(20, 0, 0), ts(20, 8, 0)],
            &[None, Some(400)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, None);
        assert_eq!(site.approach_seconds, None);
        assert_eq!(site.source, EnrichmentSource::Fleet);
    }

    /// Combat→payout is pooled across the fleet: a non-FC listener's
    /// `CombatHit` after the FC's warp anchor still starts the clock.
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
            &[None, Some(360)],
            25,
            Some(ts(19, 55, 0)),
            156,
            6,
            Some("FC Pilot"),
            None,
        );
        let site = &snap.sites[1];
        assert_eq!(site.combat_to_payout_seconds, Some(300));
        assert_eq!(site.approach_seconds, Some(60));
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

    #[test]
    fn site_missiles_count_only_events_in_gap() {
        // Site0 payout 20:10, site1 payout 20:20. run_start 20:00.
        // Gap1 = (20:10, 20:20]: reload+50 hits inside; reload+hits in gap0 must not appear on site1.
        let gunner = listener_log(
            "Gunner",
            vec![
                ev(20, 5, 0, GamelogEventKind::Reload),
                ev(20, 6, 0, GamelogEventKind::CombatHit),
                ev(20, 12, 0, GamelogEventKind::Reload),
            ]
            .into_iter()
            .chain((0..50).map(|i| ev(20, 15, i % 60, GamelogEventKind::CombatHit)))
            .collect(),
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0), ts(20, 20, 0)],
            &[Some(600), Some(600)],
            25,
            Some(ts(20, 0, 0)),
            156,
            6,
            None,
            None,
        );

        assert!(snap.sites[0].missiles.iter().any(|m| m.listener == "Gunner"));
        let s0 = snap.sites[0]
            .missiles
            .iter()
            .find(|m| m.listener == "Gunner")
            .unwrap();
        assert_eq!(s0.reload_cycles, 1);
        assert_eq!(s0.hits, 1);

        let s1 = snap.sites[1]
            .missiles
            .iter()
            .find(|m| m.listener == "Gunner")
            .unwrap();
        assert_eq!(s1.reload_cycles, 1);
        assert_eq!(s1.hits, 50);
        // expended = 1 × (156/6) = 26; dead = max(0, 26 − 50) = 0
        assert_eq!(s1.dead, 0);
    }

    #[test]
    fn break_site_has_empty_missiles_but_run_totals_keep_break_gap_events() {
        // Short site then long break (>25m). Reload during break gap still in run-level missiles.
        let gunner = listener_log(
            "Gunner",
            vec![
                ev(20, 5, 0, GamelogEventKind::Reload),
                ev(20, 40, 0, GamelogEventKind::Reload), // inside break gap after 20:10
            ],
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0), ts(21, 0, 0)],
            &[Some(600), None], // second site non-countable -> break if gap > threshold
            25,
            Some(ts(20, 0, 0)),
            156,
            6,
            None,
            None,
        );

        assert!(snap.sites[1].is_break);
        assert!(snap.sites[1].missiles.is_empty());
        let run = snap.missiles.iter().find(|m| m.listener == "Gunner").unwrap();
        assert_eq!(run.reload_cycles, 2);
    }

    #[test]
    fn unalignable_first_site_has_empty_missiles() {
        let gunner = listener_log(
            "Gunner",
            vec![ev(20, 1, 0, GamelogEventKind::Reload)],
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0)],
            &[Some(600)],
            25,
            None, // no run_start -> no gap_start
            156,
            6,
            None,
            None,
        );
        assert!(snap.sites[0].missiles.is_empty());
    }
}
