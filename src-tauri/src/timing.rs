//! Site timing, break detection, hourly buckets, and session summary.

use chrono::{DateTime, Timelike, Utc};

use crate::wallet_parse::WalletPayout;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunSettings {
    pub space: crate::vanguard_payouts::SpaceBand,
    pub fleet_size: u32,
    pub expected_isk: i64,
    pub lp_per_char: i64,
    pub isk_per_lp: f64,
    /// Minutes; gaps strictly greater than this are breaks.
    pub break_threshold_minutes: u32,
    pub run_start: Option<DateTime<Utc>>,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            space: crate::vanguard_payouts::SpaceBand::LowNull,
            fleet_size: 15,
            expected_isk: 15_000_000,
            lp_per_char: 2_000,
            isk_per_lp: 1400.0,
            break_threshold_minutes: 25,
            run_start: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SiteDetail {
    pub occurred_at: DateTime<Utc>,
    pub amount_isk: i64,
    pub fleet_lp: i64,
    pub gap_seconds: Option<i64>,
    pub duration_seconds: Option<i64>,
    pub is_break: bool,
    pub counts_toward_avg: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HourlyBucket {
    pub hour_start: DateTime<Utc>,
    pub total_isk: i64,
    pub total_lp: i64,
    pub sites: u32,
    pub avg_site_seconds: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub sites_ran: u32,
    pub time_spent_seconds: i64,
    pub avg_site_seconds: Option<f64>,
    pub liquid_isk: i64,
    pub net_lp: i64,
    pub lp_value: f64,
    pub net_value: f64,
    pub liquid_isk_per_hour: f64,
    pub lp_value_per_hour: f64,
    pub net_per_hour: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalyticsReport {
    pub session: SessionSummary,
    pub hourly: Vec<HourlyBucket>,
    pub sites: Vec<SiteDetail>,
}

fn hour_floor(ts: DateTime<Utc>) -> DateTime<Utc> {
    ts.date_naive()
        .and_hms_opt(ts.hour(), 0, 0)
        .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc))
        .unwrap_or(ts)
}

/// Build timed site details + aggregates from sorted payout events.
pub fn build_report(events: &[WalletPayout], settings: &RunSettings) -> AnalyticsReport {
    let break_secs = i64::from(settings.break_threshold_minutes) * 60;
    let fleet_lp = settings.lp_per_char * i64::from(settings.fleet_size);

    let mut sites: Vec<SiteDetail> = Vec::with_capacity(events.len());
    for (i, ev) in events.iter().enumerate() {
        let (gap_seconds, duration_seconds, is_break, counts_toward_avg) = if i == 0 {
            match settings.run_start {
                Some(start) if start <= ev.occurred_at => {
                    let gap = (ev.occurred_at - start).num_seconds();
                    let is_break = gap > break_secs;
                    (
                        Some(gap),
                        if is_break { None } else { Some(gap) },
                        is_break,
                        !is_break,
                    )
                }
                _ => (None, None, false, false),
            }
        } else {
            let prev = events[i - 1].occurred_at;
            let gap = (ev.occurred_at - prev).num_seconds();
            let is_break = gap > break_secs;
            (
                Some(gap),
                if is_break { None } else { Some(gap) },
                is_break,
                !is_break,
            )
        };

        sites.push(SiteDetail {
            occurred_at: ev.occurred_at,
            amount_isk: ev.amount_isk,
            fleet_lp,
            gap_seconds,
            duration_seconds,
            is_break,
            counts_toward_avg,
        });
    }

    let sites_ran = sites.len() as u32;
    let liquid_isk: i64 = sites.iter().map(|s| s.amount_isk).sum();
    let net_lp: i64 = sites.iter().map(|s| s.fleet_lp).sum();
    let timed: Vec<i64> = sites
        .iter()
        .filter(|s| s.counts_toward_avg)
        .filter_map(|s| s.duration_seconds)
        .collect();
    let time_spent_seconds: i64 = timed.iter().sum();
    let avg_site_seconds = if timed.is_empty() {
        None
    } else {
        Some(time_spent_seconds as f64 / timed.len() as f64)
    };

    let lp_value = net_lp as f64 * settings.isk_per_lp;
    let net_value = liquid_isk as f64 + lp_value;
    let hours = if time_spent_seconds > 0 {
        time_spent_seconds as f64 / 3600.0
    } else {
        0.0
    };
    let (liquid_isk_per_hour, lp_value_per_hour, net_per_hour) = if hours > 0.0 {
        (
            liquid_isk as f64 / hours,
            lp_value / hours,
            net_value / hours,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    // Hourly buckets
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<i64, Vec<&SiteDetail>> = BTreeMap::new();
    for s in &sites {
        let h = hour_floor(s.occurred_at);
        buckets.entry(h.timestamp()).or_default().push(s);
    }
    let hourly: Vec<HourlyBucket> = buckets
        .into_iter()
        .map(|(_, group)| {
            let hour_start = hour_floor(group[0].occurred_at);
            let total_isk: i64 = group.iter().map(|s| s.amount_isk).sum();
            let total_lp: i64 = group.iter().map(|s| s.fleet_lp).sum();
            let sites_n = group.len() as u32;
            let timed: Vec<i64> = group
                .iter()
                .filter(|s| s.counts_toward_avg)
                .filter_map(|s| s.duration_seconds)
                .collect();
            let avg_site_seconds = if timed.is_empty() {
                None
            } else {
                Some(timed.iter().sum::<i64>() as f64 / timed.len() as f64)
            };
            HourlyBucket {
                hour_start,
                total_isk,
                total_lp,
                sites: sites_n,
                avg_site_seconds,
            }
        })
        .collect();

    AnalyticsReport {
        session: SessionSummary {
            sites_ran,
            time_spent_seconds,
            avg_site_seconds,
            liquid_isk,
            net_lp,
            lp_value,
            net_value,
            liquid_isk_per_hour,
            lp_value_per_hour,
            net_per_hour,
        },
        hourly,
        sites,
    }
}

/// Merge multiple reports (spawn/overall aggregates).
pub fn merge_reports(reports: &[AnalyticsReport], isk_per_lp: f64) -> AnalyticsReport {
    if reports.is_empty() {
        return build_report(&[], &RunSettings::default());
    }
    let mut all_sites: Vec<SiteDetail> = Vec::new();
    for r in reports {
        all_sites.extend(r.sites.clone());
    }
    all_sites.sort_by_key(|s| s.occurred_at);

    // Rebuild summary from merged sites (hourly from merged)
    let sites_ran = all_sites.len() as u32;
    let liquid_isk: i64 = all_sites.iter().map(|s| s.amount_isk).sum();
    let net_lp: i64 = all_sites.iter().map(|s| s.fleet_lp).sum();
    let timed: Vec<i64> = all_sites
        .iter()
        .filter(|s| s.counts_toward_avg)
        .filter_map(|s| s.duration_seconds)
        .collect();
    let time_spent_seconds: i64 = timed.iter().sum();
    let avg_site_seconds = if timed.is_empty() {
        None
    } else {
        Some(time_spent_seconds as f64 / timed.len() as f64)
    };
    let lp_value = net_lp as f64 * isk_per_lp;
    let net_value = liquid_isk as f64 + lp_value;
    let hours = if time_spent_seconds > 0 {
        time_spent_seconds as f64 / 3600.0
    } else {
        0.0
    };
    let (liquid_isk_per_hour, lp_value_per_hour, net_per_hour) = if hours > 0.0 {
        (
            liquid_isk as f64 / hours,
            lp_value / hours,
            net_value / hours,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<i64, Vec<&SiteDetail>> = BTreeMap::new();
    for s in &all_sites {
        let h = hour_floor(s.occurred_at);
        buckets.entry(h.timestamp()).or_default().push(s);
    }
    let hourly: Vec<HourlyBucket> = buckets
        .into_iter()
        .map(|(_, group)| {
            let hour_start = hour_floor(group[0].occurred_at);
            let total_isk: i64 = group.iter().map(|s| s.amount_isk).sum();
            let total_lp: i64 = group.iter().map(|s| s.fleet_lp).sum();
            let sites_n = group.len() as u32;
            let timed: Vec<i64> = group
                .iter()
                .filter(|s| s.counts_toward_avg)
                .filter_map(|s| s.duration_seconds)
                .collect();
            let avg_site_seconds = if timed.is_empty() {
                None
            } else {
                Some(timed.iter().sum::<i64>() as f64 / timed.len() as f64)
            };
            HourlyBucket {
                hour_start,
                total_isk,
                total_lp,
                sites: sites_n,
                avg_site_seconds,
            }
        })
        .collect();

    AnalyticsReport {
        session: SessionSummary {
            sites_ran,
            time_spent_seconds,
            avg_site_seconds,
            liquid_isk,
            net_lp,
            lp_value,
            net_value,
            liquid_isk_per_hour,
            lp_value_per_hour,
            net_per_hour,
        },
        hourly,
        sites: all_sites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn payout(y: i32, m: u32, d: u32, h: u32, min: u32) -> WalletPayout {
        WalletPayout {
            occurred_at: Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap(),
            amount_isk: 15_000_000,
            ref_type: "Corporate Reward Payout".into(),
            description: String::new(),
        }
    }

    #[test]
    fn gaps_and_break() {
        let events = vec![
            payout(2026, 7, 29, 23, 0),
            payout(2026, 7, 29, 23, 6),  // 6 min
            payout(2026, 7, 29, 23, 50), // 44 min break
            payout(2026, 7, 29, 23, 56), // 6 min
        ];
        let mut settings = RunSettings::default();
        settings.run_start = Some(Utc.with_ymd_and_hms(2026, 7, 29, 22, 54, 0).unwrap());
        settings.break_threshold_minutes = 25;
        let report = build_report(&events, &settings);
        assert_eq!(report.session.sites_ran, 4);
        assert!(report.sites[2].is_break);
        assert!(!report.sites[1].is_break);
        // timed: first 6m + second 6m + fourth 6m = 18m (break excluded)
        assert_eq!(report.session.time_spent_seconds, 6 * 60 * 3);
    }

    #[test]
    fn first_site_without_run_start_excluded_from_avg() {
        let events = vec![payout(2026, 7, 29, 23, 0), payout(2026, 7, 29, 23, 5)];
        let settings = RunSettings::default();
        let report = build_report(&events, &settings);
        assert!(!report.sites[0].counts_toward_avg);
        assert!(report.sites[1].counts_toward_avg);
        assert_eq!(report.session.time_spent_seconds, 5 * 60);
    }
}
