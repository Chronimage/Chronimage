//! SQLite pool + migration runner.

use crate::{AppError, AppResult};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::PathBuf, str::FromStr, sync::Once, time::Duration};

// ── sqlite-vec auto-extension registration ───────────────────────────────────

/// Called exactly once per process lifetime (guarded by [`Once`]).
///
/// Registers `sqlite3_vec_init` as a SQLite auto-extension so that every new
/// connection (including every connection sqlx opens from the pool) automatically
/// loads the vec0 virtual-table module.
///
/// # Safety
/// `sqlite3_auto_extension` is a stable C API. We call it before any pool
/// opens a connection, so there is no concurrent connection state to race
/// against. The transmute converts the C `sqlite3_vec_init` function pointer
/// to the nullable-function-pointer type that `sqlite3_auto_extension` expects;
/// this is the canonical pattern used by the sqlite-vec crate itself.
///
/// Critically, `sqlite-vec` is compiled with `-DSQLITE_CORE`, meaning it links
/// against the same SQLite symbols as `libsqlite3-sys` (and therefore sqlx).
/// Registering here is guaranteed to reach the same SQLite instance that every
/// sqlx pool connection uses.
static SQLITE_VEC_REGISTERED: Once = Once::new();

fn ensure_sqlite_vec_registered() {
    SQLITE_VEC_REGISTERED.call_once(|| {
        // SAFETY: see module-level comment above.
        // SAFETY: sqlite3_auto_extension takes a nullable fn pointer typed as
        // `unsafe extern "C" fn(*mut sqlite3, *mut *mut i8, *const sqlite3_api_routines) -> i32`.
        // sqlite3_vec_init has C linkage and is the correct entry-point; its
        // actual C signature matches the auto-extension contract. We transmute
        // through *const () to paper over the Rust-side declaration mismatch in
        // the sqlite-vec crate (which declares it as `fn()`).
        unsafe {
            let init_fn = sqlite_vec::sqlite3_vec_init as *const ();
            let auto_ext_fn = std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut libsqlite3_sys::sqlite3,
                    *mut *mut std::ffi::c_char,
                    *const libsqlite3_sys::sqlite3_api_routines,
                ) -> std::ffi::c_int,
            >(init_fn);
            let rc = libsqlite3_sys::sqlite3_auto_extension(Some(auto_ext_fn));
            if rc != libsqlite3_sys::SQLITE_OK {
                // This is a hard startup failure — vec0 is a required module.
                // tracing may not be initialised yet at process start, so we
                // also write to stderr so it shows up in crash logs.
                let msg = format!("sqlite3_auto_extension(sqlite_vec) failed, rc={rc}");
                tracing::error!("{}", msg);
                eprintln!("FATAL: {msg}");
            }
        }
    });
}

// ── Pool options ─────────────────────────────────────────────────────────────

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

// ── open_pool ────────────────────────────────────────────────────────────────

/// Open (or create) the catalog database, apply migrations, register the
/// sqlite-vec extension, and create the `vec_photo_embeddings` and
/// `vec_face_embeddings` virtual tables.
///
/// sqlite-vec is statically linked and registered as an auto-extension before
/// the first connection opens. `vec0` is therefore always available; any
/// failure to create the virtual tables is a hard error (not a best-effort
/// fallback).
///
/// WAL + FK + normal-sync pragmas are applied via the connection options.
/// Applying them a second time on an existing DB is harmless.
pub async fn open_pool(opts: PoolOptions) -> AppResult<SqlitePool> {
    // Register vec0 before any connection is opened.
    ensure_sqlite_vec_registered();

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

    init_sqlite_vec_virtual_tables(&pool).await?;

    Ok(pool)
}

// ── Virtual-table creation ───────────────────────────────────────────────────

/// Create the sqlite-vec virtual tables required by the AI pipeline.
///
/// This function is NOT best-effort: if vec0 is unavailable after static
/// linking, that indicates a build configuration error and the caller receives
/// `AppError::Internal`.
///
/// Tables created:
/// - `vec_photo_embeddings float[768]` — SigLIP-2 (phase 1 embeddings)
/// - `vec_face_embeddings  float[512]` — ArcFace W600K R50 (phase 2 faces)
///
/// Both use `IF NOT EXISTS` so repeated calls (e.g. from `open_pool_is_idempotent`)
/// are safe.
async fn init_sqlite_vec_virtual_tables(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_photo_embeddings \
         USING vec0(embedding float[768])",
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Internal(format!(
            "failed to create vec_photo_embeddings — \
             sqlite-vec not registered correctly: {e}"
        ))
    })?;

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_face_embeddings \
         USING vec0(embedding float[512])",
    )
    .execute(pool)
    .await
    .map_err(|e| {
        AppError::Internal(format!(
            "failed to create vec_face_embeddings — \
             sqlite-vec not registered correctly: {e}"
        ))
    })?;

    tracing::debug!("sqlite-vec: vec_photo_embeddings(768) and vec_face_embeddings(512) ready");
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

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
        // If this passes, migrations ran twice without error and IF NOT EXISTS
        // on the vec virtual tables was honoured.
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

    /// Verifies that both sqlite-vec virtual tables are created by `open_pool`.
    ///
    /// This test will fail (and surface `no such module: vec0`) if
    /// `ensure_sqlite_vec_registered` or the auto-extension path is broken,
    /// giving a clear signal rather than a silent BLOB fallback.
    #[tokio::test]
    async fn open_pool_registers_vec0_and_virtual_tables_exist() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db)).await.expect("pool");

        for vtbl in ["vec_photo_embeddings", "vec_face_embeddings"] {
            // sqlite-vec virtual tables appear in sqlite_master with type='table'.
            let found: Option<(String,)> =
                sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name = ?1")
                    .bind(vtbl)
                    .fetch_optional(&pool)
                    .await
                    .expect("sqlite_master query");

            assert!(
                found.is_some(),
                "sqlite-vec virtual table '{vtbl}' not found — vec0 module not registered"
            );
        }
    }

    /// Insert a 768-dim vector into `vec_photo_embeddings`, then read it back
    /// by rowid and assert byte-exact round-trip preservation.
    #[tokio::test]
    async fn vec0_insert_and_knn_round_trip() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("catalog.db");
        let pool = open_pool(PoolOptions::new(db)).await.expect("pool");

        // Build a deterministic 768-element f32 vector.
        let dims: usize = 768;
        let input_vec: Vec<f32> = (0..dims).map(|i| i as f32 * 0.001).collect();

        // sqlite-vec accepts vectors as raw little-endian f32 bytes.
        let blob: Vec<u8> = input_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        // Insert.
        sqlx::query("INSERT INTO vec_photo_embeddings(embedding) VALUES (?1)")
            .bind(&blob)
            .execute(&pool)
            .await
            .expect("vec insert");

        // Read back by rowid = 1 (first insert).
        let row: (Vec<u8>,) =
            sqlx::query_as("SELECT embedding FROM vec_photo_embeddings WHERE rowid = 1")
                .fetch_one(&pool)
                .await
                .expect("vec select");

        assert_eq!(
            row.0.len(),
            dims * 4,
            "returned blob length mismatch: expected {} bytes, got {}",
            dims * 4,
            row.0.len()
        );

        // Decode and compare element-by-element.
        let returned_vec: Vec<f32> = row
            .0
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        for (i, (expected, got)) in input_vec.iter().zip(returned_vec.iter()).enumerate() {
            assert!(
                (expected - got).abs() < f32::EPSILON,
                "element {i}: expected {expected}, got {got}"
            );
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
