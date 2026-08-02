use sha2::{Digest, Sha256};

use crate::types::SiteCandidate;

/// Stable site identity for Ran persistence across re-parses.
pub fn site_id(fleet_log_id: &str, candidate: &SiteCandidate) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fleet_log_id.as_bytes());
    hasher.update(b"|");
    hasher.update(candidate.posted_at.to_rfc3339().as_bytes());
    hasher.update(b"|");
    hasher.update(candidate.speaker.as_bytes());
    hasher.update(b"|");
    hasher.update(candidate.tag.as_bytes());
    hasher.update(b"|");
    hasher.update(candidate.line_ordinal.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_site_candidates;
    use chrono::TimeZone;
    use chrono::Utc;

    const SAMPLE: &str = include_str!("../tests/fixtures/fleet_sample.txt");

    #[test]
    fn site_id_stable_across_reparses() {
        let a = parse_site_candidates(SAMPLE);
        let b = parse_site_candidates(SAMPLE);
        let log = "Fleet_20260801_182319_2112707019.txt";
        let ids_a: Vec<_> = a.iter().map(|c| site_id(log, c)).collect();
        let ids_b: Vec<_> = b.iter().map(|c| site_id(log, c)).collect();
        assert_eq!(ids_a, ids_b);
    }

    #[test]
    fn stacked_same_tag_get_distinct_ids() {
        let sites = parse_site_candidates(SAMPLE);
        let log = "fleet.log";
        let ones: Vec<_> = sites
            .iter()
            .filter(|s| s.tag == "1")
            .map(|c| site_id(log, c))
            .collect();
        assert_eq!(ones.len(), 3);
        assert_ne!(ones[0], ones[1]);
        assert_ne!(ones[1], ones[2]);
    }

    #[test]
    fn different_logs_differ() {
        let c = SiteCandidate {
            speaker: "A".into(),
            posted_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            tag: "1".into(),
            line_ordinal: 0,
        };
        assert_ne!(site_id("log-a", &c), site_id("log-b", &c));
    }
}
