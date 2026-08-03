//! Parse EVE wallet journal paste (tab-separated client export).

use chrono::{NaiveDateTime, TimeZone, Utc};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletPayout {
    pub occurred_at: chrono::DateTime<Utc>,
    pub amount_isk: i64,
    pub ref_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct WalletParseResult {
    pub events: Vec<WalletPayout>,
    pub ignored_lines: u32,
    pub duplicates_dropped: u32,
}

fn parse_eve_ts(s: &str) -> Option<chrono::DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y.%m.%d %H:%M").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

fn parse_isk_amount(s: &str) -> Option<i64> {
    let cleaned = s
        .trim()
        .trim_end_matches(" ISK")
        .trim_end_matches(" isk")
        .replace(',', "");
    cleaned.parse::<i64>().ok()
}

/// Parse wallet journal text. Keep rows where ref type is Corporate Reward Payout
/// and amount equals `expected_isk`. Dedupes by (timestamp, amount).
pub fn parse_wallet_journal(text: &str, expected_isk: i64) -> WalletParseResult {
    let mut out = WalletParseResult::default();
    let mut seen: HashSet<(i64, i64)> = HashSet::new();

    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').map(|p| p.trim()).collect();
        if parts.len() < 3 {
            out.ignored_lines += 1;
            continue;
        }
        let Some(ts) = parse_eve_ts(parts[0]) else {
            out.ignored_lines += 1;
            continue;
        };
        let ref_type = parts[1];
        let Some(amount) = parse_isk_amount(parts[2]) else {
            out.ignored_lines += 1;
            continue;
        };
        if ref_type != "Corporate Reward Payout" || amount != expected_isk {
            out.ignored_lines += 1;
            continue;
        }
        let key = (ts.timestamp(), amount);
        if !seen.insert(key) {
            out.duplicates_dropped += 1;
            continue;
        }
        let description = parts.get(4).unwrap_or(&"").to_string();
        out.events.push(WalletPayout {
            occurred_at: ts,
            amount_isk: amount,
            ref_type: ref_type.to_string(),
            description,
        });
    }

    out.events.sort_by_key(|e| e.occurred_at);
    out
}

/// Best-effort FC-name hint from a Corporate Reward Payout description, e.g.
/// `"CONCORD rewarded tomar norris for services performed."` → `Some("tomar norris")`.
pub fn extract_wallet_fc_hint(description: &str) -> Option<String> {
    let prefix = "CONCORD rewarded ";
    let suffix = " for services performed";
    let start = description.find(prefix)? + prefix.len();
    let rest = &description[start..];
    let end = rest.find(suffix)?;
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
2026.07.29 23:42\tCorporate Reward Payout\t15,000,000 ISK\t189,968,896 ISK\tCONCORD rewarded tomar norris for services performed.\n\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t189,218,896 ISK\tCONCORD rewarded tomar norris for services performed.\n\
2026.07.29 22:00\tAgent Mission Reward\t1,000,000 ISK\t100 ISK\tnoise\n\
2026.07.29 21:00\tCorporate Reward Payout\t10,395,000 ISK\t100 ISK\twrong amount\n";

    #[test]
    fn keeps_matching_payouts_sorted() {
        let r = parse_wallet_journal(SAMPLE, 15_000_000);
        assert_eq!(r.events.len(), 2);
        assert_eq!(r.events[0].occurred_at.timestamp(), {
            use chrono::TimeZone;
            Utc.with_ymd_and_hms(2026, 7, 29, 23, 8, 0)
                .unwrap()
                .timestamp()
        });
        assert_eq!(r.ignored_lines, 2);
    }

    #[test]
    fn drops_duplicates() {
        let text = format!("{SAMPLE}2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tdup\n");
        let r = parse_wallet_journal(&text, 15_000_000);
        assert_eq!(r.events.len(), 2);
        assert_eq!(r.duplicates_dropped, 1);
    }

    #[test]
    fn extracts_fc_hint_from_concord_rewarded_description() {
        assert_eq!(
            extract_wallet_fc_hint("CONCORD rewarded tomar norris for services performed."),
            Some("tomar norris".to_string())
        );
    }

    #[test]
    fn extracts_fc_hint_returns_none_for_unrelated_description() {
        assert_eq!(extract_wallet_fc_hint("noise"), None);
        assert_eq!(extract_wallet_fc_hint(""), None);
    }
}
