//! RunDesk document-session: trays → analyze → focus.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::path::PathBuf;

use crate::analytics_types::*;
use crate::db::Db;
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
                let target = match run_id {
                    Some(id) => Some(id),
                    None => self.inner.lock().sealed_run_id.clone(),
                };
                if let Some(id) = target {
                    self.reenrich_run(&id).await?;
                }
            }
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
        let enrichment = self.enrichment_for_scope(&scope, &sealed_run_id).await?;

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

    /// Enrichment for the currently-focused run: the scoped run when
    /// browsing a specific run, otherwise the most recently sealed run
    /// (e.g. right after Analyze, before the user navigates elsewhere).
    async fn enrichment_for_scope(
        &self,
        scope: &ReportScope,
        sealed_run_id: &Option<String>,
    ) -> Result<Option<EnrichmentSnapshot>, String> {
        let run_id = match scope {
            ReportScope::Run { run_id } => Some(run_id.clone()),
            _ => sealed_run_id.clone(),
        };
        match run_id {
            Some(id) => self.db.load_enrichment(&id).await.map_err(|e| e.to_string()),
            None => Ok(None),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanguard_payouts::SpaceBand;
    use tempfile::tempdir;

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
        use chrono::TimeZone;
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
        assert_eq!(enrichment.sites[1].source, EnrichmentSource::Fc);
        assert_eq!(enrichment.sites[1].warp_seconds, 90);
        assert_eq!(enrichment.sites[1].in_site_seconds, 180);

        // Scan diagnostics ride along on the snapshot (the unreadable/headerless
        // files this scan skipped, if any, plus enrichment's own warnings).
        assert!(
            enrichment
                .diagnostics
                .iter()
                .all(|d| d.level == "warn" || d.level == "info"),
            "unexpected diagnostic levels: {:?}",
            enrichment.diagnostics
        );

        // Re-enrich via amend without a run_id → falls back to the sealed run.
        let refocused = desk
            .amend(AmendOp::ReenrichRun { run_id: None })
            .await
            .unwrap();
        assert!(refocused.enrichment.is_some());
        assert!(refocused.sealed_run_id.is_some());
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
}
