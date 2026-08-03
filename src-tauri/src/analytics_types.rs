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
    /// Recompute gamelog enrichment for a run (defaults to the last sealed
    /// run when `run_id` is omitted) and persist it.
    ReenrichRun { run_id: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Most recently sealed run in this session; enrichment can be recomputed
    /// for it even while the scope is a spawn aggregate.
    pub sealed_run_id: Option<String>,
    pub enrichment: Option<EnrichmentSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub spawns: Vec<SpawnSummary>,
    pub runs: Vec<RunSummary>,
}

/// Where a site row's warp/combat→payout split came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentSource {
    Fc,
    Borrowed,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichmentSite {
    pub occurred_at: DateTime<Utc>,
    pub warp_seconds: i64,
    pub combat_to_payout_seconds: Option<i64>,
    pub is_break: bool,
    pub source: EnrichmentSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissileStat {
    pub listener: String,
    pub reload_cycles: u32,
    pub hits: u32,
    pub missiles_per_cycle: u32,
    pub dead: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichmentTotals {
    pub warp_seconds: i64,
    pub combat_to_payout_seconds: Option<i64>,
    pub avg_combat_to_payout_seconds: Option<f64>,
    pub fleet_dead: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichmentSnapshot {
    pub resolved_fc: Option<String>,
    /// Listener headers used (all listeners with logs in range).
    pub listeners: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub sites: Vec<EnrichmentSite>,
    pub missiles: Vec<MissileStat>,
    pub totals: EnrichmentTotals,
}
