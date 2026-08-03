//! RunDesk document-session: trays → analyze → focus.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::path::PathBuf;

use crate::analytics_types::*;
use crate::db::{Db, EnrichmentLoad};
use crate::enrichment::enrich_run;
use crate::gamelog_scan::{default_gamelogs_dir, scan_gamelogs, ScanResult};
use crate::spawn_parse::{parse_manifest, SpawnDraft};
use crate::timing::{build_report, merge_reports, AnalyticsReport, RunSettings};
use crate::wallet_parse::{extract_wallet_fc_hint, parse_wallet_journal, WalletPayout};

pub struct RunDesk {
    db: Db,
    inner: Mutex<DeskState>,
}

struct DeskState {
    settings: RunSettings,
    manifest_text: String,
    staging_spawn: Option<SpawnDraft>,
    wallet_text: String,
    wallet_batches: u32,
    pending: Vec<WalletPayout>,
    diagnostics: Vec<Diagnostic>,
    scope: ReportScope,
    sealed_run_id: Option<String>,
}

impl RunDesk {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            inner: Mutex::new(DeskState {
                settings: RunSettings::default(),
                manifest_text: String::new(),
                staging_spawn: None,
                wallet_text: String::new(),
                wallet_batches: 0,
                pending: Vec::new(),
                diagnostics: Vec::new(),
                scope: ReportScope::Overall,
                sealed_run_id: None,
            }),
        }
    }

    pub async fn open(&self) -> Result<EditionFocus, String> {
        self.snapshot().await
    }

    pub async fn paste(&self, tray: Tray, text: &str) -> Result<EditionFocus, String> {
        {
            let mut st = self.inner.lock();
            st.diagnostics.clear();
            match tray {
                Tray::Manifest => {
                    st.manifest_text = text.to_string();
                    let draft = parse_manifest(text);
                    if draft.constellation.is_none() {
                        st.diagnostics.push(Diagnostic {
                            level: "warn".into(),
                            message: "Manifest paste missing constellation".into(),
                        });
                    }
                    st.staging_spawn = Some(draft);
                }
                Tray::Wallet => {
                    if st.wallet_batches == 0 {
                        st.wallet_text = text.to_string();
                    } else {
                        st.wallet_text.push('\n');
                        st.wallet_text.push_str(text);
                    }
                    st.wallet_batches += 1;
                    let parsed = parse_wallet_journal(&st.wallet_text, st.settings.expected_isk);
                    st.pending = parsed.events;
                    if parsed.ignored_lines > 0 {
                        st.diagnostics.push(Diagnostic {
                            level: "info".into(),
                            message: format!(
                                "{} wallet lines ignored (type/amount/parse)",
                                parsed.ignored_lines
                            ),
                        });
                    }
                    if parsed.duplicates_dropped > 0 {
                        st.diagnostics.push(Diagnostic {
                            level: "info".into(),
                            message: format!(
                                "{} duplicate payouts dropped",
                                parsed.duplicates_dropped
                            ),
                        });
                    }
                }
            }
        }
        self.snapshot().await
    }

    pub async fn analyze(&self) -> Result<EditionFocus, String> {
        let (settings, spawn_draft, pending, wallet_text, manifest_text) = {
            let st = self.inner.lock();
            (
                st.settings.clone(),
                st.staging_spawn.clone(),
                st.pending.clone(),
                st.wallet_text.clone(),
                st.manifest_text.clone(),
            )
        };

        let constellation = spawn_draft
            .as_ref()
            .and_then(|s| s.constellation.clone())
            .ok_or_else(|| "Constellation required — paste a Manifest first".to_string())?;

        if pending.is_empty() {
            return Err("No qualifying Corporate Reward Payout rows for expected ISK".into());
        }

        let report = build_report(&pending, &settings);
        let run_id = format!(
            "{}-{}",
            constellation,
            Utc::now().timestamp_millis()
        );

        self.db
            .upsert_spawn(&constellation, spawn_draft.as_ref())
            .await
            .map_err(|e| e.to_string())?;
        self.db
            .save_run(
                &run_id,
                &constellation,
                &settings,
                &wallet_text,
                &manifest_text,
                &report,
            )
            .await
            .map_err(|e| e.to_string())?;

        let enrich_result = self
            .enrich_and_save(&run_id, &settings, &report, &wallet_text)
            .await;

        {
            let mut st = self.inner.lock();
            st.sealed_run_id = Some(run_id.clone());
            st.scope = ReportScope::Spawn {
                constellation: constellation.clone(),
            };
            // Prevent accidental duplicate Analyze of the same paste
            st.wallet_text.clear();
            st.wallet_batches = 0;
            st.pending.clear();
            st.diagnostics.push(Diagnostic {
                level: "info".into(),
                message: format!("Saved run {} ({} sites)", run_id, report.session.sites_ran),
            });
            if let Err(e) = enrich_result {
                st.diagnostics.push(Diagnostic {
                    level: "warn".into(),
                    message: format!("Gamelog enrichment skipped: {e}"),
                });
            }
        }

        self.snapshot().await
    }

    /// Scan gamelogs for `run_id`'s wallet window and persist an
    /// `EnrichmentSnapshot`. A missing gamelogs directory is a warning, not
    /// an error: an empty snapshot carrying that warning is saved so the v1
    /// report stays intact and the reason is visible in Tools.
    async fn enrich_and_save(
        &self,
        run_id: &str,
        settings: &RunSettings,
        report: &AnalyticsReport,
        wallet_text: &str,
    ) -> Result<(), String> {
        if report.sites.is_empty() {
            return Ok(());
        }
        let site_times: Vec<DateTime<Utc>> =
            report.sites.iter().map(|s| s.occurred_at).collect();

        let app_settings = self.db.get_settings().await.map_err(|e| e.to_string())?;
        let gamelogs_dir = app_settings
            .gamelogs_dir
            .as_ref()
            .map(|d| d.trim())
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_gamelogs_dir);
        let wallet_start = settings.run_start.unwrap_or(site_times[0]);
        let wallet_end = *site_times.last().unwrap();

        let missiles_per_cycle = (app_settings.ammo_launchers.max(0) as u32)
            .saturating_mul(app_settings.ammo_per_launcher.max(0) as u32);

        let scan = if gamelogs_dir.is_dir() {
            scan_gamelogs(&gamelogs_dir, wallet_start, wallet_end)
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
            &scan.logs,
            &site_times,
            settings.break_threshold_minutes,
            settings.run_start,
            missiles_per_cycle,
            app_settings.fc_character.as_deref(),
            wallet_fc_hint.as_deref(),
        );
        snapshot
            .diagnostics
            .extend(scan.diagnostics.into_iter().map(|message| Diagnostic {
                level: "warn".into(),
                message,
            }));

        self.db
            .save_enrichment(run_id, &snapshot)
            .await
            .map_err(|e| e.to_string())
    }

    /// Recompute and persist enrichment for an already-sealed run.
    async fn reenrich_run(&self, run_id: &str) -> Result<(), String> {
        let bundle = self
            .db
            .load_run_for_enrich(run_id)
            .await
            .map_err(|e| e.to_string())?;
        let (settings, report, wallet_text) =
            bundle.ok_or_else(|| format!("Run {run_id} not found"))?;
        self.enrich_and_save(run_id, &settings, &report, &wallet_text)
            .await
    }

    pub async fn focus(&self, scope: ReportScope) -> Result<EditionFocus, String> {
        self.inner.lock().scope = scope;
        self.snapshot().await
    }

    pub async fn amend(&self, op: AmendOp) -> Result<EditionFocus, String> {
        match op {
            AmendOp::ClearWalletTray => {
                let mut st = self.inner.lock();
                st.wallet_text.clear();
                st.wallet_batches = 0;
                st.pending.clear();
            }
            AmendOp::SetSessionSettings { settings } => {
                let mut st = self.inner.lock();
                st.settings = settings;
                // Re-filter pending from full wallet text
                if !st.wallet_text.is_empty() {
                    let parsed = parse_wallet_journal(&st.wallet_text, st.settings.expected_isk);
                    st.pending = parsed.events;
                }
            }
            AmendOp::ReopenTrays => {
                // Keep text; allow more wallet paste
            }
            AmendOp::OpenRun { run_id } => {
                self.inner.lock().scope = ReportScope::Run { run_id };
            }
            AmendOp::SetConstellation { constellation } => {
                let mut st = self.inner.lock();
                let mut draft = st.staging_spawn.clone().unwrap_or_default();
                draft.constellation = Some(constellation.trim().to_string());
                st.staging_spawn = Some(draft);
            }
            AmendOp::ReenrichRun { run_id } => {
                let ids: Vec<String> = match run_id {
                    Some(id) => vec![id],
                    None => {
                        let scope = self.inner.lock().scope.clone();
                        match scope {
                            ReportScope::Run { run_id } => vec![run_id],
                            ReportScope::Spawn { constellation } => self
                                .db
                                .list_run_ids_for_spawn(&constellation)
                                .await
                                .map_err(|e| e.to_string())?,
                            ReportScope::Overall => self
                                .db
                                .list_all_run_ids()
                                .await
                                .map_err(|e| e.to_string())?,
                        }
                    }
                };
                let total = ids.len();
                let mut ok = 0usize;
                let mut diags = Vec::new();
                for id in &ids {
                    match self.reenrich_run(id).await {
                        Ok(()) => ok += 1,
                        Err(e) => diags.push(Diagnostic {
                            level: "warn".into(),
                            message: format!("Failed re-enrich {id}: {e}"),
                        }),
                    }
                }
                if total > 0 {
                    diags.push(Diagnostic {
                        level: if ok == total { "info".into() } else { "warn".into() },
                        message: format!("{ok} of {total} re-enriched"),
                    });
                }
                self.inner.lock().diagnostics.extend(diags);
            }
        }
        self.snapshot().await
    }

    pub async fn delete_run(&self, run_id: &str) -> Result<EditionFocus, String> {
        let outcome = self.db.delete_run(run_id).await?;
        {
            let mut st = self.inner.lock();
            if st.sealed_run_id.as_deref() == Some(run_id) {
                st.sealed_run_id = None;
            }
            st.scope = if outcome.spawn_removed {
                ReportScope::Overall
            } else {
                ReportScope::Spawn {
                    constellation: outcome.constellation,
                }
            };
        }
        self.snapshot().await
    }

    pub async fn delete_spawn(&self, constellation: &str) -> Result<EditionFocus, String> {
        // Capture sealed id before DB wipe so we can decide clearance.
        let sealed = self.inner.lock().sealed_run_id.clone();
        let sealed_in_spawn = if let Some(ref id) = sealed {
            self.db
                .load_catalog()
                .await
                .map_err(|e| e.to_string())?
                .runs
                .iter()
                .any(|r| r.run_id == *id && r.constellation == constellation)
        } else {
            false
        };

        self.db.delete_spawn(constellation).await?;
        {
            let mut st = self.inner.lock();
            if sealed_in_spawn {
                st.sealed_run_id = None;
            }
            st.scope = ReportScope::Overall;
        }
        self.snapshot().await
    }

    pub async fn clear_all(&self) -> Result<EditionFocus, String> {
        self.db.clear_all_analytics().await?;
        {
            let mut st = self.inner.lock();
            st.sealed_run_id = None;
            st.scope = ReportScope::Overall;
        }
        self.snapshot().await
    }

    async fn snapshot(&self) -> Result<EditionFocus, String> {
        let catalog = self.db.load_catalog().await.map_err(|e| e.to_string())?;
        let (trays, spawn, scope, mut diagnostics, settings, staging_spawn, sealed_run_id) = {
            let st = self.inner.lock();
            let trays = TrayState {
                manifest: if st.staging_spawn
                    .as_ref()
                    .and_then(|s| s.constellation.as_ref())
                    .is_some()
                {
                    "staged".into()
                } else {
                    "empty".into()
                },
                wallet_batches: st.wallet_batches,
                pending_sites: st.pending.len() as u32,
            };
            let spawn = st
                .staging_spawn
                .as_ref()
                .and_then(|d| d.constellation.as_ref())
                .map(|c| {
                    let run_count = catalog
                        .spawns
                        .iter()
                        .find(|s| s.constellation == *c)
                        .map(|s| s.run_count)
                        .unwrap_or(0);
                    SpawnSummary {
                        constellation: c.clone(),
                        region: st
                            .staging_spawn
                            .as_ref()
                            .and_then(|s| s.region.clone()),
                        staging_system: st
                            .staging_spawn
                            .as_ref()
                            .and_then(|s| s.staging_system.clone()),
                        hq_system: st
                            .staging_spawn
                            .as_ref()
                            .and_then(|s| s.hq_system.clone()),
                        run_count,
                    }
                });
            (
                trays,
                spawn,
                st.scope.clone(),
                st.diagnostics.clone(),
                st.settings.clone(),
                st.staging_spawn.clone(),
                st.sealed_run_id.clone(),
            )
        };

        let (report, report_diagnostics) = self.report_for_scope(&scope, &settings).await?;
        diagnostics.extend(report_diagnostics);
        let (enrichment, enrichment_diagnostics) = self.enrichment_for_scope(&scope).await?;
        diagnostics.extend(enrichment_diagnostics);

        Ok(EditionFocus {
            trays,
            spawn,
            catalog,
            scope,
            report,
            diagnostics,
            session_settings: settings,
            staging_spawn,
            sealed_run_id,
            enrichment,
        })
    }

    /// Enrichment for the currently-focused scope: a single run's snapshot
    /// for `ReportScope::Run`, or the aggregate across every run in scope
    /// for `Spawn`/`Overall`. Returns diagnostics to merge into the top
    /// Analytics strip (stale-schema / partial-coverage warnings).
    async fn enrichment_for_scope(
        &self,
        scope: &ReportScope,
    ) -> Result<(Option<EnrichmentSnapshot>, Vec<Diagnostic>), String> {
        match scope {
            ReportScope::Run { run_id } => {
                match self
                    .db
                    .load_enrichment_status(run_id)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    EnrichmentLoad::Ok(snapshot) => Ok((Some(snapshot), Vec::new())),
                    EnrichmentLoad::Stale => Ok((
                        None,
                        vec![Diagnostic {
                            level: "warn".into(),
                            message: "Enrichment needs Re-enrich (schema outdated)".into(),
                        }],
                    )),
                    EnrichmentLoad::Missing => Ok((None, Vec::new())),
                }
            }
            ReportScope::Spawn { constellation } => {
                let run_ids = self
                    .db
                    .list_run_ids_for_spawn(constellation)
                    .await
                    .map_err(|e| e.to_string())?;
                self.load_and_aggregate_enrichment(&run_ids).await
            }
            ReportScope::Overall => {
                let run_ids = self.db.list_all_run_ids().await.map_err(|e| e.to_string())?;
                self.load_and_aggregate_enrichment(&run_ids).await
            }
        }
    }

    /// Load each run's enrichment (oldest first) and aggregate the ones that
    /// read cleanly. Missing/stale runs are silently skipped here; they only
    /// show up as the "N of M lack enrichment" diagnostic from
    /// `aggregate_enrichments`.
    async fn load_and_aggregate_enrichment(
        &self,
        run_ids: &[String],
    ) -> Result<(Option<EnrichmentSnapshot>, Vec<Diagnostic>), String> {
        if run_ids.is_empty() {
            return Ok((None, Vec::new()));
        }
        let mut runs = Vec::with_capacity(run_ids.len());
        for run_id in run_ids {
            if let EnrichmentLoad::Ok(snapshot) = self
                .db
                .load_enrichment_status(run_id)
                .await
                .map_err(|e| e.to_string())?
            {
                runs.push((run_id.clone(), snapshot));
            }
        }
        let aggregate = aggregate_enrichments(&runs, run_ids.len());
        Ok((Some(aggregate), Vec::new()))
    }

    /// Report for the focused scope, plus diagnostics for stored runs whose
    /// `report_json` could not be read (they are excluded from aggregates).
    async fn report_for_scope(
        &self,
        scope: &ReportScope,
        settings: &RunSettings,
    ) -> Result<(Option<AnalyticsReport>, Vec<Diagnostic>), String> {
        let rows = match scope {
            ReportScope::Overall => self.db.load_all_reports().await.map_err(|e| e.to_string())?,
            ReportScope::Spawn { constellation } => self
                .db
                .load_reports_for_spawn(constellation)
                .await
                .map_err(|e| e.to_string())?,
            ReportScope::Run { run_id } => {
                let report = self
                    .db
                    .load_report(run_id)
                    .await
                    .map_err(|e| e.to_string())?;
                let diagnostics = if report.is_none() {
                    vec![Diagnostic {
                        level: "warn".into(),
                        message: format!("Run {run_id} has no readable saved report"),
                    }]
                } else {
                    Vec::new()
                };
                return Ok((report, diagnostics));
            }
        };

        let mut diagnostics = Vec::new();
        if !rows.unreadable.is_empty() {
            diagnostics.push(Diagnostic {
                level: "warn".into(),
                message: format!(
                    "{} saved run(s) skipped — unreadable report: {}",
                    rows.unreadable.len(),
                    rows.unreadable.join(", ")
                ),
            });
        }
        let report = if rows.reports.is_empty() {
            None
        } else {
            Some(merge_reports(&rows.reports, settings.isk_per_lp))
        };
        Ok((report, diagnostics))
    }
}

/// Merge per-run enrichment snapshots into one for a Spawn/Overall scope.
///
/// `runs` holds `(run_id, snapshot)` for every run in scope with a readable
/// enrichment snapshot, in catalog order (oldest first, latest last) —
/// callers must sort this way so "latest run wins" merges (missiles/cycle,
/// `resolved_fc`) pick the right row. `scope_run_count` is the total number
/// of runs in scope, enriched or not, used for the partial-coverage warning.
///
/// This is enrichment-only aggregation: it never falls back to wallet-gap
/// timing for runs that lack enrichment, and never re-scans gamelogs.
pub fn aggregate_enrichments(
    runs: &[(String, EnrichmentSnapshot)],
    scope_run_count: usize,
) -> EnrichmentSnapshot {
    let mut sites: Vec<EnrichmentSite> = Vec::new();
    let mut missiles: Vec<MissileStat> = Vec::new();
    let mut listeners: Vec<String> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut resolved_fc: Option<String> = None;

    for (_run_id, snapshot) in runs {
        sites.extend(snapshot.sites.iter().cloned());

        for m in &snapshot.missiles {
            match missiles.iter_mut().find(|existing| existing.listener == m.listener) {
                Some(existing) => {
                    existing.reload_cycles += m.reload_cycles;
                    existing.hits += m.hits;
                    existing.dead += m.dead;
                    // Latest contributing run wins; do not sum missiles/cycle.
                    existing.missiles_per_cycle = m.missiles_per_cycle;
                }
                None => missiles.push(m.clone()),
            }
        }

        for listener in &snapshot.listeners {
            if !listeners.contains(listener) {
                listeners.push(listener.clone());
            }
        }

        for d in &snapshot.diagnostics {
            if !diagnostics
                .iter()
                .any(|existing: &Diagnostic| existing.level == d.level && existing.message == d.message)
            {
                diagnostics.push(d.clone());
            }
        }
        resolved_fc = snapshot.resolved_fc.clone();
    }

    if runs.len() < scope_run_count {
        diagnostics.push(Diagnostic {
            level: "warn".into(),
            message: format!(
                "{} of {} runs lack enrichment",
                scope_run_count - runs.len(),
                scope_run_count
            ),
        });
    }

    // Same totals rule as `enrich_run`: warp sums non-break sites; combat
    // total/avg only cover measurable (`Some`) sites and are `None` when
    // there are zero of those.
    let warp_seconds: i64 = sites
        .iter()
        .filter(|s| !s.is_break)
        .map(|s| s.warp_seconds)
        .sum();
    let combat_values: Vec<i64> = sites
        .iter()
        .filter(|s| !s.is_break)
        .filter_map(|s| s.combat_to_payout_seconds)
        .collect();
    let combat_to_payout_seconds = if combat_values.is_empty() {
        None
    } else {
        Some(combat_values.iter().sum())
    };
    let avg_combat_to_payout_seconds = if combat_values.is_empty() {
        None
    } else {
        Some(combat_values.iter().sum::<i64>() as f64 / combat_values.len() as f64)
    };
    let fleet_dead: u32 = missiles.iter().map(|m| m.dead).sum();

    EnrichmentSnapshot {
        resolved_fc,
        listeners,
        diagnostics,
        sites,
        missiles,
        totals: EnrichmentTotals {
            warp_seconds,
            combat_to_payout_seconds,
            avg_combat_to_payout_seconds,
            fleet_dead,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanguard_payouts::SpaceBand;
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn site(occurred_at: DateTime<Utc>, warp: i64, combat: Option<i64>, is_break: bool) -> EnrichmentSite {
        EnrichmentSite {
            occurred_at,
            warp_seconds: warp,
            combat_to_payout_seconds: combat,
            is_break,
            source: EnrichmentSource::Fc,
        }
    }

    fn missile(listener: &str, cycles: u32, hits: u32, per_cycle: u32, dead: u32) -> MissileStat {
        MissileStat {
            listener: listener.into(),
            reload_cycles: cycles,
            hits,
            missiles_per_cycle: per_cycle,
            dead,
        }
    }

    fn empty_snapshot(resolved_fc: Option<&str>) -> EnrichmentSnapshot {
        EnrichmentSnapshot {
            resolved_fc: resolved_fc.map(|s| s.to_string()),
            listeners: Vec::new(),
            diagnostics: Vec::new(),
            sites: Vec::new(),
            missiles: Vec::new(),
            totals: EnrichmentTotals {
                warp_seconds: 0,
                combat_to_payout_seconds: None,
                avg_combat_to_payout_seconds: None,
                fleet_dead: 0,
            },
        }
    }

    #[test]
    fn aggregate_sums_same_listener_and_keeps_latest_cycle() {
        let t0 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 10, 0).unwrap();

        let mut run1 = empty_snapshot(Some("FC One"));
        run1.listeners = vec!["A".into()];
        run1.sites = vec![site(t0, 60, Some(120), false)];
        run1.missiles = vec![missile("A", 1, 10, 156, 5)];

        let mut run2 = empty_snapshot(Some("FC Two"));
        run2.listeners = vec!["A".into()];
        run2.sites = vec![site(t1, 40, Some(80), false)];
        run2.missiles = vec![missile("A", 2, 20, 200, 7)];

        let runs = vec![("run-1".to_string(), run1), ("run-2".to_string(), run2)];
        let aggregate = aggregate_enrichments(&runs, 2);

        assert_eq!(aggregate.missiles.len(), 1);
        let a = &aggregate.missiles[0];
        assert_eq!(a.reload_cycles, 3);
        assert_eq!(a.hits, 30);
        assert_eq!(a.dead, 12);
        assert_eq!(a.missiles_per_cycle, 200);

        assert_eq!(aggregate.sites.len(), 2);
        assert_eq!(aggregate.totals.warp_seconds, 100);
        assert_eq!(aggregate.totals.combat_to_payout_seconds, Some(200));
        assert_eq!(aggregate.totals.avg_combat_to_payout_seconds, Some(100.0));
        assert_eq!(aggregate.totals.fleet_dead, 12);
        assert_eq!(aggregate.resolved_fc.as_deref(), Some("FC Two"));
        assert_eq!(aggregate.listeners, vec!["A".to_string()]);
    }

    #[test]
    fn aggregate_unions_listeners_and_excludes_break_sites_from_combat() {
        let t0 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 30, 0).unwrap();

        let mut run1 = empty_snapshot(Some("FC One"));
        run1.listeners = vec!["A".into()];
        run1.sites = vec![site(t0, 0, None, true)]; // break: excluded from combat avg
        run1.missiles = vec![missile("A", 1, 5, 156, 2)];

        let mut run2 = empty_snapshot(None);
        run2.listeners = vec!["B".into()];
        run2.sites = vec![site(t1, 30, Some(60), false)];
        run2.missiles = vec![missile("B", 1, 8, 156, 3)];

        let runs = vec![("run-1".to_string(), run1), ("run-2".to_string(), run2)];
        let aggregate = aggregate_enrichments(&runs, 2);

        assert_eq!(aggregate.listeners, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(aggregate.totals.warp_seconds, 30);
        assert_eq!(aggregate.totals.combat_to_payout_seconds, Some(60));
        assert_eq!(aggregate.totals.avg_combat_to_payout_seconds, Some(60.0));
        assert_eq!(aggregate.totals.fleet_dead, 5);
        // resolved_fc = last run's value, even when that value is None.
        assert_eq!(aggregate.resolved_fc, None);
    }

    #[test]
    fn aggregate_warns_when_scope_has_unenriched_runs() {
        let t0 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 0, 0).unwrap();
        let run1 = {
            let mut s = empty_snapshot(Some("FC One"));
            s.sites = vec![site(t0, 10, None, false)];
            s
        };
        let runs = vec![("run-1".to_string(), run1)];

        // 3 runs in scope, only 1 has readable enrichment.
        let aggregate = aggregate_enrichments(&runs, 3);

        assert!(
            aggregate
                .diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message == "2 of 3 runs lack enrichment"),
            "diagnostics: {:?}",
            aggregate.diagnostics
        );
    }

    #[test]
    fn aggregate_dedupes_repeated_diagnostics_across_runs() {
        let dup = Diagnostic {
            level: "warn".into(),
            message: "skipped malformed gamelog line".into(),
        };

        let mut run1 = empty_snapshot(Some("FC One"));
        run1.diagnostics = vec![dup.clone()];

        let mut run2 = empty_snapshot(Some("FC Two"));
        run2.diagnostics = vec![dup.clone()];

        let runs = vec![("run-1".to_string(), run1), ("run-2".to_string(), run2)];
        let aggregate = aggregate_enrichments(&runs, 2);

        let matching: Vec<_> = aggregate
            .diagnostics
            .iter()
            .filter(|d| d.level == dup.level && d.message == dup.message)
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected diagnostic to be deduped, got: {:?}",
            aggregate.diagnostics
        );
    }

    #[test]
    fn aggregate_warns_when_all_runs_lack_enrichment() {
        let aggregate = aggregate_enrichments(&[], 3);

        assert!(aggregate.sites.is_empty());
        assert!(aggregate.missiles.is_empty());
        assert!(
            aggregate.diagnostics.iter().any(|d| {
                d.level == "warn" && d.message == "3 of 3 runs lack enrichment"
            }),
            "diagnostics: {:?}",
            aggregate.diagnostics
        );
    }

    #[test]
    fn aggregate_no_warning_when_all_runs_in_scope_enriched() {
        let t0 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 0, 0).unwrap();
        let run1 = {
            let mut s = empty_snapshot(Some("FC One"));
            s.sites = vec![site(t0, 10, None, false)];
            s
        };
        let runs = vec![("run-1".to_string(), run1)];

        let aggregate = aggregate_enrichments(&runs, 1);

        assert!(
            !aggregate.diagnostics.iter().any(|d| d.message.contains("lack enrichment")),
            "diagnostics: {:?}",
            aggregate.diagnostics
        );
    }

    #[tokio::test]
    async fn analyze_saves_run_and_spawn_aggregate() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);

        let manifest = "\
New Null-Sec Incursion: 4MY-AB - Immensea
Constellation
4MY-AB
Region
Immensea
";
        desk.paste(Tray::Manifest, manifest).await.unwrap();
        desk.amend(AmendOp::SetSessionSettings {
            settings: RunSettings {
                space: SpaceBand::LowNull,
                fleet_size: 15,
                expected_isk: 15_000_000,
                lp_per_char: 2_000,
                isk_per_lp: 1400.0,
                break_threshold_minutes: 25,
                run_start: None,
            },
        })
        .await
        .unwrap();

        let wallet = "\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tx\n\
2026.07.29 23:14\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\ty\n";
        desk.paste(Tray::Wallet, wallet).await.unwrap();
        let focus = desk.analyze().await.unwrap();
        assert!(focus.report.is_some());
        assert_eq!(focus.report.as_ref().unwrap().session.sites_ran, 2);
        assert_eq!(focus.catalog.runs.len(), 1);
        assert_eq!(focus.catalog.spawns[0].constellation, "4MY-AB");

        let overall = desk.focus(ReportScope::Overall).await.unwrap();
        assert_eq!(overall.report.as_ref().unwrap().session.sites_ran, 2);
    }

    #[tokio::test]
    async fn analyze_persists_and_attaches_gamelog_enrichment() {
        use std::fs;

        let dir = tempdir().unwrap();
        let gamelogs_dir = dir.path().join("Gamelogs");
        fs::create_dir_all(&gamelogs_dir).unwrap();
        fs::write(
            gamelogs_dir.join("20260729_225000.txt"),
            "---------------------------------------------------------------\n\
  Gamelog\n\
  Listener:        FC Pilot\n\
  Session Started: 2026.07.29 22:50:00\n\
---------------------------------------------------------------\n\
\n\
[ 2026.07.29 23:09:30 ] (notify) Following Fleet Commander in warp\n\
[ 2026.07.29 23:11:00 ] (combat) <color=0xff00ffff><b>312</b> <font size=10>to</font> \
<b>Sansha's Nation Frenzy</b> - Scourge Rage Heavy Missile - Hits\n",
        )
        .unwrap();
        // Headerless file: the scan must skip it and say so.
        fs::write(gamelogs_dir.join("truncated.txt"), "no header here\n").unwrap();

        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let mut app_settings = db.get_settings().await.unwrap();
        app_settings.gamelogs_dir = Some(gamelogs_dir.to_string_lossy().to_string());
        app_settings.fc_character = Some("FC Pilot".into());
        db.set_settings(&app_settings).await.unwrap();

        let desk = RunDesk::new(db);

        let manifest = "\
New Null-Sec Incursion: 4MY-AB - Immensea
Constellation
4MY-AB
Region
Immensea
";
        desk.paste(Tray::Manifest, manifest).await.unwrap();
        desk.amend(AmendOp::SetSessionSettings {
            settings: RunSettings {
                space: SpaceBand::LowNull,
                fleet_size: 15,
                expected_isk: 15_000_000,
                lp_per_char: 2_000,
                isk_per_lp: 1400.0,
                break_threshold_minutes: 25,
                run_start: Some(Utc.with_ymd_and_hms(2026, 7, 29, 22, 54, 0).unwrap()),
            },
        })
        .await
        .unwrap();

        let wallet = "\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tCONCORD rewarded FC Pilot for services performed.\n\
2026.07.29 23:14\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tCONCORD rewarded FC Pilot for services performed.\n";
        desk.paste(Tray::Wallet, wallet).await.unwrap();

        let focus = desk.analyze().await.unwrap();
        let enrichment = focus
            .enrichment
            .expect("enrichment should be attached to focus right after analyze");
        assert_eq!(enrichment.resolved_fc.as_deref(), Some("FC Pilot"));
        assert_eq!(enrichment.sites.len(), 2);
        assert_eq!(enrichment.sites[1].source, EnrichmentSource::Fleet);
        assert_eq!(enrichment.sites[1].warp_seconds, 90);
        assert_eq!(enrichment.sites[1].combat_to_payout_seconds, Some(180));

        // Files the scan had to skip are reported on the snapshot.
        assert!(
            enrichment
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Skipped gamelog without Session Started")),
            "diagnostics: {:?}",
            enrichment.diagnostics
        );

        // Re-enrich via amend without a run_id → re-enriches every run in the
        // current scope (analyze focused Spawn, which holds this one run).
        let refocused = desk
            .amend(AmendOp::ReenrichRun { run_id: None })
            .await
            .unwrap();
        assert!(refocused.enrichment.is_some());
        assert!(refocused.sealed_run_id.is_some());
    }

    /// `ReenrichRun { Some(id) }` re-enriches that run even when there is no
    /// sealed run — proves the explicit-id path no longer depends on
    /// `sealed_run_id` at all.
    #[tokio::test]
    async fn reenrich_run_with_explicit_id_works_when_sealed_is_none() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let focus = seed_one_run(&desk).await;
        let run_id = focus.catalog.runs[0].run_id.clone();

        {
            let mut st = desk.inner.lock();
            st.sealed_run_id = None;
        }
        desk.focus(ReportScope::Run {
            run_id: run_id.clone(),
        })
        .await
        .unwrap();
        let after = desk
            .amend(AmendOp::ReenrichRun {
                run_id: Some(run_id.clone()),
            })
            .await
            .unwrap();
        assert!(after.enrichment.is_some());
        assert!(after.sealed_run_id.is_none());
    }

    /// `ReenrichRun { None }` on a Spawn scope re-enriches every run in that
    /// spawn — not just the last-sealed one — and reports how many
    /// succeeded via the top-strip diagnostic.
    #[tokio::test]
    async fn reenrich_none_on_spawn_refreshes_all_runs_in_spawn() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);

        let focus1 = seed_one_run(&desk).await;
        let run1 = focus1.catalog.runs[0].run_id.clone();
        let constellation = focus1.catalog.runs[0].constellation.clone();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
        let focus2 = desk.analyze().await.unwrap();
        let run2 = focus2
            .catalog
            .runs
            .iter()
            .map(|r| r.run_id.clone())
            .find(|id| *id != run1)
            .expect("second analyze should seal a distinct run");

        {
            let mut st = desk.inner.lock();
            st.sealed_run_id = None;
        }
        let after = desk
            .focus(ReportScope::Spawn {
                constellation: constellation.clone(),
            })
            .await
            .unwrap();
        assert!(after.sealed_run_id.is_none());

        let after = desk
            .amend(AmendOp::ReenrichRun { run_id: None })
            .await
            .unwrap();

        assert!(matches!(desk.db.load_enrichment_status(&run1).await.unwrap(), EnrichmentLoad::Ok(_)));
        assert!(matches!(desk.db.load_enrichment_status(&run2).await.unwrap(), EnrichmentLoad::Ok(_)));
        assert!(
            after
                .diagnostics
                .iter()
                .any(|d| d.message == "2 of 2 re-enriched"),
            "diagnostics: {:?}",
            after.diagnostics
        );
    }

    /// Spawn scope with runs but zero readable enrichments still returns an
    /// empty aggregate carrying the partial-coverage warning for the strip.
    #[tokio::test]
    async fn spawn_scope_warns_when_all_runs_lack_enrichment() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let settings = RunSettings {
            space: SpaceBand::LowNull,
            fleet_size: 15,
            expected_isk: 15_000_000,
            lp_per_char: 2_000,
            isk_per_lp: 1400.0,
            break_threshold_minutes: 25,
            run_start: None,
        };
        let report = build_report(&[], &settings);
        desk.db
            .upsert_spawn("4MY-AB", None)
            .await
            .unwrap();
        desk.db
            .save_run("run-a", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();
        desk.db
            .save_run("run-b", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();

        let focus = desk
            .focus(ReportScope::Spawn {
                constellation: "4MY-AB".into(),
            })
            .await
            .unwrap();
        let enrichment = focus.enrichment.expect(
            "spawn with runs but no enrichment should still return an empty aggregate",
        );

        assert!(enrichment.sites.is_empty());
        assert!(enrichment.missiles.is_empty());
        assert!(
            enrichment.diagnostics.iter().any(|d| {
                d.level == "warn" && d.message == "2 of 2 runs lack enrichment"
            }),
            "diagnostics: {:?}",
            enrichment.diagnostics
        );
    }

    /// Spawn scope aggregates every enriched run in the spawn: missiles
    /// merge by listener, sites concatenate, and totals recompute — not just
    /// the most recently sealed run's snapshot.
    #[tokio::test]
    async fn spawn_scope_aggregates_enrichment_across_runs() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);

        let focus1 = seed_one_run(&desk).await;
        let run1 = focus1.catalog.runs[0].run_id.clone();
        let constellation = focus1.catalog.runs[0].constellation.clone();

        // A second analyze under the same constellation seals a second run
        // in the same spawn.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let focus2 = seed_one_run(&desk).await;
        let run2 = focus2
            .catalog
            .runs
            .iter()
            .map(|r| r.run_id.clone())
            .find(|id| *id != run1)
            .expect("second analyze should seal a distinct run");
        assert_eq!(focus2.catalog.runs.len(), 2);

        let t0 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 7, 29, 23, 30, 0).unwrap();
        let mut snap1 = empty_snapshot(Some("FC One"));
        snap1.listeners = vec!["FC One".into(), "Alt".into()];
        snap1.sites = vec![site(t0, 60, Some(90), false)];
        snap1.missiles = vec![missile("Alt", 1, 10, 156, 5)];
        snap1.totals = EnrichmentTotals {
            warp_seconds: 60,
            combat_to_payout_seconds: Some(90),
            avg_combat_to_payout_seconds: Some(90.0),
            fleet_dead: 5,
        };

        let mut snap2 = empty_snapshot(Some("FC Two"));
        snap2.listeners = vec!["Alt".into()];
        snap2.sites = vec![site(t1, 40, Some(70), false)];
        snap2.missiles = vec![missile("Alt", 2, 20, 200, 7)];
        snap2.totals = EnrichmentTotals {
            warp_seconds: 40,
            combat_to_payout_seconds: Some(70),
            avg_combat_to_payout_seconds: Some(70.0),
            fleet_dead: 7,
        };

        // Overwrite with deterministic snapshots to isolate the aggregate
        // merge behavior from gamelog-scan fixture details (already covered
        // by `enrichment` module tests and
        // `analyze_persists_and_attaches_gamelog_enrichment`).
        desk.db.save_enrichment(&run1, &snap1).await.unwrap();
        desk.db.save_enrichment(&run2, &snap2).await.unwrap();

        let focus = desk
            .focus(ReportScope::Spawn { constellation })
            .await
            .unwrap();
        let aggregate = focus
            .enrichment
            .expect("spawn scope should aggregate enrichment across both runs");

        assert_eq!(aggregate.listeners, vec!["FC One".to_string(), "Alt".to_string()]);
        assert_eq!(aggregate.missiles.len(), 1);
        let alt = &aggregate.missiles[0];
        assert_eq!(alt.reload_cycles, 3);
        assert_eq!(alt.hits, 30);
        assert_eq!(alt.dead, 12);
        assert_eq!(alt.missiles_per_cycle, 200);
        assert_eq!(aggregate.sites.len(), 2);
        assert_eq!(aggregate.totals.warp_seconds, 100);
        assert_eq!(aggregate.totals.combat_to_payout_seconds, Some(160));
        assert_eq!(aggregate.totals.fleet_dead, 12);
        assert!(
            !aggregate.diagnostics.iter().any(|d| d.message.contains("lack enrichment")),
            "both runs enriched — no partial-coverage warning expected: {:?}",
            aggregate.diagnostics
        );
    }

    /// Missing gamelogs dir is a warning, not a failure: the v1 report stands
    /// and an empty snapshot explains why enrichment is empty.
    #[tokio::test]
    async fn missing_gamelogs_dir_warns_and_keeps_report() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let mut app_settings = db.get_settings().await.unwrap();
        app_settings.gamelogs_dir =
            Some(dir.path().join("nope").join("Gamelogs").to_string_lossy().to_string());
        db.set_settings(&app_settings).await.unwrap();

        let desk = RunDesk::new(db);
        let manifest = "\
New Null-Sec Incursion: 4MY-AB - Immensea
Constellation
4MY-AB
Region
Immensea
";
        desk.paste(Tray::Manifest, manifest).await.unwrap();
        let wallet = "\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tx\n\
2026.07.29 23:14\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\ty\n";
        desk.paste(Tray::Wallet, wallet).await.unwrap();

        let focus = desk.analyze().await.unwrap();

        assert_eq!(focus.report.as_ref().unwrap().session.sites_ran, 2);
        let enrichment = focus
            .enrichment
            .expect("an empty snapshot carrying the warning is still saved");
        assert!(enrichment.listeners.is_empty());
        assert!(
            enrichment
                .diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message.contains("Gamelogs directory not found")),
            "diagnostics: {:?}",
            enrichment.diagnostics
        );
        // Re-enrich stays available: there is a sealed run to recompute.
        assert!(focus.sealed_run_id.is_some());
    }

    fn sample_manifest() -> &'static str {
        "\
New Null-Sec Incursion: 4MY-AB - Immensea
Constellation
4MY-AB
Region
Immensea
"
    }

    fn sample_wallet() -> &'static str {
        "\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tx\n\
2026.07.29 23:14\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\ty\n"
    }

    async fn seed_one_run(desk: &RunDesk) -> EditionFocus {
        desk.paste(Tray::Manifest, sample_manifest()).await.unwrap();
        desk.amend(AmendOp::SetSessionSettings {
            settings: RunSettings {
                space: SpaceBand::LowNull,
                fleet_size: 15,
                expected_isk: 15_000_000,
                lp_per_char: 2_000,
                isk_per_lp: 1400.0,
                break_threshold_minutes: 25,
                run_start: None,
            },
        })
        .await
        .unwrap();
        desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
        desk.analyze().await.unwrap()
    }

    #[tokio::test]
    async fn delete_run_updates_catalog_scope_and_clears_sealed() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let focus = seed_one_run(&desk).await;
        let run_id = focus.catalog.runs[0].run_id.clone();
        assert_eq!(focus.sealed_run_id.as_deref(), Some(run_id.as_str()));

        desk.focus(ReportScope::Run {
            run_id: run_id.clone(),
        })
        .await
        .unwrap();

        let after = desk.delete_run(&run_id).await.unwrap();
        assert!(after.catalog.runs.is_empty());
        assert!(after.catalog.spawns.is_empty());
        assert_eq!(after.scope, ReportScope::Overall);
        assert!(after.sealed_run_id.is_none());
        assert_eq!(
            after
                .staging_spawn
                .as_ref()
                .and_then(|s| s.constellation.as_deref()),
            Some("4MY-AB")
        );
    }

    #[tokio::test]
    async fn delete_run_with_sibling_focuses_parent_spawn() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        // Two runs same spawn: analyze twice with wallet re-paste
        let desk = RunDesk::new(db);
        let first = seed_one_run(&desk).await;
        let run_a = first.catalog.runs[0].run_id.clone();
        // Re-stage wallet for second analyze (manifest still staged)
        desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
        let second = desk.analyze().await.unwrap();
        let run_b = second
            .catalog
            .runs
            .iter()
            .find(|r| r.run_id != run_a)
            .unwrap()
            .run_id
            .clone();

        let after = desk.delete_run(&run_a).await.unwrap();
        assert_eq!(after.catalog.runs.len(), 1);
        assert_eq!(after.catalog.runs[0].run_id, run_b);
        assert_eq!(
            after.scope,
            ReportScope::Spawn {
                constellation: "4MY-AB".into()
            }
        );
        // sealed was run_b (last analyze); still present
        assert_eq!(after.sealed_run_id.as_deref(), Some(run_b.as_str()));
    }

    #[tokio::test]
    async fn delete_spawn_clears_sealed_when_sealed_in_spawn() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let focus = seed_one_run(&desk).await;
        assert!(focus.sealed_run_id.is_some());

        let after = desk.delete_spawn("4MY-AB").await.unwrap();
        assert!(after.catalog.runs.is_empty());
        assert!(after.catalog.spawns.is_empty());
        assert_eq!(after.scope, ReportScope::Overall);
        assert!(after.sealed_run_id.is_none());
    }

    #[tokio::test]
    async fn clear_all_empties_catalog_and_clears_sealed_keeps_trays() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let _ = seed_one_run(&desk).await;
        // Put something in wallet tray after analyze
        desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
        let before = desk.open().await.unwrap();
        assert!(before.trays.pending_sites >= 1);
        assert!(before.sealed_run_id.is_some());

        let after = desk.clear_all().await.unwrap();
        assert!(after.catalog.runs.is_empty());
        assert!(after.catalog.spawns.is_empty());
        assert_eq!(after.scope, ReportScope::Overall);
        assert!(after.sealed_run_id.is_none());
        assert!(after.trays.pending_sites >= 1);
    }

    #[tokio::test]
    async fn delete_run_unknown_leaves_state() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).await.unwrap();
        let desk = RunDesk::new(db);
        let _ = seed_one_run(&desk).await;
        let before = desk.open().await.unwrap();
        let err = desk.delete_run("nope").await.unwrap_err();
        assert!(!err.is_empty());
        let after = desk.open().await.unwrap();
        assert_eq!(after.catalog.runs.len(), before.catalog.runs.len());
        assert_eq!(after.scope, before.scope);
    }
}
