//! Parse Kundalini Manifest–style INC spawn notices.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpawnDraft {
    pub constellation: Option<String>,
    pub region: Option<String>,
    pub security_status: Option<String>,
    pub sov_holder: Option<String>,
    pub staging_system: Option<String>,
    pub hq_system: Option<String>,
    pub assault_systems: Vec<String>,
    pub vanguard_systems: Vec<String>,
    pub announced_at: Option<chrono::DateTime<Utc>>,
    pub title: Option<String>,
}

fn parse_announced(s: &str) -> Option<chrono::DateTime<Utc>> {
    // e.g. 7/28/2026 5:41 AM — chrono on Windows lacks %-m; normalize manually.
    let s = s.trim().trim_start_matches('—').trim().trim_start_matches('-').trim();
    let formats = [
        "%m/%d/%Y %I:%M %p",
        "%m/%d/%Y %H:%M",
        "%Y.%m.%d %H:%M",
    ];
    for fmt in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    // Pad single-digit month/day: 7/28/2026 5:41 AM → 07/28/2026 05:41 AM
    let padded = pad_us_datetime(s)?;
    for fmt in ["%m/%d/%Y %I:%M %p", "%m/%d/%Y %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&padded, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%m/%d/%Y") {
        let t = NaiveTime::from_hms_opt(0, 0, 0)?;
        return Some(Utc.from_utc_datetime(&NaiveDateTime::new(d, t)));
    }
    if let Some(padded) = pad_us_date(s) {
        if let Ok(d) = NaiveDate::parse_from_str(&padded, "%m/%d/%Y") {
            let t = NaiveTime::from_hms_opt(0, 0, 0)?;
            return Some(Utc.from_utc_datetime(&NaiveDateTime::new(d, t)));
        }
    }
    None
}

fn pad_us_date(s: &str) -> Option<String> {
    let mut parts = s.split('/');
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    let y = parts.next()?;
    Some(format!("{m:02}/{d:02}/{y}"))
}

fn pad_us_datetime(s: &str) -> Option<String> {
    let (date, rest) = s.split_once(' ')?;
    let date = pad_us_date(date)?;
    let rest = rest.trim();
    // 5:41 AM → 05:41 AM
    let mut bits = rest.split_whitespace();
    let hm = bits.next()?;
    let ampm = bits.next().unwrap_or("");
    let (h, m) = hm.split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    if ampm.is_empty() {
        Some(format!("{date} {h:02}:{m:02}"))
    } else {
        Some(format!("{date} {h:02}:{m:02} {ampm}"))
    }
}

/// Extract labeled fields from a Discord/bot Manifest paste.
pub fn parse_manifest(text: &str) -> SpawnDraft {
    let mut draft = SpawnDraft::default();
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty())
        .collect();

    // Title: first non-empty line that isn't a label
    if let Some(first) = lines.first() {
        if !first.contains(':') && !matches!(*first, "Region" | "Constellation" | "APP") {
            draft.title = Some((*first).to_string());
        }
    }

    // "New Null-Sec Incursion: 4MY-AB - Immensea"
    for line in &lines {
        if let Some(rest) = line
            .strip_prefix("New Null-Sec Incursion:")
            .or_else(|| line.strip_prefix("New Low-Sec Incursion:"))
            .or_else(|| line.strip_prefix("New High-Sec Incursion:"))
        {
            let rest = rest.trim();
            if let Some((constel, region)) = rest.split_once(" - ") {
                draft.constellation = Some(constel.trim().to_string());
                draft.region = Some(region.trim().to_string());
            } else if !rest.is_empty() {
                draft.constellation = Some(rest.to_string());
            }
        }
        if let Some(ts) = parse_announced(line) {
            draft.announced_at = Some(ts);
        }
    }

    let mut i = 0;
    while i < lines.len() {
        let label = lines[i];
        let next = lines.get(i + 1).copied();
        match label {
            "Region" => {
                if let Some(v) = next {
                    if !v.contains(':') {
                        draft.region = Some(v.to_string());
                        i += 1;
                    }
                }
            }
            "Constellation" => {
                if let Some(v) = next {
                    draft.constellation = Some(v.to_string());
                    i += 1;
                }
            }
            "Security Status" => {
                if let Some(v) = next {
                    draft.security_status = Some(v.to_string());
                    i += 1;
                }
            }
            "Sov Holder" => {
                if let Some(v) = next {
                    draft.sov_holder = Some(v.to_string());
                    i += 1;
                }
            }
            "Staging System" => {
                if let Some(v) = next {
                    draft.staging_system = Some(v.to_string());
                    i += 1;
                }
            }
            "HQ System" => {
                if let Some(v) = next {
                    draft.hq_system = Some(v.to_string());
                    i += 1;
                }
            }
            "Assault System(s)" => {
                i += 1;
                while i < lines.len() {
                    let sys = lines[i];
                    if matches!(
                        sys,
                        "Vanguard Systems"
                            | "Region"
                            | "Constellation"
                            | "HQ System"
                            | "Staging System"
                    ) || sys.contains("Incursion:")
                    {
                        i -= 1;
                        break;
                    }
                    if parse_announced(sys).is_some() && !sys.chars().any(|c| c.is_ascii_alphabetic() && c != 'A' && c != 'M' && c != 'P') {
                        // likely a date line
                        break;
                    }
                    // System names look like F76-8Q / X-6WC7
                    if sys.contains('-') || sys.chars().any(|c| c.is_ascii_digit()) {
                        if parse_announced(sys).is_none() {
                            draft.assault_systems.push(sys.to_string());
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                    i += 1;
                }
            }
            "Vanguard Systems" => {
                i += 1;
                while i < lines.len() {
                    let sys = lines[i];
                    if parse_announced(sys).is_some() {
                        break;
                    }
                    if matches!(sys, "Region" | "Constellation" | "Assault System(s)") {
                        i -= 1;
                        break;
                    }
                    draft.vanguard_systems.push(sys.to_string());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let _ = lines;
    draft
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Kundalini Manifest
APP
 — 7/28/2026 5:41 AM
New Null-Sec Incursion: 4MY-AB - Immensea
Region
Immensea
Constellation
4MY-AB
Security Status
-0.1
Sov Holder
Goonswarm Federation [CONDI]
Staging System
F76-8Q
HQ System
PH-NFR
Assault System(s)
X-6WC7
W-FHWJ
Vanguard Systems
RHE7-W
D-BAMJ
JKWP-U
DW-N2S
7/28/2026 5:41 AM
";

    #[test]
    fn parses_constellation_and_region() {
        let d = parse_manifest(SAMPLE);
        assert_eq!(d.constellation.as_deref(), Some("4MY-AB"));
        assert_eq!(d.region.as_deref(), Some("Immensea"));
        assert_eq!(d.staging_system.as_deref(), Some("F76-8Q"));
        assert_eq!(d.hq_system.as_deref(), Some("PH-NFR"));
        assert_eq!(d.assault_systems, vec!["X-6WC7", "W-FHWJ"]);
        assert_eq!(
            d.vanguard_systems,
            vec!["RHE7-W", "D-BAMJ", "JKWP-U", "DW-N2S"]
        );
        assert!(d.announced_at.is_some());
    }
}
