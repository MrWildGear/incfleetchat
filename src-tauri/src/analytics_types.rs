//! Shared analytics DTOs returned to the Tools UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::spawn_parse::SpawnDraft;
use crate::timing::{AnalyticsReport, RunSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tray {
    Manifest,
    Wallet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportScope {
    Overall,
    Spawn { constellation: String },
    Run { run_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AmendOp {
    ClearWalletTray,
    SetSessionSettings { settings: RunSettings },
    ReopenTrays,
    OpenRun { run_id: String },
    SetConstellation { constellation: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSummary {
    pub constellation: String,
    pub region: Option<String>,
    pub staging_system: Option<String>,
    pub hq_system: Option<String>,
    pub run_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub constellation: String,
    pub saved_at: DateTime<Utc>,
    pub site_count: u32,
    pub liquid_isk: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayState {
    pub manifest: String,
    pub wallet_batches: u32,
    pub pending_sites: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditionFocus {
    pub trays: TrayState,
    pub spawn: Option<SpawnSummary>,
    pub catalog: Catalog,
    pub scope: ReportScope,
    pub report: Option<AnalyticsReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub session_settings: RunSettings,
    pub staging_spawn: Option<SpawnDraft>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub spawns: Vec<SpawnSummary>,
    pub runs: Vec<RunSummary>,
}
