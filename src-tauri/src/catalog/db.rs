//! SQLite pool + migration runner.

use crate::{AppError, AppResult};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::PathBuf, str::FromStr, time::Duration};

/// Knobs for [`open_pool`]. Sensible defaults match the Phase 1 NFRs
/// (200k-photo catalog, < 500ms 95p search).
#[derive(Debug, Clone)]
pub struct PoolOptions {
    pub db_path: PathBuf,
    pub max_connections: u32,
    pub create_if_missing: bool,
    pub run_migrations: bool,
}

impl PoolOptions {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            max_connections: 4,
            create_if_missing: true,
            run_migrations: true,
        }
    }
}

/// Open (or create) the catalog database, apply migrations if requested, and
/// return a [`SqlitePool`].
///
/// Configures WAL, foreign keys, normal-sync — match what the initial
/// migration also pragmas. Doing it twice is harmless.
pub async fn open_pool(opts: PoolOptions) -> AppResult<SqlitePool> {
    if let Some(parent) = opts.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let db_url = format!("sqlite://{}", opts.db_path.display());
    let conn_opts = SqliteConnectOptions::from_str(&db_url)
        .map_err(|e| AppError::Internal(format!("bad sqlite url: {e}")))?
        .create_if_missing(opts.create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(opts.max_connections)
        .acquire_timeout(Duration::from_secs(15))
        .connect_with(conn_opts)
        .await?;

    if opts.run_migrations {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal(format!("catalog migration failed: {e}")))?;
    }

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn open_pool_creates_db_and_applies_migrations() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db.clone())).await.expect("pool");
        assert!(db.exists(), "db file should exist");

        // Schema version must be present and set by the latest migration.
        let row: (String,) =
            sqlx::query_as("SELECT value FROM settings WHERE key = 'schema_version'")
                .fetch_one(&pool)
                .await
                .expect("schema_version row");
        assert!(
            ["1", "2"].contains(&row.0.as_str()),
            "unexpected schema version: {}",
            row.0
        );

        // Core tables exist.
        for tbl in ["photos", "sources", "source_copies", "imports", "settings"] {
            let found: Option<(String,)> =
                sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name = ?1")
                    .bind(tbl)
                    .fetch_optional(&pool)
                    .await
                    .expect("sqlite_master query");
            assert!(found.is_some(), "missing table: {tbl}");
        }
    }

    #[tokio::test]
    async fn open_pool_is_idempotent() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let _p1 = open_pool(PoolOptions::new(db.clone()))
            .await
            .expect("first");
        let _p2 = open_pool(PoolOptions::new(db.clone()))
            .await
            .expect("second");
        // If this passes, migrations ran twice without error.
    }

    #[tokio::test]
    async fn phase1_tables_exist_after_migration() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db)).await.expect("pool");

        for tbl in [
            "tags",
            "models",
            "photo_embeddings",
            "faces",
            "clusters",
            "smart_albums",
            "photo_views",
            "source_deletions",
        ] {
            let found: Option<(String,)> =
                sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name = ?1")
                    .bind(tbl)
                    .fetch_optional(&pool)
                    .await
                    .expect("sqlite_master query");
            assert!(found.is_some(), "missing phase 1 table: {tbl}");
        }
    }
}
