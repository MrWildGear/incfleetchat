use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;
use crate::state::{default_chatlogs_dir, AppState};
use crate::types::{AppSettings, Board};
use crate::watch;

#[tauri::command]
async fn get_board(state: State<'_, Arc<AppState>>) -> Result<Board, String> {
    Ok(state.board())
}

#[tauri::command]
async fn mark_ran(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    site_id: String,
) -> Result<Board, String> {
    let board = state.mark_ran_cmd(&site_id).await?;
    let _ = app.emit("board-updated", &board);
    Ok(board)
}

#[tauri::command]
async fn clear_site(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    site_id: String,
) -> Result<Board, String> {
    let board = state.clear_site_cmd(&site_id).await?;
    let _ = app.emit("board-updated", &board);
    Ok(board)
}

#[tauri::command]
async fn clear_ready(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<Board, String> {
    let board = state.clear_ready_cmd().await?;
    let _ = app.emit("board-updated", &board);
    Ok(board)
}

#[tauri::command]
async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    Ok(state.settings())
}

#[derive(Debug, serde::Deserialize)]
pub struct SettingsPatch {
    pub character: Option<Option<String>>,
    pub chatlogs_dir: Option<Option<String>>,
    pub always_on_top: Option<bool>,
}

#[tauri::command]
async fn set_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let mut next = state.settings();
    if let Some(c) = patch.character {
        next.character = c;
    }
    if let Some(d) = patch.chatlogs_dir {
        next.chatlogs_dir = d;
    }
    if let Some(a) = patch.always_on_top {
        next.always_on_top = a;
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_always_on_top(a);
        }
    }
    let settings = state.set_settings(next).await?;
    let _ = app.emit("board-updated", state.board());
    Ok(settings)
}

#[tauri::command]
async fn list_characters(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    state.list_characters()
}

#[tauri::command]
async fn set_always_on_top(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    on: bool,
) -> Result<(), String> {
    let mut s = state.settings();
    s.always_on_top = on;
    state.set_settings(s).await?;
    if let Some(win) = app.get_webview_window("main") {
        win.set_always_on_top(on).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn refresh_board(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<Board, String> {
    state.refresh_board();
    let board = state.board();
    let _ = app.emit("board-updated", &board);
    Ok(board)
}

fn data_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).ok();
    Ok(dir.join("incfleetchat.db"))
}

pub fn run_app() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let db_path = data_db_path(&handle)?;
            let state = tauri::async_runtime::block_on(async {
                let db = Db::open(&db_path).await.map_err(|e| e.to_string())?;
                AppState::new(db).await
            })?;

            let _ = default_chatlogs_dir();
            let settings = state.settings();
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_always_on_top(settings.always_on_top);
            }

            watch::start_watcher(handle, state.clone())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_board,
            mark_ran,
            clear_site,
            clear_ready,
            get_settings,
            set_settings,
            list_characters,
            set_always_on_top,
            refresh_board,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
