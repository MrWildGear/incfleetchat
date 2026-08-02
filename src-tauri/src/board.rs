use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;

use crate::site_id::site_id;
use crate::types::{
    Board, BoardStatus, SiteCandidate, SitePhase, SiteRow, TIMER_MINUTES,
};

pub fn build_board(
    status: BoardStatus,
    fleet_log_id: &str,
    candidates: &[SiteCandidate],
    ran_ids: &HashSet<String>,
    cleared_ids: &HashSet<String>,
    now: DateTime<Utc>,
) -> Board {
    let mut sites: Vec<SiteRow> = candidates
        .iter()
        .filter_map(|c| {
            let id = site_id(fleet_log_id, c);
            if cleared_ids.contains(&id) {
                return None;
            }
            let expires_at = c.posted_at + Duration::minutes(TIMER_MINUTES);
            let ran = ran_ids.contains(&id);
            let expired = now >= expires_at;
            let (phase, clearable) = if expired && ran {
                (SitePhase::Ready, true)
            } else if expired {
                (SitePhase::Overdue, false)
            } else {
                (SitePhase::Active, false)
            };
            Some(SiteRow {
                id,
                tag: c.tag.clone(),
                speaker: c.speaker.clone(),
                posted_at: c.posted_at,
                expires_at,
                ran,
                phase,
                clearable,
            })
        })
        .collect();

    sites.sort_by(|a, b| a.expires_at.cmp(&b.expires_at).then_with(|| a.id.cmp(&b.id)));

    let ready_count = sites.iter().filter(|s| s.clearable).count() as u32;

    Board {
        status,
        sites,
        ready_count,
        updated_at: now,
    }
}

pub fn mark_ran(board: &mut Board, site_id: &str, ran_ids: &mut HashSet<String>, now: DateTime<Utc>) {
    ran_ids.insert(site_id.to_string());
    if let Some(row) = board.sites.iter_mut().find(|s| s.id == site_id) {
        row.ran = true;
        let expired = now >= row.expires_at;
        if expired {
            row.phase = SitePhase::Ready;
            row.clearable = true;
        }
    }
    board.ready_count = board.sites.iter().filter(|s| s.clearable).count() as u32;
    board.updated_at = now;
}

pub fn clear_site(
    board: &mut Board,
    site_id: &str,
    cleared_ids: &mut HashSet<String>,
    now: DateTime<Utc>,
) {
    let clearable = board
        .sites
        .iter()
        .find(|s| s.id == site_id)
        .map(|s| s.clearable)
        .unwrap_or(false);
    if !clearable {
        board.updated_at = now;
        return;
    }
    cleared_ids.insert(site_id.to_string());
    board.sites.retain(|s| s.id != site_id);
    board.ready_count = board.sites.iter().filter(|s| s.clearable).count() as u32;
    board.updated_at = now;
}

pub fn clear_ready(board: &mut Board, cleared_ids: &mut HashSet<String>, now: DateTime<Utc>) {
    let ready: Vec<String> = board
        .sites
        .iter()
        .filter(|s| s.clearable)
        .map(|s| s.id.clone())
        .collect();
    for id in &ready {
        cleared_ids.insert(id.clone());
    }
    board.sites.retain(|s| !s.clearable);
    board.ready_count = 0;
    board.updated_at = now;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn candidate(tag: &str, hour: u32, minute: u32, ordinal: u32) -> SiteCandidate {
        SiteCandidate {
            speaker: "Pilot".into(),
            posted_at: Utc.with_ymd_and_hms(2026, 8, 1, hour, minute, 0).unwrap(),
            tag: tag.into(),
            line_ordinal: ordinal,
        }
    }

    #[test]
    fn active_overdue_and_ready_phases() {
        let log = "fleet.log";
        let c_active = candidate("1", 12, 0, 0);
        let c_over = candidate("2", 11, 0, 1);
        let c_ready = candidate("3", 11, 0, 2);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 11, 30, 0).unwrap();
        // active: posted 12:00, expires 12:20 — wait, now is 11:30, so 12:00 is in the future = active
        // overdue: posted 11:00 expires 11:20, now 11:30, not ran
        // ready: same time but ran

        let id_ready = site_id(log, &c_ready);
        let mut ran = HashSet::new();
        ran.insert(id_ready);
        let board = build_board(
            BoardStatus::Watching {
                character: "X".into(),
                log_name: log.into(),
            },
            log,
            &[c_active, c_over, c_ready],
            &ran,
            &HashSet::new(),
            now,
        );
        assert_eq!(board.sites.len(), 3);
        let by_tag = |t: &str| board.sites.iter().find(|s| s.tag == t).unwrap();
        assert_eq!(by_tag("1").phase, SitePhase::Active);
        assert_eq!(by_tag("2").phase, SitePhase::Overdue);
        assert!(!by_tag("2").clearable);
        assert_eq!(by_tag("3").phase, SitePhase::Ready);
        assert!(by_tag("3").clearable);
        assert_eq!(board.ready_count, 1);
        // soonest expiry first: 11:20 rows before 12:20
        assert_eq!(board.sites[0].expires_at < board.sites[2].expires_at, true);
    }

    #[test]
    fn mark_ran_then_clear_site_only_when_ready() {
        let log = "fleet.log";
        let c = candidate("a", 11, 0, 0);
        let id = site_id(log, &c);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 11, 30, 0).unwrap();
        let mut ran = HashSet::new();
        let mut cleared = HashSet::new();
        let mut board = build_board(
            BoardStatus::NoCharacter,
            log,
            &[c],
            &ran,
            &cleared,
            now,
        );
        assert_eq!(board.sites[0].phase, SitePhase::Overdue);

        clear_site(&mut board, &id, &mut cleared, now);
        assert_eq!(board.sites.len(), 1); // not clearable yet

        mark_ran(&mut board, &id, &mut ran, now);
        assert_eq!(board.sites[0].phase, SitePhase::Ready);
        assert_eq!(board.ready_count, 1);

        clear_site(&mut board, &id, &mut cleared, now);
        assert!(board.sites.is_empty());
        assert!(cleared.contains(&id));
    }

    #[test]
    fn clear_ready_bulk() {
        let log = "fleet.log";
        let c1 = candidate("1", 11, 0, 0);
        let c2 = candidate("2", 11, 5, 1);
        let c3 = candidate("3", 12, 0, 2); // still active
        let id1 = site_id(log, &c1);
        let id2 = site_id(log, &c2);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 11, 30, 0).unwrap();
        let ran = HashSet::from([id1.clone(), id2.clone()]);
        let mut cleared = HashSet::new();
        let mut board = build_board(
            BoardStatus::NoCharacter,
            log,
            &[c1, c2, c3],
            &ran,
            &cleared,
            now,
        );
        assert_eq!(board.ready_count, 2);
        clear_ready(&mut board, &mut cleared, now);
        assert_eq!(board.sites.len(), 1);
        assert_eq!(board.sites[0].tag, "3");
        assert_eq!(board.ready_count, 0);
    }

    #[test]
    fn cleared_ids_hidden_on_rebuild() {
        let log = "fleet.log";
        let c = candidate("1", 11, 0, 0);
        let id = site_id(log, &c);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 11, 30, 0).unwrap();
        let ran = HashSet::from([id.clone()]);
        let cleared = HashSet::from([id]);
        let board = build_board(
            BoardStatus::NoCharacter,
            log,
            &[c],
            &ran,
            &cleared,
            now,
        );
        assert!(board.sites.is_empty());
    }
}
