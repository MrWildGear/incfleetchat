//! FC resolution, warp-vs-in-site alignment, and dead-missile math.
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

/// Earliest `after`-exclusive event of a given predicate across a flat pool.
fn earliest_after<'a>(
    pool: &[&'a GamelogEvent],
    after: DateTime<Utc>,
    pred: impl Fn(GamelogEventKind) -> bool,
) -> Option<&'a GamelogEvent> {
    pool.iter()
        .filter(|e| pred(e.kind) && e.occurred_at > after)
        .min_by_key(|e| e.occurred_at)
        .copied()
}

struct WarpStart {
    at: DateTime<Utc>,
    source: EnrichmentSource,
}

/// Align a single payout gap into warp/in-site seconds.
fn align_gap(
    logs: &[ListenerLog],
    resolved_fc: Option<&str>,
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
    break_threshold_minutes: u32,
) -> (i64, i64, bool, EnrichmentSource) {
    let fc_log = resolved_fc.and_then(|fc| logs.iter().find(|l| l.listener == fc));

    let fc_gap_events: Vec<&GamelogEvent> = fc_log
        .map(|l| events_in_gap(l, gap_start, gap_end))
        .unwrap_or_default();

    let fc_warp_starts: Vec<DateTime<Utc>> = {
        let mut v: Vec<DateTime<Utc>> = fc_gap_events
            .iter()
            .filter(|e| e.kind == GamelogEventKind::FollowingWarp)
            .map(|e| e.occurred_at)
            .collect();
        v.sort();
        v
    };

    // Flat pool of every listener's events in the gap (used for borrowing and
    // for the "any listener" end-marker fallback).
    let all_gap_events: Vec<&GamelogEvent> = logs
        .iter()
        .flat_map(|l| events_in_gap(l, gap_start, gap_end))
        .collect();

    let warp_starts: Vec<WarpStart> = if !fc_warp_starts.is_empty() {
        fc_warp_starts
            .into_iter()
            .map(|at| WarpStart {
                at,
                source: EnrichmentSource::Fc,
            })
            .collect()
    } else {
        let borrowed = all_gap_events
            .iter()
            .filter(|e| e.kind == GamelogEventKind::FollowingWarp)
            .min_by_key(|e| e.occurred_at)
            .map(|e| e.occurred_at);
        match borrowed {
            Some(at) => vec![WarpStart {
                at,
                source: EnrichmentSource::Borrowed,
            }],
            None => Vec::new(),
        }
    };

    if warp_starts.is_empty() {
        let gap_seconds = (gap_end - gap_start).num_seconds().max(0);
        let break_secs = i64::from(break_threshold_minutes) * 60;
        let is_break = gap_seconds > break_secs;
        let in_site = if is_break { 0 } else { gap_seconds };
        return (0, in_site, is_break, EnrichmentSource::Heuristic);
    }

    let mut warp_seconds: i64 = 0;
    let mut in_site_seconds: i64 = 0;
    let last_source = warp_starts.last().unwrap().source;

    for (idx, ws) in warp_starts.iter().enumerate() {
        let next_warp_start = warp_starts.get(idx + 1).map(|w| w.at);

        // Prefer FC's own regroup/combat_any after this warp-start; fall back
        // to any listener's only when FC has none anywhere later in the gap.
        let fc_marker = earliest_after(&fc_gap_events, ws.at, is_end_marker).map(|e| e.occurred_at);
        let marker = if fc_marker.is_some() {
            fc_marker
        } else {
            earliest_after(&all_gap_events, ws.at, is_end_marker).map(|e| e.occurred_at)
        };

        let mut candidates: Vec<DateTime<Utc>> = vec![gap_end];
        if let Some(m) = marker {
            candidates.push(m);
        }
        if let Some(nws) = next_warp_start {
            candidates.push(nws);
        }
        let segment_end = *candidates.iter().min().unwrap();

        warp_seconds += (segment_end - ws.at).num_seconds().max(0);

        // Intervening/tail in-site: from this segment's end to the next
        // warp-start (or to the payout for the final segment).
        let tail_end = next_warp_start.unwrap_or(gap_end);
        if tail_end > segment_end {
            in_site_seconds += (tail_end - segment_end).num_seconds();
        }
    }

    (warp_seconds, in_site_seconds, false, last_source)
}

/// Compute per-listener dead missiles over the whole scanned window.
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

/// Enrich a sealed run's wallet sites with gamelog-derived warp/in-site
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

    let resolved_fc = resolve_fc(logs, fc_character, wallet_fc_hint);

    let mut sites = Vec::with_capacity(site_times.len());
    // (warp_seconds, in_site_seconds) for sites that count toward averages.
    let mut counted: Vec<(i64, i64)> = Vec::new();

    for (i, &occurred_at) in site_times.iter().enumerate() {
        let gap_start = if i == 0 {
            run_start
        } else {
            Some(site_times[i - 1])
        };

        let Some(gap_start) = gap_start else {
            // v1 parity: first site with no run_start has no alignable open
            // bound. It still counts for ISK/LP upstream, but is excluded
            // from warp/in-site averages here.
            sites.push(EnrichmentSite {
                occurred_at,
                warp_seconds: 0,
                in_site_seconds: 0,
                is_break: false,
                source: EnrichmentSource::Heuristic,
            });
            continue;
        };

        let (warp_seconds, in_site_seconds, is_break, source) = align_gap(
            logs,
            resolved_fc.as_deref(),
            gap_start,
            occurred_at,
            break_threshold_minutes,
        );

        if !is_break {
            counted.push((warp_seconds, in_site_seconds));
        }

        sites.push(EnrichmentSite {
            occurred_at,
            warp_seconds,
            in_site_seconds,
            is_break,
            source,
        });
    }

    let missiles = missile_stats(logs, missiles_per_cycle);
    let fleet_dead: u32 = missiles.iter().map(|m| m.dead).sum();

    let total_warp: i64 = counted.iter().map(|(w, _)| w).sum();
    let total_in_site: i64 = counted.iter().map(|(_, s)| s).sum();
    let avg_in_site_seconds = if counted.is_empty() {
        None
    } else {
        Some(total_in_site as f64 / counted.len() as f64)
    };

    EnrichmentSnapshot {
        resolved_fc,
        listeners,
        diagnostics,
        sites,
        missiles,
        totals: EnrichmentTotals {
            warp_seconds: total_warp,
            in_site_seconds: total_in_site,
            avg_in_site_seconds,
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

    /// Worked example from the spec: borrowed warp-start (Alt), FC's first
    /// combat ends the warp → warp 130s, in-site 330s, source borrowed.
    #[test]
    fn worked_example_borrowed_warp_130_330() {
        let fc = listener_log(
            "FC Pilot",
            vec![ev(20, 2, 30, GamelogEventKind::CombatHit)],
        );
        let alt = listener_log(
            "Alt Pilot",
            vec![ev(20, 0, 20, GamelogEventKind::FollowingWarp)],
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
        assert_eq!(site.in_site_seconds, 330);
        assert_eq!(site.source, EnrichmentSource::Borrowed);
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
        assert_eq!(site.in_site_seconds, 0);
        assert_eq!(site.source, EnrichmentSource::Heuristic);
    }

    #[test]
    fn no_markers_short_gap_is_in_site_not_break() {
        let fc = listener_log("FC Pilot", vec![]);
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 10, 0)];

        let snap = enrich_run(&logs, &sites, 25, Some(ts(19, 55, 0)), 156, None, None);

        let site = &snap.sites[1];
        assert!(!site.is_break);
        assert_eq!(site.warp_seconds, 0);
        assert_eq!(site.in_site_seconds, 600);
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
                ev(20, 12, 0, GamelogEventKind::CombatAny),
            ],
        );
        let logs = vec![fc];
        let sites = vec![ts(20, 0, 0), ts(20, 20, 0)];

        let snap = enrich_run(&logs, &sites, 25, None, 156, Some("FC Pilot"), None);

        let first = &snap.sites[0];
        assert_eq!(first.warp_seconds, 0);
        assert_eq!(first.in_site_seconds, 0);
        assert!(!first.is_break);

        let second = &snap.sites[1];
        assert_eq!(second.warp_seconds, 120);
        assert_eq!(second.in_site_seconds, 480);

        // Averages must only reflect the second (alignable) site.
        assert_eq!(snap.totals.avg_in_site_seconds, Some(480.0));
        assert_eq!(snap.totals.warp_seconds, 120);
        assert_eq!(snap.totals.in_site_seconds, 480);
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
