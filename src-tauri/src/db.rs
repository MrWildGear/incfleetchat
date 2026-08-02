use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::path::Path;

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
