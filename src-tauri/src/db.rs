use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::path::Path;

use crate::analytics_types::{Catalog, EnrichmentSnapshot, RunSummary, SpawnSummary};
use crate::spawn_parse::SpawnDraft;
use crate::timing::{AnalyticsReport, RunSettings};
use crate::types::AppSettings;

#[derive(Clone)]
pub struct Db {
    pool: Pool<Sqlite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRunOutcome {
    pub constellation: String,
    pub spawn_removed: bool,
}

/// Stored reports for a scope, plus the runs whose `report_json` could not
/// be read. Unreadable rows are counted so the UI can say so instead of
/// silently aggregating fewer runs than the catalog shows.
#[derive(Debug, Clone, Default)]
pub struct ReportRows {
    pub reports: Vec<AnalyticsReport>,
    pub unreadable: Vec<String>,
}

impl ReportRows {
    fn from_rows(rows: Vec<(String, String)>) -> Self {
        let mut out = Self::default();
        for (run_id, json) in rows {
            match parse_report(&run_id, &json) {
                Some(report) => out.reports.push(report),
                None => out.unreadable.push(run_id),
            }
        }
        out
    }
}

/// Result of attempting to load a run's persisted `enrichment_json`.
#[derive(Debug, Clone, PartialEq)]
pub enum EnrichmentLoad {
    /// No `enrichment_json` stored for this run (never enriched).
    Missing,
    /// JSON present but does not deserialize into the current
    /// `EnrichmentSnapshot` shape (pre-combat→payout schema).
    Stale,
    Ok(EnrichmentSnapshot),
}

fn parse_report(run_id: &str, json: &str) -> Option<AnalyticsReport> {
    match serde_json::from_str(json) {
        Ok(report) => Some(report),
        Err(err) => {
            eprintln!("Unreadable report_json for run {run_id}: {err}");
            None
        }
    }
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self, sqlx::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                character TEXT,
                chatlogs_dir TEXT,
                always_on_top INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS ran_marks (
                site_id TEXT PRIMARY KEY,
                marked_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cleared_marks (
                site_id TEXT PRIMARY KEY,
                cleared_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS spawns (
                constellation TEXT PRIMARY KEY,
                region TEXT,
                security_status TEXT,
                sov_holder TEXT,
                staging_system TEXT,
                hq_system TEXT,
                assault_systems TEXT NOT NULL DEFAULT '[]',
                vanguard_systems TEXT NOT NULL DEFAULT '[]',
                announced_at TEXT,
                title TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS analytics_runs (
                run_id TEXT PRIMARY KEY,
                constellation TEXT NOT NULL,
                saved_at TEXT NOT NULL,
                settings_json TEXT NOT NULL,
                wallet_text TEXT NOT NULL,
                manifest_text TEXT NOT NULL,
                report_json TEXT NOT NULL,
                site_count INTEGER NOT NULL,
                liquid_isk INTEGER NOT NULL,
                FOREIGN KEY (constellation) REFERENCES spawns(constellation)
            );
            "#,
        )
        .execute(&self.pool)
        .await?;
        // Ensure single settings row
        sqlx::query(
            "INSERT OR IGNORE INTO settings (id, character, chatlogs_dir, always_on_top) VALUES (1, NULL, NULL, 0)",
        )
        .execute(&self.pool)
        .await?;

        // Recreate-safe additive migrations for older DBs created before these columns existed.
        self.ensure_column("settings", "gamelogs_dir", "TEXT").await?;
        self.ensure_column("settings", "fc_character", "TEXT").await?;
        self.ensure_column("settings", "ammo_launchers", "INTEGER NOT NULL DEFAULT 6")
            .await?;
        self.ensure_column("settings", "ammo_per_launcher", "INTEGER NOT NULL DEFAULT 26")
            .await?;
        self.ensure_column("analytics_runs", "enrichment_json", "TEXT").await?;

        Ok(())
    }

    /// Add `column` to `table` if it doesn't already exist. SQLite has no
    /// `ADD COLUMN IF NOT EXISTS`, so we attempt the ALTER and swallow the
    /// "duplicate column" error on subsequent opens of an already-migrated DB.
    async fn ensure_column(
        &self,
        table: &str,
        column: &str,
        decl: &str,
    ) -> Result<(), sqlx::Error> {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {decl}");
        match sqlx::query(&sql).execute(&self.pool).await {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(e)) if e.message().contains("duplicate column name") => {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn upsert_spawn(
        &self,
        constellation: &str,
        draft: Option<&SpawnDraft>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let region = draft.and_then(|d| d.region.clone());
        let security_status = draft.and_then(|d| d.security_status.clone());
        let sov_holder = draft.and_then(|d| d.sov_holder.clone());
        let staging_system = draft.and_then(|d| d.staging_system.clone());
        let hq_system = draft.and_then(|d| d.hq_system.clone());
        let assault = serde_json::to_string(
            &draft.map(|d| d.assault_systems.clone()).unwrap_or_default(),
        )
        .unwrap_or_else(|_| "[]".into());
        let vanguard = serde_json::to_string(
            &draft.map(|d| d.vanguard_systems.clone()).unwrap_or_default(),
        )
        .unwrap_or_else(|_| "[]".into());
        let announced_at = draft.and_then(|d| d.announced_at.map(|t| t.to_rfc3339()));
        let title = draft.and_then(|d| d.title.clone());

        sqlx::query(
            r#"
            INSERT INTO spawns (
                constellation, region, security_status, sov_holder, staging_system, hq_system,
                assault_systems, vanguard_systems, announced_at, title, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(constellation) DO UPDATE SET
                region = excluded.region,
                security_status = excluded.security_status,
                sov_holder = excluded.sov_holder,
                staging_system = excluded.staging_system,
                hq_system = excluded.hq_system,
                assault_systems = excluded.assault_systems,
                vanguard_systems = excluded.vanguard_systems,
                announced_at = excluded.announced_at,
                title = excluded.title,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(constellation)
        .bind(region)
        .bind(security_status)
        .bind(sov_holder)
        .bind(staging_system)
        .bind(hq_system)
        .bind(assault)
        .bind(vanguard)
        .bind(announced_at)
        .bind(title)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_run(
        &self,
        run_id: &str,
        constellation: &str,
        settings: &RunSettings,
        wallet_text: &str,
        manifest_text: &str,
        report: &AnalyticsReport,
    ) -> Result<(), sqlx::Error> {
        let settings_json = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
        let report_json = serde_json::to_string(report).unwrap_or_else(|_| "{}".into());
        sqlx::query(
            r#"
            INSERT INTO analytics_runs (
                run_id, constellation, saved_at, settings_json, wallet_text, manifest_text,
                report_json, site_count, liquid_isk
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run_id)
        .bind(constellation)
        .bind(Utc::now().to_rfc3339())
        .bind(settings_json)
        .bind(wallet_text)
        .bind(manifest_text)
        .bind(report_json)
        .bind(report.session.sites_ran as i64)
        .bind(report.session.fleet_liquid_isk)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_catalog(&self) -> Result<Catalog, sqlx::Error> {
        let spawn_rows: Vec<(String, Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT constellation, region, staging_system, hq_system FROM spawns ORDER BY constellation",
            )
            .fetch_all(&self.pool)
            .await?;

        let mut spawns = Vec::new();
        for (constellation, region, staging_system, hq_system) in spawn_rows {
            let (run_count,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM analytics_runs WHERE constellation = ?")
                    .bind(&constellation)
                    .fetch_one(&self.pool)
                    .await?;
            spawns.push(SpawnSummary {
                constellation,
                region,
                staging_system,
                hq_system,
                run_count: run_count as u32,
            });
        }

        let run_rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT run_id, constellation, saved_at, site_count, liquid_isk FROM analytics_runs ORDER BY saved_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut runs = Vec::new();
        for (run_id, constellation, saved_at, site_count, liquid_isk) in run_rows {
            let saved = DateTime::parse_from_rfc3339(&saved_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            runs.push(RunSummary {
                run_id,
                constellation,
                saved_at: saved,
                site_count: site_count as u32,
                liquid_isk,
            });
        }

        Ok(Catalog { spawns, runs })
    }

    pub async fn load_report(&self, run_id: &str) -> Result<Option<AnalyticsReport>, sqlx::Error> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT report_json FROM analytics_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(json,)| parse_report(run_id, &json)))
    }

    pub async fn load_reports_for_spawn(
        &self,
        constellation: &str,
    ) -> Result<ReportRows, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT run_id, report_json FROM analytics_runs WHERE constellation = ?",
        )
        .bind(constellation)
        .fetch_all(&self.pool)
        .await?;
        Ok(ReportRows::from_rows(rows))
    }

    pub async fn load_all_reports(&self) -> Result<ReportRows, sqlx::Error> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT run_id, report_json FROM analytics_runs")
                .fetch_all(&self.pool)
                .await?;
        Ok(ReportRows::from_rows(rows))
    }

    pub async fn get_settings(&self) -> Result<AppSettings, sqlx::Error> {
        let row: (Option<String>, Option<String>, i64, Option<String>, Option<String>, i64, i64) =
            sqlx::query_as(
                "SELECT character, chatlogs_dir, always_on_top, gamelogs_dir, fc_character, \
                 ammo_launchers, ammo_per_launcher FROM settings WHERE id = 1",
            )
            .fetch_one(&self.pool)
            .await?;
        Ok(AppSettings {
            character: row.0,
            chatlogs_dir: row.1,
            always_on_top: row.2 != 0,
            gamelogs_dir: row.3,
            fc_character: row.4,
            ammo_launchers: row.5,
            ammo_per_launcher: row.6,
        })
    }

    pub async fn set_settings(&self, settings: &AppSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE settings SET character = ?, chatlogs_dir = ?, always_on_top = ?, \
             gamelogs_dir = ?, fc_character = ?, ammo_launchers = ?, ammo_per_launcher = ? \
             WHERE id = 1",
        )
        .bind(&settings.character)
        .bind(&settings.chatlogs_dir)
        .bind(if settings.always_on_top { 1 } else { 0 })
        .bind(&settings.gamelogs_dir)
        .bind(&settings.fc_character)
        .bind(settings.ammo_launchers)
        .bind(settings.ammo_per_launcher)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persist an enrichment snapshot computed for a sealed run.
    pub async fn save_enrichment(
        &self,
        run_id: &str,
        snapshot: &EnrichmentSnapshot,
    ) -> Result<(), sqlx::Error> {
        let json = serde_json::to_string(snapshot).unwrap_or_else(|_| "null".into());
        sqlx::query("UPDATE analytics_runs SET enrichment_json = ? WHERE run_id = ?")
            .bind(json)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Load `run_id`'s enrichment, distinguishing "never enriched" from
    /// "enriched under an older schema and needs Re-enrich" so the caller
    /// can surface the right diagnostic for each.
    pub async fn load_enrichment_status(
        &self,
        run_id: &str,
    ) -> Result<EnrichmentLoad, sqlx::Error> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT enrichment_json FROM analytics_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(match row.and_then(|(json,)| json) {
            None => EnrichmentLoad::Missing,
            Some(json) => match serde_json::from_str::<EnrichmentSnapshot>(&json) {
                Ok(snapshot) => EnrichmentLoad::Ok(snapshot),
                Err(_) => EnrichmentLoad::Stale,
            },
        })
    }

    /// Run ids for a spawn, oldest first (latest last) — the order
    /// `enrichment::aggregate_enrichments` expects so "latest wins" merges behave.
    pub async fn list_run_ids_for_spawn(
        &self,
        constellation: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT run_id FROM analytics_runs WHERE constellation = ? ORDER BY saved_at ASC",
        )
        .bind(constellation)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// All run ids, oldest first (latest last). See `list_run_ids_for_spawn`.
    pub async fn list_all_run_ids(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT run_id FROM analytics_runs ORDER BY saved_at ASC")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Load the settings/report/wallet text needed to recompute enrichment
    /// for an already-sealed run (used by the `ReenrichRun` amend).
    pub async fn load_run_for_enrich(
        &self,
        run_id: &str,
    ) -> Result<Option<(RunSettings, AnalyticsReport, String)>, sqlx::Error> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT settings_json, report_json, wallet_text FROM analytics_runs WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(settings_json, report_json, wallet_text)| {
            let settings: RunSettings = serde_json::from_str(&settings_json).ok()?;
            let report: AnalyticsReport = serde_json::from_str(&report_json).ok()?;
            Some((settings, report, wallet_text))
        }))
    }

    pub async fn load_ran_ids(&self) -> Result<HashSet<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT site_id FROM ran_marks")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn mark_ran(&self, site_id: &str, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO ran_marks (site_id, marked_at) VALUES (?, ?)",
        )
        .bind(site_id)
        .bind(at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_cleared_ids(&self) -> Result<HashSet<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT site_id FROM cleared_marks")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn clear_site(&self, site_id: &str, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO cleared_marks (site_id, cleared_at) VALUES (?, ?)",
        )
        .bind(site_id)
        .bind(at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_sites(&self, site_ids: &[String], at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        for id in site_ids {
            self.clear_site(id, at).await?;
        }
        Ok(())
    }

    pub async fn delete_run(&self, run_id: &str) -> Result<DeleteRunOutcome, String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT constellation FROM analytics_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        let constellation = row
            .map(|(c,)| c)
            .ok_or_else(|| format!("Run {run_id} not found"))?;

        sqlx::query("DELETE FROM analytics_runs WHERE run_id = ?")
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let (remaining,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM analytics_runs WHERE constellation = ?")
                .bind(&constellation)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

        let spawn_removed = if remaining == 0 {
            sqlx::query("DELETE FROM spawns WHERE constellation = ?")
                .bind(&constellation)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            true
        } else {
            false
        };

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(DeleteRunOutcome {
            constellation,
            spawn_removed,
        })
    }

    pub async fn delete_spawn(&self, constellation: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let (exists,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM spawns WHERE constellation = ?")
                .bind(constellation)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!("Spawn {constellation} not found"));
        }

        sqlx::query("DELETE FROM analytics_runs WHERE constellation = ?")
            .bind(constellation)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM spawns WHERE constellation = ?")
            .bind(constellation)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn clear_all_analytics(&self) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM analytics_runs")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM spawns")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn settings_and_ran_round_trip() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let mut s = db.get_settings().await.unwrap();
        assert!(s.character.is_none());
        s.character = Some("Estemaire".into());
        s.chatlogs_dir = Some("C:/logs".into());
        s.always_on_top = true;
        db.set_settings(&s).await.unwrap();
        let loaded = db.get_settings().await.unwrap();
        assert_eq!(loaded.character.as_deref(), Some("Estemaire"));
        assert!(loaded.always_on_top);

        let now = Utc::now();
        db.mark_ran("abc123", now).await.unwrap();
        let ran = db.load_ran_ids().await.unwrap();
        assert!(ran.contains("abc123"));

        db.clear_site("abc123", now).await.unwrap();
        let cleared = db.load_cleared_ids().await.unwrap();
        assert!(cleared.contains("abc123"));
    }

    #[tokio::test]
    async fn settings_gamelog_and_ammo_fields_default_and_round_trip() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let mut s = db.get_settings().await.unwrap();
        assert_eq!(s.ammo_launchers, 6);
        assert_eq!(s.ammo_per_launcher, 26);
        assert!(s.gamelogs_dir.is_none());
        assert!(s.fc_character.is_none());

        s.gamelogs_dir = Some("C:/EVE/logs/Gamelogs".into());
        s.fc_character = Some("FC Pilot".into());
        s.ammo_launchers = 7;
        s.ammo_per_launcher = 20;
        db.set_settings(&s).await.unwrap();

        let loaded = db.get_settings().await.unwrap();
        assert_eq!(loaded.gamelogs_dir.as_deref(), Some("C:/EVE/logs/Gamelogs"));
        assert_eq!(loaded.fc_character.as_deref(), Some("FC Pilot"));
        assert_eq!(loaded.ammo_launchers, 7);
        assert_eq!(loaded.ammo_per_launcher, 20);
    }

    #[tokio::test]
    async fn enrichment_json_round_trips_on_saved_run() {
        use crate::analytics_types::{
            Diagnostic, EnrichmentSite, EnrichmentSnapshot, EnrichmentSource, EnrichmentTotals,
            MissileStat,
        };
        use crate::timing::{build_report, RunSettings};

        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.save_run("run-1", "4MY-AB", &settings, "wallet", "manifest", &report)
            .await
            .unwrap();

        assert_eq!(
            db.load_enrichment_status("run-1").await.unwrap(),
            EnrichmentLoad::Missing
        );

        let snapshot = EnrichmentSnapshot {
            resolved_fc: Some("FC Pilot".into()),
            listeners: vec!["FC Pilot".into()],
            diagnostics: vec![Diagnostic {
                level: "info".into(),
                message: "ok".into(),
            }],
            sites: vec![EnrichmentSite {
                occurred_at: Utc::now(),
                approach_seconds: Some(10),
                combat_to_payout_seconds: Some(20),
                is_break: false,
                source: EnrichmentSource::Fc,
                missiles: vec![],
            }],
            missiles: vec![MissileStat {
                listener: "FC Pilot".into(),
                reload_cycles: 1,
                hits: 5,
                missiles_per_cycle: 156,
                launchers: 6,
                dead: 151,
            }],
            totals: EnrichmentTotals {
                approach_seconds: Some(10),
                combat_to_payout_seconds: Some(20),
                avg_combat_to_payout_seconds: Some(20.0),
                fleet_dead: 151,
            },
        };
        db.save_enrichment("run-1", &snapshot).await.unwrap();

        let loaded = match db.load_enrichment_status("run-1").await.unwrap() {
            EnrichmentLoad::Ok(snapshot) => snapshot,
            other => panic!("expected EnrichmentLoad::Ok, got {other:?}"),
        };
        assert_eq!(loaded, snapshot);

        let (loaded_settings, loaded_report, wallet_text) =
            db.load_run_for_enrich("run-1").await.unwrap().unwrap();
        assert_eq!(loaded_settings.fleet_size, settings.fleet_size);
        assert_eq!(loaded_report.session.sites_ran, 0);
        assert_eq!(wallet_text, "wallet");
    }

    #[tokio::test]
    async fn load_enrichment_status_distinguishes_missing_from_stale() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.save_run("run-missing", "4MY-AB", &settings, "wallet", "manifest", &report)
            .await
            .unwrap();
        db.save_run("run-stale", "4MY-AB", &settings, "wallet", "manifest", &report)
            .await
            .unwrap();

        assert_eq!(
            db.load_enrichment_status("run-missing").await.unwrap(),
            EnrichmentLoad::Missing
        );

        // Pre-combat→payout schema: `in_site_seconds` instead of
        // `combat_to_payout_seconds` — deserialize must fail.
        sqlx::query("UPDATE analytics_runs SET enrichment_json = ? WHERE run_id = ?")
            .bind(
                r#"{"resolved_fc":null,"listeners":[],"diagnostics":[],"sites":[],"missiles":[],"totals":{"approach_seconds":0,"in_site_seconds":0,"fleet_dead":0}}"#,
            )
            .bind("run-stale")
            .execute(&db.pool)
            .await
            .unwrap();

        assert_eq!(
            db.load_enrichment_status("run-stale").await.unwrap(),
            EnrichmentLoad::Stale
        );

        let ids = db.list_run_ids_for_spawn("4MY-AB").await.unwrap();
        assert_eq!(ids, vec!["run-missing".to_string(), "run-stale".to_string()]);
        let all_ids = db.list_all_run_ids().await.unwrap();
        assert_eq!(all_ids, ids);
    }

    use crate::timing::{build_report, RunSettings};

    #[tokio::test]
    async fn delete_run_removes_row_and_keeps_spawn_when_siblings_remain() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.save_run("run-a", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();
        db.save_run("run-b", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();

        let out = db.delete_run("run-a").await.unwrap();
        assert_eq!(
            out,
            DeleteRunOutcome {
                constellation: "4MY-AB".into(),
                spawn_removed: false,
            }
        );
        let cat = db.load_catalog().await.unwrap();
        assert_eq!(cat.runs.len(), 1);
        assert_eq!(cat.runs[0].run_id, "run-b");
        assert_eq!(cat.spawns.len(), 1);
        assert_eq!(cat.spawns[0].run_count, 1);
    }

    #[tokio::test]
    async fn delete_run_last_run_removes_empty_spawn() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.save_run("run-only", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();

        let out = db.delete_run("run-only").await.unwrap();
        assert!(out.spawn_removed);
        let cat = db.load_catalog().await.unwrap();
        assert!(cat.runs.is_empty());
        assert!(cat.spawns.is_empty());
    }

    #[tokio::test]
    async fn delete_run_unknown_id_errors() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let err = db.delete_run("missing").await.unwrap_err();
        assert!(err.contains("missing") || err.contains("not found"));
    }

    #[tokio::test]
    async fn delete_spawn_removes_runs_and_spawn() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.upsert_spawn("OTHER", None).await.unwrap();
        db.save_run("r1", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();
        db.save_run("r2", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();
        db.save_run("r3", "OTHER", &settings, "", "", &report)
            .await
            .unwrap();

        db.delete_spawn("4MY-AB").await.unwrap();
        let cat = db.load_catalog().await.unwrap();
        assert_eq!(cat.spawns.len(), 1);
        assert_eq!(cat.spawns[0].constellation, "OTHER");
        assert_eq!(cat.runs.len(), 1);
        assert_eq!(cat.runs[0].run_id, "r3");
    }

    #[tokio::test]
    async fn delete_spawn_unknown_errors() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let err = db.delete_spawn("NOPE").await.unwrap_err();
        assert!(err.contains("NOPE") || err.contains("not found"));
    }

    #[tokio::test]
    async fn clear_all_analytics_empties_both_tables() {
        let dir = tempdir().unwrap();
        let db = Db::open(&dir.path().join("app.db")).await.unwrap();
        let settings = RunSettings::default();
        let report = build_report(&[], &settings);
        db.upsert_spawn("4MY-AB", None).await.unwrap();
        db.save_run("r1", "4MY-AB", &settings, "", "", &report)
            .await
            .unwrap();

        db.clear_all_analytics().await.unwrap();
        let cat = db.load_catalog().await.unwrap();
        assert!(cat.runs.is_empty());
        assert!(cat.spawns.is_empty());
    }
}
