use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::analytics_types::{AmendOp, EditionFocus, ReportScope, Tray};
use crate::db::Db;
use crate::enrichment_pipeline::EnrichmentInputs;
use crate::run_desk::RunDesk;
use crate::state::{default_chatlogs_dir, AppState};
use crate::types::{Board, OverlaySettings, RecordSessionTrackingInput, SessionTrackingEvent, ToolsSettings};
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
async fn get_overlay_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<OverlaySettings, String> {
    Ok(state.overlay_settings())
}

#[derive(Debug, serde::Deserialize)]
pub struct OverlaySettingsPatch {
    pub character: Option<Option<String>>,
    pub chatlogs_dir: Option<Option<String>>,
    pub always_on_top: Option<bool>,
    pub tracking_pip_enabled: Option<bool>,
}

#[tauri::command]
async fn set_overlay_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    patch: OverlaySettingsPatch,
) -> Result<OverlaySettings, String> {
    let mut next = state.overlay_settings();
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
    if let Some(enabled) = patch.tracking_pip_enabled {
        next.tracking_pip_enabled = enabled;
        if !enabled {
            if let Some(win) = app.get_webview_window("tracking-pip") {
                let _ = win.hide();
            }
            state.dismiss_tracking_pip();
        }
    }
    let settings = state.set_overlay_settings(next).await?;
    let _ = app.emit("board-updated", state.board());
    Ok(settings)
}

#[tauri::command]
async fn get_tools_settings(state: State<'_, Arc<AppState>>) -> Result<ToolsSettings, String> {
    state
        .db
        .get_tools_settings()
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
pub struct ToolsSettingsPatch {
    pub gamelogs_dir: Option<Option<String>>,
    pub fc_character: Option<Option<String>>,
    pub ammo_launchers: Option<i64>,
    pub ammo_per_launcher: Option<i64>,
}

#[tauri::command]
async fn set_tools_settings(
    state: State<'_, Arc<AppState>>,
    patch: ToolsSettingsPatch,
) -> Result<ToolsSettings, String> {
    let mut next = state
        .db
        .get_tools_settings()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(d) = patch.gamelogs_dir {
        next.gamelogs_dir = d;
    }
    if let Some(f) = patch.fc_character {
        next.fc_character = f;
    }
    if let Some(a) = patch.ammo_launchers {
        next.ammo_launchers = a;
    }
    if let Some(a) = patch.ammo_per_launcher {
        next.ammo_per_launcher = a;
    }
    state
        .db
        .set_tools_settings(&next)
        .await
        .map_err(|e| e.to_string())?;
    state.set_cached_gamelogs_dir(next.gamelogs_dir.clone());
    Ok(next)
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
    let mut s = state.overlay_settings();
    s.always_on_top = on;
    state.set_overlay_settings(s).await?;
    if let Some(win) = app.get_webview_window("main") {
        win.set_always_on_top(on).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn record_session_tracking_event(
    state: State<'_, Arc<AppState>>,
    input: RecordSessionTrackingInput,
) -> Result<SessionTrackingEvent, String> {
    state.record_session_tracking_event_cmd(input).await
}

#[tauri::command]
async fn refresh_board(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<Board, String> {
    state.refresh_board();
    let board = state.board();
    let _ = app.emit("board-updated", &board);
    Ok(board)
}

fn show_tracking_pip(app: &AppHandle, state: Arc<AppState>) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("tracking-pip") {
        let _ = win.unminimize();
        if !win.is_visible().map_err(|e| e.to_string())? {
            win.show().map_err(|e| e.to_string())?;
        }
        let _ = app.emit_to("tracking-pip", "fleet-warp-detected", ());
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, "tracking-pip", WebviewUrl::App("index.html".into()))
        .title("Fleet warp")
        .inner_size(240.0, 220.0)
        .min_inner_size(200.0, 180.0)
        .resizable(true)
        .always_on_top(true)
        .focused(false)
        .decorations(true)
        .skip_taskbar(false)
        .build()
        .map_err(|e| e.to_string())?;
    let win_for_handler = win.clone();
    win.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let _ = win_for_handler.hide();
            state.dismiss_tracking_pip();
            api.prevent_close();
        }
    });
    Ok(())
}

#[tauri::command]
async fn open_tracking_pip(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if !state.tracking_pip_should_open() {
        return Ok(());
    }
    show_tracking_pip(&app, state.inner().clone())
}

#[tauri::command]
async fn hide_tracking_pip(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.dismiss_tracking_pip();
    if let Some(win) = app.get_webview_window("tracking-pip") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn open_tools_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("tools") {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "tools", WebviewUrl::App("index.html".into()))
        .title("IncFleetChat Tools")
        .inner_size(960.0, 680.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn run_desk_open(desk: State<'_, Arc<RunDesk>>) -> Result<EditionFocus, String> {
    desk.open().await
}

#[tauri::command]
async fn run_desk_paste(
    desk: State<'_, Arc<RunDesk>>,
    tray: Tray,
    text: String,
) -> Result<EditionFocus, String> {
    desk.paste(tray, &text).await
}

#[tauri::command]
async fn run_desk_analyze(
    desk: State<'_, Arc<RunDesk>>,
    inputs: EnrichmentInputs,
) -> Result<EditionFocus, String> {
    desk.analyze(&inputs).await
}

#[tauri::command]
async fn run_desk_focus(
    desk: State<'_, Arc<RunDesk>>,
    scope: ReportScope,
) -> Result<EditionFocus, String> {
    desk.focus(scope).await
}

#[tauri::command]
async fn run_desk_amend(
    desk: State<'_, Arc<RunDesk>>,
    op: AmendOp,
) -> Result<EditionFocus, String> {
    desk.amend(op).await
}

#[tauri::command]
async fn run_desk_reenrich(
    desk: State<'_, Arc<RunDesk>>,
    run_id: Option<String>,
    inputs: EnrichmentInputs,
) -> Result<EditionFocus, String> {
    desk.reenrich(run_id, &inputs).await
}

#[tauri::command]
async fn run_desk_delete_run(
    desk: State<'_, Arc<RunDesk>>,
    run_id: String,
) -> Result<EditionFocus, String> {
    desk.delete_run(&run_id).await
}

#[tauri::command]
async fn run_desk_delete_spawn(
    desk: State<'_, Arc<RunDesk>>,
    constellation: String,
) -> Result<EditionFocus, String> {
    desk.delete_spawn(&constellation).await
}

#[tauri::command]
async fn run_desk_clear_all(desk: State<'_, Arc<RunDesk>>) -> Result<EditionFocus, String> {
    desk.clear_all().await
}

#[tauri::command]
async fn lookup_vanguard_payout(
    space: crate::vanguard_payouts::SpaceBand,
    fleet_size: u32,
) -> Result<crate::vanguard_payouts::PayoutTicket, String> {
    Ok(crate::vanguard_payouts::lookup_payout(space, fleet_size))
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let db_path = data_db_path(&handle)?;
            let (state, desk) = tauri::async_runtime::block_on(async {
                let db = Db::open(&db_path).await.map_err(|e| e.to_string())?;
                let desk = Arc::new(RunDesk::new(db.clone()));
                let state = AppState::new(db).await?;
                Ok::<_, String>((state, desk))
            })?;

            let _ = default_chatlogs_dir();
            let settings = state.overlay_settings();
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_always_on_top(settings.always_on_top);
            }

            watch::start_watcher(handle.clone(), state.clone())?;
            watch::start_gamelog_watcher(handle, state.clone())?;
            app.manage(state);
            app.manage(desk);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_board,
            mark_ran,
            clear_site,
            clear_ready,
            get_overlay_settings,
            set_overlay_settings,
            get_tools_settings,
            set_tools_settings,
            list_characters,
            set_always_on_top,
            refresh_board,
            record_session_tracking_event,
            open_tracking_pip,
            hide_tracking_pip,
            open_tools_window,
            run_desk_open,
            run_desk_paste,
            run_desk_analyze,
            run_desk_focus,
            run_desk_amend,
            run_desk_reenrich,
            run_desk_delete_run,
            run_desk_delete_spawn,
            run_desk_clear_all,
            lookup_vanguard_payout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
