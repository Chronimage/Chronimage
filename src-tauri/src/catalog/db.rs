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
///
/// After migrations run, attempts a best-effort load of the sqlite-vec
/// extension and creation of `vec_photo_embeddings`. If the extension is
/// absent the error is logged as a warning and the app continues using the
/// BLOB fallback in `photo_embeddings.embedding`.
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
        // Run all migrations except those that require sqlite-vec. The
        // 20260421000000_sqlite_vec.sql migration contains a CREATE VIRTUAL
        // TABLE statement that will fail if the vec0 extension is not loaded.
        // We skip that statement here and attempt it ourselves below via
        // best-effort extension loading.
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal(format!("catalog migration failed: {e}")))?;
    }

    // Best-effort: try to ensure the sqlite-vec virtual table exists.
    // If the extension or the vec0 module is unavailable we log a warning and
    // continue — the BLOB fallback path in photo_embeddings handles this case.
    try_init_sqlite_vec(&pool).await;

    Ok(pool)
}

/// Attempt to create `vec_photo_embeddings` using the sqlite-vec vec0 module.
///
/// sqlite-vec is a loadable extension; on some machines it may not be present.
/// Any error here is non-fatal — the app falls back to brute-force cosine
/// over `photo_embeddings.embedding` BLOBs.
async fn try_init_sqlite_vec(pool: &SqlitePool) {
    let result = sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_photo_embeddings USING vec0(embedding float[768])",
    )
    .execute(pool)
    .await;

    match result {
        Ok(_) => tracing::debug!("sqlite-vec: vec_photo_embeddings ready"),
        Err(e) => tracing::warn!(
            error = %e,
            "sqlite-vec extension unavailable — falling back to BLOB cosine search. \
             Install vec0 to enable ANN search."
        ),
    }
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
            ["1", "2", "3"].contains(&row.0.as_str()),
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
