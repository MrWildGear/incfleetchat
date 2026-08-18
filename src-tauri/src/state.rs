use chrono::Utc;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::encoding::read_chatlog;
use crate::board::build_board;
use crate::db::Db;
use crate::gamelog_scan::default_gamelogs_dir;
use crate::gamelog_watch::{
    latest_following_warp_at, poll_following_warp, resolve_active_gamelog, WarpPoll, WarpWatchCursor,
};
use crate::parse::parse_site_candidates;
use crate::resolve::{fleet_log_id, list_characters, resolve_active_fleet_log};
use crate::types::{
    Board, BoardStatus, OverlaySettings, RecordSessionTrackingInput, SessionTrackingEvent,
    SessionTrackingEventKind,
};

pub struct AppState {
    pub db: Db,
    inner: Mutex<Inner>,
}

struct Inner {
    settings: OverlaySettings,
    gamelogs_dir: Option<String>,
    board: Board,
    ran_ids: HashSet<String>,
    cleared_ids: HashSet<String>,
    active_log: Option<PathBuf>,
    warp_cursor: WarpWatchCursor,
    tracking_pip_dismissed: bool,
}

impl AppState {
    pub async fn new(db: Db) -> Result<Arc<Self>, String> {
        let settings = db
            .get_overlay_settings()
            .await
            .map_err(|e| e.to_string())?;
        let ran_ids = db.load_ran_ids().await.map_err(|e| e.to_string())?;
        let cleared_ids = db.load_cleared_ids().await.map_err(|e| e.to_string())?;
        let tools = db.get_tools_settings().await.map_err(|e| e.to_string())?;
        let state = Arc::new(Self {
            db,
            inner: Mutex::new(Inner {
                settings,
                gamelogs_dir: tools.gamelogs_dir,
                board: Board {
                    status: BoardStatus::NoCharacter,
                    sites: vec![],
                    ready_count: 0,
                    updated_at: Utc::now(),
                },
                ran_ids,
                cleared_ids,
                active_log: None,
                warp_cursor: WarpWatchCursor::default(),
                tracking_pip_dismissed: false,
            }),
        });
        state.refresh_board();
        Ok(state)
    }

    pub fn board(&self) -> Board {
        self.inner.lock().board.clone()
    }

    pub fn overlay_settings(&self) -> OverlaySettings {
        self.inner.lock().settings.clone()
    }

    pub fn chatlogs_dir(&self) -> PathBuf {
        let s = self.inner.lock().settings.clone();
        if let Some(dir) = s.chatlogs_dir {
            return PathBuf::from(dir);
        }
        default_chatlogs_dir()
    }

    pub fn gamelogs_dir(&self) -> PathBuf {
        let dir = self.inner.lock().gamelogs_dir.clone();
        if let Some(dir) = dir {
            if !dir.trim().is_empty() {
                return PathBuf::from(dir);
            }
        }
        default_gamelogs_dir()
    }

    pub fn set_cached_gamelogs_dir(&self, dir: Option<String>) {
        let mut inner = self.inner.lock();
        inner.gamelogs_dir = dir;
        inner.warp_cursor = WarpWatchCursor::default();
    }

    pub fn poll_listener_following_warp(&self) -> bool {
        let character = {
            let inner = self.inner.lock();
            match &inner.settings.character {
                Some(c) if !c.is_empty() => c.clone(),
                _ => return false,
            }
        };
        let dir = self.gamelogs_dir();
        let path = resolve_active_gamelog(&dir, &character).ok().flatten();
        let latest = path.as_ref().and_then(|p| latest_following_warp_at(p));
        let mut inner = self.inner.lock();
        let poll = poll_following_warp(&mut inner.warp_cursor, path, latest);
        if matches!(poll, WarpPoll::NewWarp) {
            inner.tracking_pip_dismissed = false;
            return true;
        }
        false
    }

    pub fn tracking_pip_should_open(&self) -> bool {
        let inner = self.inner.lock();
        inner.settings.tracking_pip_enabled && !inner.tracking_pip_dismissed
    }

    pub fn dismiss_tracking_pip(&self) {
        self.inner.lock().tracking_pip_dismissed = true;
    }

    pub async fn set_overlay_settings(
        &self,
        patch: OverlaySettings,
    ) -> Result<OverlaySettings, String> {
        {
            let mut inner = self.inner.lock();
            let character_changed = inner.settings.character != patch.character;
            inner.settings = patch.clone();
            if character_changed {
                inner.warp_cursor = WarpWatchCursor::default();
            }
            if !inner.settings.tracking_pip_enabled {
                inner.tracking_pip_dismissed = true;
            }
        }
        self.db
            .set_overlay_settings(&patch)
            .await
            .map_err(|e| e.to_string())?;
        self.refresh_board();
        Ok(self.overlay_settings())
    }

    pub fn list_characters(&self) -> Result<Vec<String>, String> {
        list_characters(&self.chatlogs_dir()).map_err(|e| e.to_string())
    }

    pub fn refresh_board(&self) {
        let mut inner = self.inner.lock();
        let now = Utc::now();
        let character = match &inner.settings.character {
            Some(c) if !c.is_empty() => c.clone(),
            _ => {
                inner.board = Board {
                    status: BoardStatus::NoCharacter,
                    sites: vec![],
                    ready_count: 0,
                    updated_at: now,
                };
                inner.active_log = None;
                return;
            }
        };

        let dir = if let Some(ref d) = inner.settings.chatlogs_dir {
            PathBuf::from(d)
        } else {
            default_chatlogs_dir()
        };

        let resolved = match resolve_active_fleet_log(&dir, &character) {
            Ok(p) => p,
            Err(e) => {
                inner.board = Board {
                    status: BoardStatus::Error {
                        message: e.to_string(),
                    },
                    sites: vec![],
                    ready_count: 0,
                    updated_at: now,
                };
                return;
            }
        };

        let Some(path) = resolved else {
            inner.active_log = None;
            inner.board = Board {
                status: BoardStatus::WaitingForLog {
                    character: character.clone(),
                },
                sites: vec![],
                ready_count: 0,
                updated_at: now,
            };
            return;
        };

        // New fleet file → fresh board visuals; cleared set is per-id so old clears don't matter
        let log_changed = inner
            .active_log
            .as_ref()
            .map(|p| p != &path)
            .unwrap_or(true);
        if log_changed {
            // Keep ran/cleared in DB; live board only shows current file's candidates
            inner.active_log = Some(path.clone());
        }

        let text = match read_chatlog(&path) {
            Ok(t) => t,
            Err(e) => {
                inner.board = Board {
                    status: BoardStatus::Error {
                        message: e.to_string(),
                    },
                    sites: vec![],
                    ready_count: 0,
                    updated_at: now,
                };
                return;
            }
        };

        let log_id = fleet_log_id(&path);
        let candidates = parse_site_candidates(&text);
        let status = BoardStatus::Watching {
            character,
            log_name: log_id.clone(),
        };
        inner.board = build_board(
            status,
            &log_id,
            &candidates,
            &inner.ran_ids,
            &inner.cleared_ids,
            now,
        );
    }

    pub async fn mark_ran_cmd(&self, site_id: &str) -> Result<Board, String> {
        let now = Utc::now();
        self.db
            .mark_ran(site_id, now)
            .await
            .map_err(|e| e.to_string())?;
        {
            let mut inner = self.inner.lock();
            inner.ran_ids.insert(site_id.to_string());
            if let Some(row) = inner.board.sites.iter_mut().find(|s| s.id == site_id) {
                row.ran = true;
                let expired = now >= row.expires_at;
                if expired {
                    row.phase = crate::types::SitePhase::Ready;
                    row.clearable = true;
                }
            }
            inner.board.ready_count =
                inner.board.sites.iter().filter(|s| s.clearable).count() as u32;
            inner.board.updated_at = now;
        }
        Ok(self.board())
    }

    pub async fn clear_site_cmd(&self, site_id: &str) -> Result<Board, String> {
        let now = Utc::now();
        let should_persist = {
            let inner = self.inner.lock();
            inner
                .board
                .sites
                .iter()
                .find(|s| s.id == site_id)
                .map(|s| s.ran && now >= s.expires_at)
                .unwrap_or(false)
        };
        if should_persist {
            self.db
                .clear_site(site_id, now)
                .await
                .map_err(|e| e.to_string())?;
        }
        {
            let mut inner = self.inner.lock();
            let clearable = inner
                .board
                .sites
                .iter()
                .find(|s| s.id == site_id)
                .map(|s| s.ran && now >= s.expires_at)
                .unwrap_or(false);
            if clearable {
                inner.cleared_ids.insert(site_id.to_string());
                inner.board.sites.retain(|s| s.id != site_id);
                inner.board.ready_count = inner
                    .board
                    .sites
                    .iter()
                    .filter(|s| s.ran && now >= s.expires_at)
                    .count() as u32;
            }
            inner.board.updated_at = now;
        }
        Ok(self.board())
    }

    pub async fn clear_ready_cmd(&self) -> Result<Board, String> {
        let now = Utc::now();
        let ids: Vec<String> = {
            let inner = self.inner.lock();
            inner
                .board
                .sites
                .iter()
                .filter(|s| s.ran && now >= s.expires_at)
                .map(|s| s.id.clone())
                .collect()
        };
        if !ids.is_empty() {
            self.db
                .clear_sites(&ids, now)
                .await
                .map_err(|e| e.to_string())?;
        }
        {
            let mut inner = self.inner.lock();
            for id in &ids {
                inner.cleared_ids.insert(id.clone());
            }
            let id_set: HashSet<_> = ids.iter().cloned().collect();
            inner.board.sites.retain(|s| !id_set.contains(&s.id));
            inner.board.ready_count = inner
                .board
                .sites
                .iter()
                .filter(|s| s.ran && now >= s.expires_at)
                .count() as u32;
            inner.board.updated_at = now;
        }
        Ok(self.board())
    }

    fn active_fleet_log_id(&self) -> Result<String, String> {
        match &self.inner.lock().board.status {
            BoardStatus::Watching { log_name, .. } => Ok(log_name.clone()),
            BoardStatus::WaitingForLog { character } => Err(format!(
                "Session tracking requires an active fleet log (waiting for log for {character})"
            )),
            BoardStatus::NoCharacter => {
                Err("Session tracking requires a configured Listener".into())
            }
            BoardStatus::Error { message } => Err(message.clone()),
        }
    }

    pub async fn record_session_tracking_event_cmd(
        &self,
        input: RecordSessionTrackingInput,
    ) -> Result<SessionTrackingEvent, String> {
        match input.event_kind {
            SessionTrackingEventKind::FleetWarp => {
                if input.site_kind.is_none() {
                    return Err("Fleet warp tracking requires a site kind".into());
                }
            }
            SessionTrackingEventKind::BreakStart => {
                if input.site_kind.is_some() {
                    return Err("Break tracking must not include a site kind".into());
                }
            }
        }
        let fleet_log_id = match self.active_fleet_log_id() {
            Ok(id) => id,
            Err(_) => self
                .inner
                .lock()
                .warp_cursor
                .path
                .as_ref()
                .map(|p| fleet_log_id(p))
                .ok_or_else(|| "Session tracking requires a Listener gamelog".to_string())?,
        };
        let now = Utc::now();
        self.db
            .record_session_tracking_event(&fleet_log_id, &input, now)
            .await
            .map_err(|e| e.to_string())
    }
}

pub fn default_chatlogs_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("EVE")
        .join("logs")
        .join("Chatlogs")
}

pub fn path_in_chatlogs(chatlogs: &Path, changed: &Path) -> bool {
    changed.starts_with(chatlogs)
        || changed
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("Fleet_") && n.ends_with(".txt"))
            .unwrap_or(false)
}
