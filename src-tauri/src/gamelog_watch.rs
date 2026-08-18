//! Live follow of the overlay Listener's gamelog for fleet-warp prompts.

use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::encoding::read_chatlog;
use crate::gamelog_parse::{parse_gamelog_events, parse_listener, GamelogEventKind};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WarpWatchCursor {
    pub path: Option<PathBuf>,
    pub last_warp_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpPoll {
    None,
    NewWarp,
}

fn is_gamelog(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("txt"))
        .unwrap_or(false)
}

fn gamelog_stamp(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// Newest gamelog whose Listener matches `character` (filename stamp, then path).
pub fn resolve_active_gamelog(dir: &Path, character: &str) -> std::io::Result<Option<PathBuf>> {
    let mut best: Option<(String, PathBuf)> = None;
    if !dir.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_gamelog(&path) {
            continue;
        }
        let Ok(text) = read_chatlog(&path) else {
            continue;
        };
        let Some(listener) = parse_listener(&text) else {
            continue;
        };
        if !listener.eq_ignore_ascii_case(character) {
            continue;
        }
        let stamp = gamelog_stamp(&path);
        match &best {
            None => best = Some((stamp, path)),
            Some((s, _)) if stamp >= *s => best = Some((stamp, path)),
            _ => {}
        }
    }
    Ok(best.map(|(_, p)| p))
}

pub fn latest_following_warp_at(path: &Path) -> Option<DateTime<Utc>> {
    let text = read_chatlog(path).ok()?;
    parse_gamelog_events(&text)
        .into_iter()
        .filter(|e| e.kind == GamelogEventKind::FollowingWarp)
        .map(|e| e.occurred_at)
        .max()
}

/// First sight of a file (or a new file) only baselines. Later warps after that
/// timestamp are NewWarp so we do not popup for history already in the log.
pub fn poll_following_warp(
    cursor: &mut WarpWatchCursor,
    path: Option<PathBuf>,
    latest_warp: Option<DateTime<Utc>>,
) -> WarpPoll {
    match path {
        None => {
            *cursor = WarpWatchCursor::default();
            WarpPoll::None
        }
        Some(path) => {
            let path_changed = cursor.path.as_ref() != Some(&path);
            if path_changed {
                cursor.path = Some(path);
                cursor.last_warp_at = latest_warp;
                return WarpPoll::None;
            }
            match (latest_warp, cursor.last_warp_at) {
                (Some(latest), Some(prev)) if latest > prev => {
                    cursor.last_warp_at = Some(latest);
                    WarpPoll::NewWarp
                }
                (Some(latest), None) => {
                    cursor.last_warp_at = Some(latest);
                    WarpPoll::NewWarp
                }
                (Some(latest), Some(_)) => {
                    cursor.last_warp_at = Some(latest);
                    WarpPoll::None
                }
                (None, _) => WarpPoll::None
            }
        }
    }
}

pub fn path_in_gamelogs(gamelogs: &Path, changed: &Path) -> bool {
    changed.starts_with(gamelogs)
        || changed
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("txt"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;
    use tempfile::tempdir;

    fn write_gamelog(dir: &Path, name: &str, listener: &str, extra: &str) {
        let body = format!(
            "---------------------------------------------------------------\n\
  Gamelog\n\
  Listener:        {listener}\n\
  Session Started: 2026.08.02 18:00:00\n\
---------------------------------------------------------------\n\
\n{extra}"
        );
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn resolve_picks_newest_matching_listener() {
        let dir = tempdir().unwrap();
        write_gamelog(dir.path(), "20260802_100000.txt", "Alpha Pilot", "");
        write_gamelog(dir.path(), "20260802_110000.txt", "Alpha Pilot", "");
        write_gamelog(dir.path(), "20260802_120000.txt", "Other Pilot", "");

        let path = resolve_active_gamelog(dir.path(), "Alpha Pilot")
            .unwrap()
            .expect("should find");
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("110000"));
    }

    #[test]
    fn latest_following_warp_reads_notify_line() {
        let dir = tempdir().unwrap();
        write_gamelog(
            dir.path(),
            "20260802_180000.txt",
            "Alpha Pilot",
            "[ 2026.08.02 18:21:05 ] (notify) Following Fleet Commander in warp\n",
        );
        let path = dir.path().join("20260802_180000.txt");
        assert_eq!(
            latest_following_warp_at(&path),
            Some(Utc.with_ymd_and_hms(2026, 8, 2, 18, 21, 5).unwrap())
        );
    }

    #[test]
    fn first_poll_baselines_without_prompt() {
        let mut cursor = WarpWatchCursor::default();
        let path = PathBuf::from("20260802_180000.txt");
        let ts = Utc.with_ymd_and_hms(2026, 8, 2, 18, 21, 5).unwrap();
        assert_eq!(
            poll_following_warp(&mut cursor, Some(path.clone()), Some(ts)),
            WarpPoll::None
        );
        assert_eq!(cursor.last_warp_at, Some(ts));
    }

    #[test]
    fn later_warp_prompts() {
        let mut cursor = WarpWatchCursor::default();
        let path = PathBuf::from("20260802_180000.txt");
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 18, 21, 5).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 8, 2, 18, 30, 0).unwrap();
        poll_following_warp(&mut cursor, Some(path.clone()), Some(t0));
        assert_eq!(
            poll_following_warp(&mut cursor, Some(path), Some(t1)),
            WarpPoll::NewWarp
        );
    }

    #[test]
    fn new_file_does_not_prompt_for_existing_warps() {
        let mut cursor = WarpWatchCursor::default();
        let first = PathBuf::from("20260802_180000.txt");
        let second = PathBuf::from("20260802_190000.txt");
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 18, 21, 5).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 8, 2, 19, 5, 0).unwrap();
        poll_following_warp(&mut cursor, Some(first), Some(t0));
        assert_eq!(
            poll_following_warp(&mut cursor, Some(second), Some(t1)),
            WarpPoll::None
        );
    }
}
