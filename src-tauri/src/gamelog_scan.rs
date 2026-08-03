//! Scan EVE Gamelogs for files overlapping a wallet run time window.

use crate::encoding::read_chatlog;
use crate::gamelog_parse::{
    parse_gamelog_events, parse_listener, parse_session_started, GamelogEvent,
};
use chrono::{DateTime, Days, Duration, Local, Utc};
use std::fs;
use std::path::{Path, PathBuf};

/// Small slack past `wallet_end` for session-start clock skew.
const SESSION_END_SLACK: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerLog {
    pub listener: String,
    pub path: PathBuf,
    pub events: Vec<GamelogEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub logs: Vec<ListenerLog>,
    pub diagnostics: Vec<String>,
}

/// Default EVE Gamelogs directory under the user's Documents folder.
pub fn default_gamelogs_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("EVE")
        .join("logs")
        .join("Gamelogs")
}

/// Expand wallet start by one local calendar day (UTC vs local filename skew).
fn expanded_wallet_start(wallet_start: DateTime<Utc>) -> DateTime<Utc> {
    let local = wallet_start.with_timezone(&Local);
    (local - Days::new(1)).with_timezone(&Utc)
}

/// Scan `dir` (non-recursive) for gamelog files whose session overlaps
/// `[wallet_start − 1 local day, wallet_end + slack]`.
pub fn scan_gamelogs(
    dir: &Path,
    wallet_start: DateTime<Utc>,
    wallet_end: DateTime<Utc>,
) -> ScanResult {
    let mut diagnostics = Vec::new();
    let mut logs = Vec::new();

    if !dir.is_dir() {
        diagnostics.push(format!(
            "Gamelogs directory missing or not a directory: {}",
            dir.display()
        ));
        return ScanResult { logs, diagnostics };
    }

    let expanded_start = expanded_wallet_start(wallet_start);
    let end_bound = wallet_end + SESSION_END_SLACK;

    let mut entries: Vec<(PathBuf, Option<std::time::SystemTime>)> = Vec::new();
    match fs::read_dir(dir) {
        Ok(rd) => {
            for entry in rd {
                match entry {
                    Ok(e) => {
                        let path = e.path();
                        if !path.is_file() {
                            continue;
                        }
                        let mtime = e.metadata().ok().and_then(|m| m.modified().ok());
                        entries.push((path, mtime));
                    }
                    Err(err) => diagnostics.push(format!("Failed to read directory entry: {err}")),
                }
            }
        }
        Err(err) => {
            diagnostics.push(format!(
                "Failed to list Gamelogs directory {}: {err}",
                dir.display()
            ));
            return ScanResult { logs, diagnostics };
        }
    }

    // Newest → oldest (mtime descending; missing mtime sorts last).
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));

    for (path, _) in entries {
        let text = match read_chatlog(&path) {
            Ok(t) => t,
            Err(err) => {
                diagnostics.push(format!(
                    "Skipped unreadable gamelog {}: {err}",
                    path.display()
                ));
                continue;
            }
        };

        let Some(session) = parse_session_started(&text) else {
            diagnostics.push(format!(
                "Skipped gamelog without Session Started: {}",
                path.display()
            ));
            continue;
        };

        // Outside expanded window (correctness first: no early-stop on mtime order).
        if session < expanded_start || session > end_bound {
            continue;
        }

        let Some(listener) = parse_listener(&text) else {
            diagnostics.push(format!(
                "Skipped gamelog without Listener: {}",
                path.display()
            ));
            continue;
        };

        let events = parse_gamelog_events(&text);
        logs.push(ListenerLog {
            listener,
            path,
            events,
        });
    }

    ScanResult { logs, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;
    use std::time::{Duration as StdDuration, SystemTime};
    use tempfile::tempdir;

    fn gamelog_body(listener: &str, session: &str) -> String {
        format!(
            "---------------------------------------------------------------\n\
  Gamelog\n\
  Listener:        {listener}\n\
  Session Started: {session}\n\
---------------------------------------------------------------\n\
\n\
[ {session} ] (notify) Following Fleet Commander in warp\n"
        )
    }

    fn write_gamelog(dir: &Path, name: &str, listener: &str, session: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, gamelog_body(listener, session)).unwrap();
        path
    }

    #[test]
    fn default_gamelogs_dir_ends_with_gamelogs() {
        let p = default_gamelogs_dir();
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("Gamelogs"));
    }

    #[test]
    fn scan_includes_only_files_overlapping_wallet_window() {
        let dir = tempdir().unwrap();
        let in_window = write_gamelog(
            dir.path(),
            "20260802_180000.txt",
            "Pilot In Window",
            "2026.08.02 18:00:00",
        );
        // Far older than expanded start (wallet_start − 1 day).
        write_gamelog(
            dir.path(),
            "20260701_120000.txt",
            "Pilot Too Old",
            "2026.07.01 12:00:00",
        );

        // Touch so newest→oldest prefers the in-window file first if sorted by mtime.
        let newer = SystemTime::now();
        let older = newer - StdDuration::from_secs(60);
        let _ = filetime_set(&in_window, newer);
        let _ = filetime_set(&dir.path().join("20260701_120000.txt"), older);

        let wallet_start = Utc.with_ymd_and_hms(2026, 8, 2, 19, 0, 0).unwrap();
        let wallet_end = Utc.with_ymd_and_hms(2026, 8, 2, 22, 0, 0).unwrap();

        let result = scan_gamelogs(dir.path(), wallet_start, wallet_end);

        assert_eq!(result.logs.len(), 1, "diagnostics: {:?}", result.diagnostics);
        assert_eq!(result.logs[0].listener, "Pilot In Window");
        assert_eq!(result.logs[0].path, in_window);
        assert!(!result.logs[0].events.is_empty());
    }

    #[test]
    fn scan_missing_dir_emits_diagnostic() {
        let missing = PathBuf::from("C:/definitely/missing/incfleetchat_gamelogs_xyz");
        let wallet_start = Utc.with_ymd_and_hms(2026, 8, 2, 19, 0, 0).unwrap();
        let wallet_end = Utc.with_ymd_and_hms(2026, 8, 2, 22, 0, 0).unwrap();

        let result = scan_gamelogs(&missing, wallet_start, wallet_end);

        assert!(result.logs.is_empty());
        assert!(
            !result.diagnostics.is_empty(),
            "expected diagnostic for missing path"
        );
        assert!(
            result.diagnostics.iter().any(|d| d.to_lowercase().contains("missing")
                || d.to_lowercase().contains("not a directory")
                || d.contains("definitely")),
            "diagnostics: {:?}",
            result.diagnostics
        );
    }

    /// Best-effort mtime set without an extra crate (Windows + Unix).
    fn filetime_set(path: &Path, at: SystemTime) -> std::io::Result<()> {
        fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .set_modified(at)
    }
}
