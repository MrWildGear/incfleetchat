use chrono::{NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::OnceLock;

use crate::types::SiteCandidate;

fn message_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^\[ (\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}) \] (.+?) > (.*)$",
        )
        .expect("message regex")
    })
}

fn parse_eve_timestamp(s: &str) -> Option<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y.%m.%d %H:%M:%S").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

fn is_site_tag(body: &str) -> Option<String> {
    let t = body.trim();
    if t.len() != 1 {
        return None;
    }
    let c = t.chars().next()?;
    if c.is_ascii_alphanumeric() {
        Some(c.to_ascii_lowercase().to_string())
    } else {
        None
    }
}

/// Extract Listener name from EVE chatlog header, if present.
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

/// Parse fleet chatlog text into stacked site tag candidates.
pub fn parse_site_candidates(text: &str) -> Vec<SiteCandidate> {
    let cleaned = text.trim_start_matches('\u{feff}');
    let mut out = Vec::new();
    let mut ordinal: u32 = 0;
    for cap in message_re().captures_iter(cleaned) {
        let ts = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let speaker = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        let body = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let Some(tag) = is_site_tag(body) else {
            continue;
        };
        let Some(posted_at) = parse_eve_timestamp(ts) else {
            continue;
        };
        out.push(SiteCandidate {
            speaker: speaker.to_string(),
            posted_at,
            tag,
            line_ordinal: ordinal,
        });
        ordinal += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const SAMPLE: &str = include_str!("../tests/fixtures/fleet_sample.txt");

    #[test]
    fn parses_listener_from_header() {
        assert_eq!(
            parse_listener(SAMPLE).as_deref(),
            Some("Estemaire Saissore Orlenard")
        );
    }

    #[test]
    fn extracts_only_exact_single_char_tags() {
        let sites = parse_site_candidates(SAMPLE);
        let tags: Vec<&str> = sites.iter().map(|s| s.tag.as_str()).collect();
        assert_eq!(tags, vec!["1", "1", "2", "3", "a", "b", "c", "1"]);
    }

    #[test]
    fn ignores_non_tag_messages_including_cleared() {
        let sites = parse_site_candidates(SAMPLE);
        assert!(sites.iter().all(|s| s.tag.len() == 1));
        assert!(!sites.iter().any(|s| s.speaker == "tomar norris"));
    }

    #[test]
    fn stacks_duplicate_tags_with_distinct_ordinals() {
        let sites = parse_site_candidates(SAMPLE);
        let ones: Vec<_> = sites.iter().filter(|s| s.tag == "1").collect();
        assert_eq!(ones.len(), 3);
        assert_eq!(ones[0].line_ordinal, 0);
        assert_eq!(ones[1].line_ordinal, 1);
        assert_eq!(ones[2].line_ordinal, 7);
        assert_eq!(
            ones[0].posted_at,
            Utc.with_ymd_and_hms(2026, 8, 1, 19, 42, 9).unwrap()
        );
    }

    #[test]
    fn normalizes_tag_case() {
        let text = "[ 2026.08.01 12:00:00 ] Pilot > A\n";
        let sites = parse_site_candidates(text);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].tag, "a");
    }
}
