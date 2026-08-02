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

/// Pick the newest Fleet_*.txt whose Listener matches `character`.
pub fn resolve_active_fleet_log(dir: &Path, character: &str) -> std::io::Result<Option<PathBuf>> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
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
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &best {
            None => best = Some((modified, path)),
            Some((t, _)) if modified >= *t => best = Some((modified, path)),
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
    use std::thread;
    use std::time::Duration;
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
        thread::sleep(Duration::from_millis(20));
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
}
