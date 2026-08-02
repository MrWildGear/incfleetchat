//! Eve Uni Vanguard site payout presets (ISK + LP per character).

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceBand {
    Highsec,
    LowNull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PayoutTicket {
    pub isk: i64,
    pub lp_per_char: i64,
}

/// Look up expected per-character ticket for space band + fleet size.
/// Fleet sizes below the soft-cap still receive the maximum individual payout.
pub fn lookup_payout(space: SpaceBand, fleet_size: u32) -> PayoutTicket {
    let size = fleet_size.max(1);
    match space {
        SpaceBand::LowNull => match size {
            1..=15 => PayoutTicket {
                isk: 15_000_000,
                lp_per_char: 2_000,
            },
            16 => PayoutTicket {
                isk: 13_875_000,
                lp_per_char: 1_850,
            },
            17 => PayoutTicket {
                isk: 12_674_000,
                lp_per_char: 1_690,
            },
            _ => PayoutTicket {
                isk: 12_674_000,
                lp_per_char: 1_690,
            },
        },
        SpaceBand::Highsec => match size {
            1..=10 => PayoutTicket {
                isk: 10_395_000,
                lp_per_char: 1_400,
            },
            11 => PayoutTicket {
                isk: 9_615_375,
                lp_per_char: 1_295,
            },
            12 => PayoutTicket {
                isk: 8_783_775,
                lp_per_char: 1_183,
            },
            13 => PayoutTicket {
                isk: 7_900_200,
                lp_per_char: 1_064,
            },
            _ => PayoutTicket {
                isk: 7_900_200,
                lp_per_char: 1_064,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_null_at_cap() {
        let t = lookup_payout(SpaceBand::LowNull, 15);
        assert_eq!(t.isk, 15_000_000);
        assert_eq!(t.lp_per_char, 2_000);
    }

    #[test]
    fn low_null_sixteen() {
        let t = lookup_payout(SpaceBand::LowNull, 16);
        assert_eq!(t.isk, 13_875_000);
        assert_eq!(t.lp_per_char, 1_850);
    }

    #[test]
    fn highsec_at_cap() {
        let t = lookup_payout(SpaceBand::Highsec, 10);
        assert_eq!(t.isk, 10_395_000);
        assert_eq!(t.lp_per_char, 1_400);
    }

    #[test]
    fn highsec_eleven() {
        let t = lookup_payout(SpaceBand::Highsec, 11);
        assert_eq!(t.isk, 9_615_375);
        assert_eq!(t.lp_per_char, 1_295);
    }
}
