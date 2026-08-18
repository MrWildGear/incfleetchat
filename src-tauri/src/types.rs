use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const TIMER_MINUTES: i64 = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SitePhase {
    Active,
    Overdue,
    Ready,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SiteRow {
    pub id: String,
    pub tag: String,
    pub speaker: String,
    pub posted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ran: bool,
    pub phase: SitePhase,
    pub clearable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BoardStatus {
    Watching { character: String, log_name: String },
    NoCharacter,
    WaitingForLog { character: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Board {
    pub status: BoardStatus,
    pub sites: Vec<SiteRow>,
    pub ready_count: u32,
    pub updated_at: DateTime<Utc>,
}

/// Durable overlay prefs — Listener, chatlogs, always-on-top.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlaySettings {
    pub character: Option<String>,
    pub chatlogs_dir: Option<String>,
    pub always_on_top: bool,
    pub tracking_pip_enabled: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            character: None,
            chatlogs_dir: None,
            always_on_top: false,
            tracking_pip_enabled: true,
        }
    }
}

/// Durable Tools prefs — gamelogs, FC, ammo fit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolsSettings {
    pub gamelogs_dir: Option<String>,
    pub fc_character: Option<String>,
    pub ammo_launchers: i64,
    pub ammo_per_launcher: i64,
}

impl Default for ToolsSettings {
    fn default() -> Self {
        Self {
            gamelogs_dir: None,
            fc_character: None,
            ammo_launchers: 6,
            ammo_per_launcher: 26,
        }
    }
}

/// Parsed tag candidate before Ran merge / phase computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteCandidate {
    pub speaker: String,
    pub posted_at: DateTime<Utc>,
    pub tag: String,
    pub line_ordinal: u32,
}

/// Incursion site chosen from the overlay PiP fleet-warp prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTrackingSiteKind {
    OtaHacking,
    Nco,
    NmcMining,
}

/// Operator action recorded from the overlay session-tracking popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTrackingEventKind {
    FleetWarp,
    BreakStart,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTrackingEvent {
    pub id: i64,
    pub fleet_log_id: String,
    pub event_kind: SessionTrackingEventKind,
    pub site_kind: Option<SessionTrackingSiteKind>,
    pub overlay_site_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSessionTrackingInput {
    pub event_kind: SessionTrackingEventKind,
    pub site_kind: Option<SessionTrackingSiteKind>,
    pub overlay_site_id: Option<String>,
}
