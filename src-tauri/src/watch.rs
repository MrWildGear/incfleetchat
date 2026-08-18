use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::state::{path_in_chatlogs, AppState};
use crate::types::Board;

const DEBOUNCE_MS: u64 = 150;

pub fn start_watcher(app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
    let dir = state.chatlogs_dir();
    std::fs::create_dir_all(&dir).ok();

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .map_err(|e| e.to_string())?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    // Keep watcher alive for process lifetime
    std::mem::forget(watcher);

    let watch_dir = dir.clone();
    std::thread::spawn(move || {
        let mut pending = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS)) {
                Ok(Ok(event)) => {
                    if event_touches_fleet(&watch_dir, &event) {
                        pending = true;
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if pending {
                        pending = false;
                        state.refresh_board();
                        let board: Board = state.board();
                        let _ = app.emit("board-updated", board);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // Drain burst
            while let Ok(Ok(event)) = rx.try_recv() {
                if event_touches_fleet(&watch_dir, &event) {
                    pending = true;
                }
            }
        }
    });

    Ok(())
}

pub fn start_gamelog_watcher(app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
    let dir = state.gamelogs_dir();
    std::fs::create_dir_all(&dir).ok();

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default()).map_err(|e| e.to_string())?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    std::mem::forget(watcher);

    let _ = state.poll_listener_following_warp();

    let watch_dir = dir.clone();
    std::thread::spawn(move || {
        let mut pending = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(Ok(event)) => {
                    if event_touches_gamelog(&watch_dir, &event) {
                        pending = true;
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    pending = true;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(Ok(event)) = rx.try_recv() {
                if event_touches_gamelog(&watch_dir, &event) {
                    pending = true;
                }
            }
            if pending {
                pending = false;
                if state.poll_listener_following_warp() && state.tracking_pip_should_open() {
                    let _ = app.emit("fleet-warp-detected", ());
                }
            }
        }
    });

    Ok(())
}

fn event_touches_gamelog(gamelogs: &PathBuf, event: &Event) -> bool {
    use crate::gamelog_watch::path_in_gamelogs;
    event.paths.iter().any(|p| path_in_gamelogs(gamelogs, p))
}

fn event_touches_fleet(chatlogs: &PathBuf, event: &Event) -> bool {
    event.paths.iter().any(|p| path_in_chatlogs(chatlogs, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::types::OverlaySettings;
    use std::fs;
    use std::thread;
    use tempfile::tempdir;

    fn write_fleet(dir: &std::path::Path, name: &str, listener: &str, body_extra: &str) {
        let body = format!(
            "---------------------------------------------------------------\n\
          Listener:        {listener}\n\
        ---------------------------------------------------------------\n\
\n\
[ 2026.08.01 12:00:00 ] Pilot > 1\n{body_extra}"
        );
        fs::write(dir.join(name), body).unwrap();
    }

    #[tokio::test]
    async fn refresh_picks_up_new_tag_after_rewrite() {
        let dir = tempdir().unwrap();
        write_fleet(dir.path(), "Fleet_test_1.txt", "Test Pilot", "");
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let state = AppState::new(db).await.unwrap();
        state
            .set_overlay_settings(OverlaySettings {
                character: Some("Test Pilot".into()),
                chatlogs_dir: Some(dir.path().to_string_lossy().to_string()),
                always_on_top: false,
                tracking_pip_enabled: true,
            })
            .await
            .unwrap();
        assert_eq!(state.board().sites.len(), 1);

        write_fleet(
            dir.path(),
            "Fleet_test_1.txt",
            "Test Pilot",
            "[ 2026.08.01 12:05:00 ] Pilot > 2\n",
        );
        // Simulate watch-triggered refresh
        state.refresh_board();
        let board = state.board();
        let tags: Vec<_> = board.sites.iter().map(|s| s.tag.as_str()).collect();
        assert!(tags.contains(&"1"));
        assert!(tags.contains(&"2"));
        thread::sleep(Duration::from_millis(10));
    }
}
