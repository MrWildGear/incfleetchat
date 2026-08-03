//! Parse EVE gamelog headers and typed combat/notify events.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamelogEventKind {
    FollowingWarp,
    Regrouping,
    CombatHit,
    Reload,
    CombatAny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamelogEvent {
    pub occurred_at: DateTime<Utc>,
    pub kind: GamelogEventKind,
    pub raw_hint: String,
}

fn message_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Optional U+FEFF: live EVE logs often have a BOM on every message line.
        Regex::new(
            r"(?m)^\u{feff}?\[ (\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}) \] \((\w+)\) (.*)$",
        )
        .expect("gamelog message regex")
    })
}

fn parse_eve_timestamp(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y.%m.%d %H:%M:%S").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

/// Extract Listener name from EVE gamelog header, if present.
pub fn parse_listener(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        if let Some(rest) = trimmed.strip_prefix("Listener:") {
            let name = rest.trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract Session Started timestamp from EVE gamelog header, if present.
pub fn parse_session_started(text: &str) -> Option<DateTime<Utc>> {
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        let lower = trimmed.to_ascii_lowercase();
        if let Some(idx) = lower.find("session started:") {
            let rest = trimmed[idx + "session started:".len()..].trim();
            if let Some(ts) = parse_eve_timestamp(rest) {
                return Some(ts);
            }
        }
    }
    None
}

fn classify_event(channel: &str, body: &str) -> Option<GamelogEventKind> {
    match channel {
        "notify" => {
            if body.contains("Following") && body.contains("in warp") {
                Some(GamelogEventKind::FollowingWarp)
            } else if body.contains("Regrouping") {
                Some(GamelogEventKind::Regrouping)
            } else if body.contains("Loading the") && body.contains("Missile Launcher") {
                Some(GamelogEventKind::Reload)
            } else {
                None
            }
        }
        "combat" => {
            if body.contains("Hits") {
                Some(GamelogEventKind::CombatHit)
            } else {
                Some(GamelogEventKind::CombatAny)
            }
        }
        _ => None,
    }
}

/// Parse gamelog message lines into typed events.
pub fn parse_gamelog_events(text: &str) -> Vec<GamelogEvent> {
    let cleaned = text.trim_start_matches('\u{feff}');
    let mut out = Vec::new();
    for cap in message_re().captures_iter(cleaned) {
        let ts = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let channel = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let body = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let Some(kind) = classify_event(channel, body) else {
            continue;
        };
        let Some(occurred_at) = parse_eve_timestamp(ts) else {
            continue;
        };
        out.push(GamelogEvent {
            occurred_at,
            kind,
            raw_hint: body.to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const SAMPLE: &str = include_str!("../tests/fixtures/gamelog_sample.txt");

    #[test]
    fn parses_listener_and_session_started() {
        assert_eq!(
            parse_listener(SAMPLE).as_deref(),
            Some("Chelien Alabel Maricadie")
        );
        assert_eq!(
            parse_session_started(SAMPLE),
            Some(Utc.with_ymd_and_hms(2026, 8, 2, 18, 20, 40).unwrap())
        );
    }

    #[test]
    fn classifies_warp_regroup_combat_reload_events() {
        let events = parse_gamelog_events(SAMPLE);
        let kinds: Vec<GamelogEventKind> = events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                GamelogEventKind::FollowingWarp,
                GamelogEventKind::Regrouping,
                GamelogEventKind::CombatHit,
                GamelogEventKind::Reload,
                GamelogEventKind::CombatAny,
            ]
        );
        assert_eq!(
            events[0].occurred_at,
            Utc.with_ymd_and_hms(2026, 8, 2, 18, 21, 5).unwrap()
        );
        assert_eq!(
            events[3].occurred_at,
            Utc.with_ymd_and_hms(2026, 8, 2, 18, 24, 20).unwrap()
        );
        // Exactly one Reload — "run out of charges" must not double-count.
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::Reload)
                .count(),
            1
        );
    }

    #[test]
    fn ignores_question_lines() {
        let events = parse_gamelog_events(SAMPLE);
        assert!(!events.iter().any(|e| e.raw_hint.contains("question")));
        assert!(!events.iter().any(|e| e.raw_hint.contains("Is the site clear")));
        assert_eq!(events.len(), 5);
    }
}
