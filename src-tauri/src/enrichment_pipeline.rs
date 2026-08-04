//! Enrichment pipeline: settings + gamelog scan → `enrich_run` → persist,
//! plus load-and-aggregate for Spawn/Overall focus.
//!
//! Pure math lives in `enrichment`. The run desk calls through this module
//! via [`EnrichmentStore`] and [`GamelogScan`] adapters.

use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::analytics_types::{Diagnostic, EnrichmentSnapshot};
use crate::db::{Db, EnrichmentLoad};
use crate::enrichment::{aggregate_enrichments, enrich_run};
use crate::gamelog_scan::{default_gamelogs_dir, scan_gamelogs, ScanResult};
use crate::timing::{AnalyticsReport, RunSettings};
use crate::types::AppSettings;
use crate::wallet_parse::{extract_wallet_fc_hint, parse_wallet_journal};

/// Narrow DB seam used by the enrichment pipeline.
pub trait EnrichmentStore: Send + Sync {
    fn get_settings(
        &self,
    ) -> impl std::future::Future<Output = Result<AppSettings, String>> + Send;

    fn save_enrichment(
        &self,
        run_id: &str,
        snapshot: &EnrichmentSnapshot,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    fn load_enrichment_status(
        &self,
        run_id: &str,
    ) -> impl std::future::Future<Output = Result<EnrichmentLoad, String>> + Send;

    fn load_run_for_enrich(
        &self,
        run_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<(RunSettings, AnalyticsReport, String)>, String>>
           + Send;
}

impl EnrichmentStore for Db {
    async fn get_settings(&self) -> Result<AppSettings, String> {
        Db::get_settings(self).await.map_err(|e| e.to_string())
    }

    async fn save_enrichment(
        &self,
        run_id: &str,
        snapshot: &EnrichmentSnapshot,
    ) -> Result<(), String> {
        Db::save_enrichment(self, run_id, snapshot)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_enrichment_status(&self, run_id: &str) -> Result<EnrichmentLoad, String> {
        Db::load_enrichment_status(self, run_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_run_for_enrich(
        &self,
        run_id: &str,
    ) -> Result<Option<(RunSettings, AnalyticsReport, String)>, String> {
        Db::load_run_for_enrich(self, run_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Filesystem / scan seam for gamelogs.
pub trait GamelogScan: Send + Sync {
    fn scan(&self, dir: &Path, wallet_start: DateTime<Utc>, wallet_end: DateTime<Utc>)
        -> ScanResult;
}

/// Production adapter: `scan_gamelogs` on disk.
pub struct FsGamelogScan;

impl GamelogScan for FsGamelogScan {
    fn scan(
        &self,
        dir: &Path,
        wallet_start: DateTime<Utc>,
        wallet_end: DateTime<Utc>,
    ) -> ScanResult {
        scan_gamelogs(dir, wallet_start, wallet_end)
    }
}

/// Scan gamelogs (when the directory exists), run enrichment math, persist.
pub async fn enrich_and_save<S: EnrichmentStore, G: GamelogScan>(
    store: &S,
    scan: &G,
    run_id: &str,
    settings: &RunSettings,
    report: &AnalyticsReport,
    wallet_text: &str,
) -> Result<(), String> {
    if report.sites.is_empty() {
        return Ok(());
    }
    let site_times: Vec<DateTime<Utc>> = report.sites.iter().map(|s| s.occurred_at).collect();
    let site_durations: Vec<Option<i64>> =
        report.sites.iter().map(|s| s.duration_seconds).collect();

    let app_settings = store.get_settings().await?;
    let gamelogs_dir = app_settings
        .gamelogs_dir
        .as_ref()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_gamelogs_dir);
    let wallet_start = settings.run_start.unwrap_or(site_times[0]);
    let wallet_end = *site_times.last().unwrap();

    let launchers = app_settings.ammo_launchers.max(0) as u32;
    let missiles_per_cycle = launchers.saturating_mul(app_settings.ammo_per_launcher.max(0) as u32);

    let scan_result = if gamelogs_dir.is_dir() {
        scan.scan(&gamelogs_dir, wallet_start, wallet_end)
    } else {
        ScanResult {
            logs: Vec::new(),
            diagnostics: vec![format!(
                "Gamelogs directory not found: {} — enrichment unavailable",
                gamelogs_dir.display()
            )],
        }
    };

    let wallet_fc_hint = parse_wallet_journal(wallet_text, settings.expected_isk)
        .events
        .first()
        .and_then(|e| extract_wallet_fc_hint(&e.description));

    let mut snapshot = enrich_run(
        &scan_result.logs,
        &site_times,
        &site_durations,
        settings.break_threshold_minutes,
        settings.run_start,
        missiles_per_cycle,
        launchers,
        app_settings.fc_character.as_deref(),
        wallet_fc_hint.as_deref(),
    );
    snapshot
        .diagnostics
        .extend(scan_result.diagnostics.into_iter().map(|message| Diagnostic {
            level: "warn".into(),
            message,
        }));

    store.save_enrichment(run_id, &snapshot).await
}

/// Recompute and persist enrichment for an already-sealed run.
pub async fn reenrich<S: EnrichmentStore, G: GamelogScan>(
    store: &S,
    scan: &G,
    run_id: &str,
) -> Result<(), String> {
    let bundle = store
        .load_run_for_enrich(run_id)
        .await?
        .ok_or_else(|| format!("Run {run_id} not found"))?;
    let (settings, report, wallet_text) = bundle;
    enrich_and_save(store, scan, run_id, &settings, &report, &wallet_text).await
}

/// Load each run's enrichment (oldest first) and aggregate the ones that
/// read cleanly. Missing/stale runs are silently skipped here; they only
/// show up as the "N of M lack enrichment" diagnostic from
/// `aggregate_enrichments`.
pub async fn load_and_aggregate<S: EnrichmentStore>(
    store: &S,
    run_ids: &[String],
) -> Result<(Option<EnrichmentSnapshot>, Vec<Diagnostic>), String> {
    if run_ids.is_empty() {
        return Ok((None, Vec::new()));
    }
    let mut runs = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
        if let EnrichmentLoad::Ok(snapshot) = store.load_enrichment_status(run_id).await? {
            runs.push((run_id.clone(), snapshot));
        }
    }
    let aggregate = aggregate_enrichments(&runs, run_ids.len());
    Ok((Some(aggregate), Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics_types::{
        EnrichmentSite, EnrichmentSource, EnrichmentTotals, MissileStat,
    };
    use crate::gamelog_scan::ListenerLog;
    use crate::timing::{AnalyticsReport, RunSettings, SessionSummary, SiteDetail};
    use chrono::TimeZone;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct FakeStore {
        settings: AppSettings,
        saved: Mutex<Vec<(String, EnrichmentSnapshot)>>,
        runs: Mutex<HashMap<String, (RunSettings, AnalyticsReport, String)>>,
        enrichments: Mutex<HashMap<String, EnrichmentLoad>>,
    }

    impl FakeStore {
        fn new(settings: AppSettings) -> Self {
            Self {
                settings,
                saved: Mutex::new(Vec::new()),
                runs: Mutex::new(HashMap::new()),
                enrichments: Mutex::new(HashMap::new()),
            }
        }
    }

    impl EnrichmentStore for FakeStore {
        async fn get_settings(&self) -> Result<AppSettings, String> {
            Ok(self.settings.clone())
        }

        async fn save_enrichment(
            &self,
            run_id: &str,
            snapshot: &EnrichmentSnapshot,
        ) -> Result<(), String> {
            self.saved
                .lock()
                .push((run_id.to_string(), snapshot.clone()));
            self.enrichments
                .lock()
                .insert(run_id.to_string(), EnrichmentLoad::Ok(snapshot.clone()));
            Ok(())
        }

        async fn load_enrichment_status(&self, run_id: &str) -> Result<EnrichmentLoad, String> {
            Ok(self
                .enrichments
                .lock()
                .get(run_id)
                .cloned()
                .unwrap_or(EnrichmentLoad::Missing))
        }

        async fn load_run_for_enrich(
            &self,
            run_id: &str,
        ) -> Result<Option<(RunSettings, AnalyticsReport, String)>, String> {
            Ok(self.runs.lock().get(run_id).cloned())
        }
    }

    struct CannedScan {
        result: ScanResult,
        calls: Arc<Mutex<u32>>,
    }

    impl GamelogScan for CannedScan {
        fn scan(
            &self,
            _dir: &Path,
            _wallet_start: DateTime<Utc>,
            _wallet_end: DateTime<Utc>,
        ) -> ScanResult {
            *self.calls.lock() += 1;
            self.result.clone()
        }
    }

    fn empty_report_with_sites(sites: Vec<SiteDetail>) -> AnalyticsReport {
        AnalyticsReport {
            sites,
            hourly: vec![],
            session: SessionSummary {
                sites_ran: 0,
                active_site_seconds: 0,
                wallet_elapsed_seconds: 0,
                avg_site_seconds: None,
                character_liquid_isk: 0,
                fleet_liquid_isk: 0,
                net_lp: 0,
                lp_per_character_total: None,
                lp_value: 0.0,
                net_value: 0.0,
                liquid_isk_per_hour: 0.0,
                lp_value_per_hour: 0.0,
                net_per_hour: 0.0,
            },
        }
    }

    fn site_at(t: DateTime<Utc>) -> SiteDetail {
        SiteDetail {
            occurred_at: t,
            amount_isk: 15_000_000,
            fleet_isk: 225_000_000,
            fleet_lp: 30_000,
            gap_seconds: Some(600),
            duration_seconds: Some(600),
            is_break: false,
            counts_toward_avg: true,
        }
    }

    #[tokio::test]
    async fn enrich_and_save_skips_empty_sites() {
        let store = FakeStore::new(AppSettings::default());
        let calls = Arc::new(Mutex::new(0u32));
        let scan = CannedScan {
            result: ScanResult {
                logs: vec![],
                diagnostics: vec![],
            },
            calls: calls.clone(),
        };
        let report = empty_report_with_sites(vec![]);
        enrich_and_save(
            &store,
            &scan,
            "run-1",
            &RunSettings::default(),
            &report,
            "",
        )
        .await
        .unwrap();
        assert!(store.saved.lock().is_empty());
        assert_eq!(*calls.lock(), 0);
    }

    #[tokio::test]
    async fn enrich_and_save_scans_and_persists_when_gamelogs_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = AppSettings::default();
        settings.gamelogs_dir = Some(dir.path().display().to_string());
        let store = FakeStore::new(settings);
        let calls = Arc::new(Mutex::new(0u32));
        let scan = CannedScan {
            result: ScanResult {
                logs: vec![ListenerLog {
                    listener: "Pilot".into(),
                    path: PathBuf::from("Pilot.txt"),
                    events: vec![],
                }],
                diagnostics: vec![],
            },
            calls: calls.clone(),
        };
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 20, 10, 0).unwrap();
        let report = empty_report_with_sites(vec![site_at(t0)]);

        enrich_and_save(
            &store,
            &scan,
            "run-1",
            &RunSettings::default(),
            &report,
            "",
        )
        .await
        .unwrap();

        assert_eq!(*calls.lock(), 1);
        let saved = store.saved.lock();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].0, "run-1");
        assert_eq!(saved[0].1.listeners, vec!["Pilot".to_string()]);
    }

    #[tokio::test]
    async fn enrich_and_save_skips_scan_when_gamelogs_dir_missing() {
        let mut settings = AppSettings::default();
        settings.gamelogs_dir = Some("C:\\definitely\\missing\\gamelogs-xyz".into());
        let store = FakeStore::new(settings);
        let calls = Arc::new(Mutex::new(0u32));
        let scan = CannedScan {
            result: ScanResult {
                logs: vec![],
                diagnostics: vec![],
            },
            calls: calls.clone(),
        };
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 20, 10, 0).unwrap();
        let report = empty_report_with_sites(vec![site_at(t0)]);

        enrich_and_save(
            &store,
            &scan,
            "run-1",
            &RunSettings::default(),
            &report,
            "",
        )
        .await
        .unwrap();

        assert_eq!(*calls.lock(), 0);
        let saved = store.saved.lock();
        assert_eq!(saved.len(), 1);
        assert!(
            saved[0]
                .1
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Gamelogs directory not found")),
            "diagnostics: {:?}",
            saved[0].1.diagnostics
        );
    }

    #[tokio::test]
    async fn reenrich_loads_sealed_run_then_saves() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = AppSettings::default();
        settings.gamelogs_dir = Some(dir.path().display().to_string());
        let store = FakeStore::new(settings);
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 20, 10, 0).unwrap();
        let report = empty_report_with_sites(vec![site_at(t0)]);
        store.runs.lock().insert(
            "run-9".into(),
            (RunSettings::default(), report, String::new()),
        );
        let calls = Arc::new(Mutex::new(0u32));
        let scan = CannedScan {
            result: ScanResult {
                logs: vec![],
                diagnostics: vec![],
            },
            calls: calls.clone(),
        };

        reenrich(&store, &scan, "run-9").await.unwrap();
        assert_eq!(*calls.lock(), 1);
        assert_eq!(store.saved.lock().len(), 1);
        assert_eq!(store.saved.lock()[0].0, "run-9");
    }

    #[tokio::test]
    async fn load_and_aggregate_skips_missing_and_stale() {
        let store = FakeStore::new(AppSettings::default());
        let t0 = Utc.with_ymd_and_hms(2026, 8, 2, 20, 10, 0).unwrap();
        let ok = EnrichmentSnapshot {
            resolved_fc: Some("FC".into()),
            listeners: vec!["A".into()],
            diagnostics: vec![],
            sites: vec![EnrichmentSite {
                occurred_at: t0,
                approach_seconds: Some(30),
                combat_to_payout_seconds: Some(60),
                is_break: false,
                source: EnrichmentSource::Fc,
                missiles: vec![],
            }],
            missiles: vec![MissileStat {
                listener: "A".into(),
                reload_cycles: 1,
                hits: 2,
                missiles_per_cycle: 156,
                launchers: 6,
                dead: 0,
            }],
            totals: EnrichmentTotals {
                approach_seconds: Some(30),
                combat_to_payout_seconds: Some(60),
                avg_combat_to_payout_seconds: Some(60.0),
                fleet_dead: 0,
            },
        };
        store
            .enrichments
            .lock()
            .insert("run-ok".into(), EnrichmentLoad::Ok(ok));
        store
            .enrichments
            .lock()
            .insert("run-stale".into(), EnrichmentLoad::Stale);

        let ids = vec![
            "run-ok".to_string(),
            "run-stale".to_string(),
            "run-missing".to_string(),
        ];
        let (aggregate, diags) = load_and_aggregate(&store, &ids).await.unwrap();
        assert!(diags.is_empty());
        let aggregate = aggregate.expect("aggregate snapshot");
        assert_eq!(aggregate.sites.len(), 1);
        assert!(
            aggregate
                .diagnostics
                .iter()
                .any(|d| d.message == "2 of 3 runs lack enrichment"),
            "diagnostics: {:?}",
            aggregate.diagnostics
        );
    }
}
