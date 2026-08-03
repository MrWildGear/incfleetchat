use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::path::Path;

use crate::analytics_types::{Catalog, RunSummary, SpawnSummary};
use crate::spawn_parse::SpawnDraft;
use crate::timing::{AnalyticsReport, RunSettings};
use crate::types::AppSettings;

#[derive(Clone)]
pub struct Db {
    pool: Pool<Sqlite>,
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
        Ok(())
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
        Ok(row.and_then(|(json,)| serde_json::from_str(&json).ok()))
    }

    pub async fn load_reports_for_spawn(
        &self,
        constellation: &str,
    ) -> Result<Vec<AnalyticsReport>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT report_json FROM analytics_runs WHERE constellation = ?")
                .bind(constellation)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(json,)| serde_json::from_str(&json).ok())
            .collect())
    }

    pub async fn load_all_reports(&self) -> Result<Vec<AnalyticsReport>, sqlx::Error> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT report_json FROM analytics_runs").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(json,)| serde_json::from_str(&json).ok())
            .collect())
    }

    pub async fn get_settings(&self) -> Result<AppSettings, sqlx::Error> {
        let row: (Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT character, chatlogs_dir, always_on_top FROM settings WHERE id = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(AppSettings {
            character: row.0,
            chatlogs_dir: row.1,
            always_on_top: row.2 != 0,
        })
    }

    pub async fn set_settings(&self, settings: &AppSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE settings SET character = ?, chatlogs_dir = ?, always_on_top = ? WHERE id = 1",
        )
        .bind(&settings.character)
        .bind(&settings.chatlogs_dir)
        .bind(if settings.always_on_top { 1 } else { 0 })
        .execute(&self.pool)
        .await?;
        Ok(())
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
}
