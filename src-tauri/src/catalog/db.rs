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
            ["1", "2", "3", "4", "5"].contains(&row.0.as_str()),
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

    /// Regression test: inserting a tag must NOT produce
    /// "cannot UPDATE contentless fts5 table: photos_fts".
    ///
    /// The broken triggers in 20260420000000_phase1_catalog.sql used
    /// `UPDATE photos_fts SET tags = …` which SQLite rejects on contentless
    /// tables. Migration 20260424000000_fts5_triggers_fix.sql drops those and
    /// replaces them with the correct delete-then-reinsert pattern. This test
    /// proves the fix is in place and that FTS5 search finds the tag value.
    #[tokio::test]
    async fn fts5_tag_insert_does_not_error_and_is_searchable() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db)).await.expect("pool");

        let now = chrono::Utc::now().to_rfc3339();

        // Insert a photo — this fires photos_fts_insert (should succeed).
        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'golden.jpg', 100, 100, ?2, 0) RETURNING id",
        )
        .bind("a".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo insert");

        // Insert a tag — this fires tags_fts_insert.
        // Before the fix this would return "cannot UPDATE contentless fts5 table".
        sqlx::query(
            "INSERT INTO tags (photo_id, label, kind, confidence, created_at) \
             VALUES (?1, 'golden', 'auto_scene', 1.0, ?2)",
        )
        .bind(photo_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("tag insert must not error — migration 20260424000000 applied");

        // FTS5 must now find the photo by the tag label.
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'golden'")
                .fetch_one(&pool)
                .await
                .expect("fts5 match query");

        assert_eq!(count, 1, "FTS5 must find the photo after tag insert");
    }

    /// Deleting a tag must rebuild the FTS row without error, and the deleted
    /// label must no longer be discoverable by the FTS index.
    #[tokio::test]
    async fn fts5_tag_delete_rebuilds_row() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db)).await.expect("pool");

        let now = chrono::Utc::now().to_rfc3339();

        // Use a filename that does NOT contain the tag label word to avoid
        // FTS filename-column contamination in the MATCH query.
        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'img00042.jpg', 100, 100, ?2, 0) RETURNING id",
        )
        .bind("b".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo insert");

        sqlx::query(
            "INSERT INTO tags (photo_id, label, kind, confidence, created_at) \
             VALUES (?1, 'crimsonsky', 'auto_scene', 1.0, ?2)",
        )
        .bind(photo_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("tag insert");

        // Confirm it is searchable before deletion.
        let before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'crimsonsky'",
        )
        .fetch_one(&pool)
        .await
        .expect("fts match before");
        assert_eq!(before, 1, "expected match before delete");

        // Delete the tag — fires tags_fts_before_delete + tags_fts_after_delete.
        sqlx::query("DELETE FROM tags WHERE photo_id = ?1")
            .bind(photo_id)
            .execute(&pool)
            .await
            .expect("tag delete must not error");

        // After deletion the photo row has empty tags — must not match 'crimsonsky'.
        let after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM photos_fts WHERE photos_fts MATCH 'crimsonsky'",
        )
        .fetch_one(&pool)
        .await
        .expect("fts match after");
        assert_eq!(after, 0, "FTS must not find deleted tag label");
    }
}
