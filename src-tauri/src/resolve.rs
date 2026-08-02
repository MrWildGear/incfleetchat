use std::path::{Path, PathBuf};
use std::fs;

use crate::encoding::read_chatlog;
use crate::parse::parse_listener;

/// List unique Listener names found in Fleet_*.txt headers under `dir`.
pub fn list_characters(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    if !dir.is_dir() {
        return Ok(names);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !is_fleet_log(&path) {
            continue;
        }
        let Ok(text) = read_chatlog(&path) else {
            continue;
        };
        if let Some(name) = parse_listener(&text) {
            if !names.iter().any(|n| n == &name) {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

fn is_fleet_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("Fleet_") && n.ends_with(".txt"))
        .unwrap_or(false)
}

/// Session stamp from `Fleet_YYYYMMDD_HHMMSS_*.txt` (lexicographic = chronological).
fn fleet_session_stamp(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// Pick the newest Fleet_*.txt whose Listener matches `character`.
/// Prefer filename session stamp over mtime — EVE often touches many logs at once.
pub fn resolve_active_fleet_log(dir: &Path, character: &str) -> std::io::Result<Option<PathBuf>> {
    let mut best: Option<(String, PathBuf)> = None;
    if !dir.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !is_fleet_log(&path) {
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
        let stamp = fleet_session_stamp(&path);
        match &best {
            None => best = Some((stamp, path)),
            Some((s, _)) if stamp >= *s => best = Some((stamp, path)),
            _ => {}
        }
    }
    Ok(best.map(|(_, p)| p))
}

/// Stable log id from file name (used in site_id).
pub fn fleet_log_id(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_fleet(dir: &Path, name: &str, listener: &str) {
        let body = format!(
            "---------------------------------------------------------------\n\
          Listener:        {listener}\n\
        ---------------------------------------------------------------\n\
\n\
[ 2026.08.01 12:00:00 ] Pilot > 1\n"
        );
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn resolves_listener_newest_among_many() {
        let dir = tempdir().unwrap();
        write_fleet(dir.path(), "Fleet_20260801_100000_1.txt", "Alpha Pilot");
        write_fleet(dir.path(), "Fleet_20260801_110000_1.txt", "Alpha Pilot");
        write_fleet(dir.path(), "Fleet_20260801_110000_2.txt", "Other Pilot");

        let path = resolve_active_fleet_log(dir.path(), "Alpha Pilot")
            .unwrap()
            .expect("should find");
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("110000"));
        assert_eq!(
            resolve_active_fleet_log(dir.path(), "Missing")
                .unwrap(),
            None
        );
    }

    #[test]
    fn list_characters_scans_headers() {
        let dir = tempdir().unwrap();
        write_fleet(dir.path(), "Fleet_a_1.txt", "Estemaire Saissore Orlenard");
        write_fleet(dir.path(), "Fleet_b_2.txt", "Other Pilot");
        write_fleet(dir.path(), "Fleet_c_1.txt", "Estemaire Saissore Orlenard");
        let names = list_characters(dir.path()).unwrap();
        assert_eq!(
            names,
            vec![
                "Estemaire Saissore Orlenard".to_string(),
                "Other Pilot".to_string()
            ]
        );
    }

    #[test]
    fn live_hamilton_log_parses_two_ones_when_present() {
        let p = std::path::Path::new(
            r"c:\Users\MrWildGear\Documents\EVE\logs\Chatlogs\Fleet_20260802_183116_90645543.txt",
        );
        if !p.exists() {
            return;
        }
        let text = match crate::encoding::read_chatlog(p) {
            Ok(t) => t,
            Err(_) => return, // skip if EVE holds an exclusive lock in this environment
        };
        let sites = crate::parse::parse_site_candidates(&text);
        assert!(
            sites.len() >= 2,
            "expected stacked tag 1s from live log, got {} (listener={:?})",
            sites.len(),
            crate::parse::parse_listener(&text)
        );
        assert!(sites.iter().all(|s| s.tag == "1"));
    }
}
