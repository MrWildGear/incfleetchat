//! RunDesk document-session: trays → analyze → focus.

use chrono::Utc;
use parking_lot::Mutex;

use crate::analytics_types::*;
use crate::db::Db;
use crate::spawn_parse::{parse_manifest, SpawnDraft};
use crate::timing::{build_report, merge_reports, AnalyticsReport, RunSettings};
use crate::wallet_parse::{parse_wallet_journal, WalletPayout};

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
        }

        self.snapshot().await
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
        }
        self.snapshot().await
    }

    async fn snapshot(&self) -> Result<EditionFocus, String> {
        let catalog = self.db.load_catalog().await.map_err(|e| e.to_string())?;
        let (trays, spawn, scope, diagnostics, settings, staging_spawn) = {
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
            )
        };

        let report = self.report_for_scope(&scope, &settings).await?;

        Ok(EditionFocus {
            trays,
            spawn,
            catalog,
            scope,
            report,
            diagnostics,
            session_settings: settings,
            staging_spawn,
        })
    }

    async fn report_for_scope(
        &self,
        scope: &ReportScope,
        settings: &RunSettings,
    ) -> Result<Option<AnalyticsReport>, String> {
        match scope {
            ReportScope::Overall => {
                let reports = self.db.load_all_reports().await.map_err(|e| e.to_string())?;
                if reports.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(merge_reports(&reports, settings.isk_per_lp)))
                }
            }
            ReportScope::Spawn { constellation } => {
                let reports = self
                    .db
                    .load_reports_for_spawn(constellation)
                    .await
                    .map_err(|e| e.to_string())?;
                if reports.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(merge_reports(&reports, settings.isk_per_lp)))
                }
            }
            ReportScope::Run { run_id } => self
                .db
                .load_report(run_id)
                .await
                .map_err(|e| e.to_string()),
        }
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
}
