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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AppSettings {
    pub character: Option<String>,
    pub chatlogs_dir: Option<String>,
    pub always_on_top: bool,
}

/// Parsed tag candidate before Ran merge / phase computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteCandidate {
    pub speaker: String,
    pub posted_at: DateTime<Utc>,
    pub tag: String,
    pub line_ordinal: u32,
}
