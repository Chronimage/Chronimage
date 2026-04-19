//! Real import pipeline — Phase 1.
//!
//! Stages:
//!   1. Scan root for supported image extensions.
//!   2. Hash + insert each file (concurrent, semaphore-bounded to 4).
//!   3. Detect RAW+JPG pairs and write `paired_photo_id` links.
//!
//! Progress is emitted as `"chronimage://import-progress"` Tauri events so the
//! frontend can show a live progress bar. The pipeline is launched as a
//! detached tokio task from `commands::start_import` and returns immediately.

use crate::{
    import::{is_raw_extension, scan_dir, ScanOptions},
    AppError, AppResult,
};
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
use tokio::sync::Semaphore;

// ── Public types ─────────────────────────────────────────────────────────────

/// Live progress event emitted on `"chronimage://import-progress"`.
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgress {
    pub source_id: i64,
    pub import_id: i64,
    pub total: usize,
    pub done: usize,
    pub current_file: String,
    pub eta_seconds: Option<u64>,
}

/// Summary returned by [`run_pipeline`].
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub import_id: i64,
    pub imported_count: usize,
    pub skipped_count: usize,
    pub error_count: usize,
}

// ── Pipeline ─────────────────────────────────────────────────────────────────

/// Maximum concurrent SHA256 tasks.
const MAX_CONCURRENT_HASHES: usize = 4;

/// Run the full import pipeline, creating a fresh `imports` row first.
///
/// # Caller responsibilities
/// - The `sources` row for `source_id` must already exist.
pub async fn run_pipeline<R: tauri::Runtime>(
    source_id: i64,
    root: PathBuf,
    pool: SqlitePool,
    app_handle: tauri::AppHandle<R>,
) -> AppResult<ImportResult> {
    let now = Utc::now().to_rfc3339();
    let import_id: i64 = sqlx::query_scalar(
        "INSERT INTO imports (source_id, started_at, total_files, imported_count, \
         skipped_count, error_count) VALUES (?1, ?2, 0, 0, 0, 0) RETURNING id",
    )
    .bind(source_id)
    .bind(&now)
    .fetch_one(&pool)
    .await?;

    let on_progress = make_emit_callback(app_handle);
    execute_pipeline(source_id, import_id, root, pool, on_progress).await
}

/// Continue a pipeline run against an already-created `imports` row.
///
/// Used by `commands::start_import`, which creates the row synchronously so
/// the caller can obtain `import_id` before the background task runs.
pub async fn run_pipeline_from_import_id<R: tauri::Runtime>(
    source_id: i64,
    import_id: i64,
    root: PathBuf,
    pool: SqlitePool,
    app_handle: tauri::AppHandle<R>,
) -> AppResult<ImportResult> {
    let on_progress = make_emit_callback(app_handle);
    execute_pipeline(source_id, import_id, root, pool, on_progress).await
}

fn make_emit_callback<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
) -> Arc<dyn Fn(ImportProgress) + Send + Sync> {
    use tauri::Emitter;
    Arc::new(move |progress: ImportProgress| {
        let _ = app_handle.emit("chronimage://import-progress", progress);
    })
}

async fn execute_pipeline(
    source_id: i64,
    import_id: i64,
    root: PathBuf,
    pool: SqlitePool,
    on_progress: Arc<dyn Fn(ImportProgress) + Send + Sync>,
) -> AppResult<ImportResult> {
    // ── Stage 1: scan ────────────────────────────────────────────────────────
    let root_clone = root.clone();
    let entries = tokio::task::spawn_blocking(move || scan_dir(&ScanOptions::new(root_clone)))
        .await
        .map_err(|e| AppError::Internal(format!("scan task join: {e}")))??;

    let total = entries.len();

    // Update total_files in the imports row immediately so the UI can show it.
    sqlx::query("UPDATE imports SET total_files = ?1 WHERE id = ?2")
        .bind(total as i64)
        .bind(import_id)
        .execute(&pool)
        .await?;

    // ── Stage 2: hash + insert ───────────────────────────────────────────────
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_HASHES));
    let imported = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();

    // Collect paths for later pair-linking (need sha256 → photo_id mapping).
    // We'll build this as we go.
    let mut handles = Vec::with_capacity(entries.len());

    for entry in entries {
        let sem = Arc::clone(&sem);
        let pool = pool.clone();
        let imported = Arc::clone(&imported);
        let skipped = Arc::clone(&skipped);
        let done = Arc::clone(&done);
        let on_progress = Arc::clone(&on_progress);
        let path = entry.path.clone();
        let size_bytes = entry.size_bytes;
        let ext = entry.ext_lowercase.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| AppError::Internal(format!("semaphore closed: {e}")))?;

            let path_clone = path.clone();
            let hash = tokio::task::spawn_blocking(move || crate::import::sha256_file(&path_clone))
                .await
                .map_err(|e| AppError::Internal(format!("hash task join: {e}")))??;

            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let path_str = path.to_string_lossy().to_string();
            let is_raw = if is_raw_extension(&ext) { 1i64 } else { 0i64 };
            let now_ts = Utc::now().to_rfc3339();

            // INSERT OR IGNORE — duplicate sha256 is silently skipped.
            let rows_affected = sqlx::query(
                "INSERT OR IGNORE INTO photos \
                 (sha256, filename, width, height, imported_at, is_raw, size_bytes) \
                 VALUES (?1, ?2, 0, 0, ?3, ?4, ?5)",
            )
            .bind(&hash)
            .bind(&filename)
            .bind(&now_ts)
            .bind(is_raw)
            .bind(size_bytes as i64)
            .execute(&pool)
            .await?
            .rows_affected();

            let photo_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE sha256 = ?1")
                .bind(&hash)
                .fetch_one(&pool)
                .await?;

            if rows_affected > 0 {
                // Newly inserted — add source_copy.
                sqlx::query(
                    "INSERT INTO source_copies \
                     (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
                     VALUES (?1, ?2, ?3, 1, ?4, ?5)",
                )
                .bind(photo_id)
                .bind(source_id)
                .bind(&path_str)
                .bind(&hash)
                .bind(&now_ts)
                .execute(&pool)
                .await?;
                imported.fetch_add(1, Ordering::Relaxed);

                // Stage 2.5: extract EXIF + pHash for this photo.
                let meta_path = path.clone();
                let (exif, phash) = match tokio::task::spawn_blocking(move || {
                    let exif = crate::import::exif::read(&meta_path);
                    let phash = crate::dedupe::phash::compute(&meta_path);
                    (exif, phash)
                })
                .await
                {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!(error = %e, path = %path_str, "metadata task join error");
                        (crate::import::exif::ExifData::default(), None)
                    }
                };

                if let Err(e) = sqlx::query(
                    "UPDATE photos SET \
                     width            = COALESCE(?1,  width), \
                     height           = COALESCE(?2,  height), \
                     captured_at      = COALESCE(?3,  captured_at), \
                     captured_at_local= COALESCE(?4,  captured_at_local), \
                     camera_make      = COALESCE(?5,  camera_make), \
                     camera_model     = COALESCE(?6,  camera_model), \
                     lens_model       = COALESCE(?7,  lens_model), \
                     aperture         = COALESCE(?8,  aperture), \
                     shutter          = COALESCE(?9,  shutter), \
                     iso              = COALESCE(?10, iso), \
                     focal_mm         = COALESCE(?11, focal_mm), \
                     gps_lat          = COALESCE(?12, gps_lat), \
                     gps_lng          = COALESCE(?13, gps_lng), \
                     phash            = COALESCE(?14, phash) \
                     WHERE id = ?15",
                )
                .bind(exif.width.map(|v| v as i64))
                .bind(exif.height.map(|v| v as i64))
                .bind(&exif.captured_at)
                .bind(&exif.captured_at_local)
                .bind(&exif.camera_make)
                .bind(&exif.camera_model)
                .bind(&exif.lens_model)
                .bind(exif.aperture)
                .bind(&exif.shutter)
                .bind(exif.iso.map(|v| v as i64))
                .bind(exif.focal_mm)
                .bind(exif.gps_lat)
                .bind(exif.gps_lng)
                .bind(&phash)
                .bind(photo_id)
                .execute(&pool)
                .await
                {
                    tracing::warn!(error = %e, photo_id, "metadata UPDATE failed");
                }
            } else {
                skipped.fetch_add(1, Ordering::Relaxed);
            }

            let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
            let elapsed = start_time.elapsed().as_secs_f64();
            let eta = if finished > 0 && finished < total {
                let rate = finished as f64 / elapsed;
                if rate > 0.0 {
                    Some(((total - finished) as f64 / rate) as u64)
                } else {
                    None
                }
            } else {
                None
            };

            on_progress(ImportProgress {
                source_id,
                import_id,
                total,
                done: finished,
                current_file: filename,
                eta_seconds: eta,
            });

            Ok::<(PathBuf, String, i64, bool), AppError>((path, hash, photo_id, rows_affected > 0))
        });

        handles.push(handle);
    }

    // Collect results; track newly-inserted photos for the AI stage.
    let mut path_hash_id: Vec<(PathBuf, String, i64)> = Vec::new();
    let mut new_photos: Vec<(PathBuf, i64)> = Vec::new();
    for h in handles {
        match h.await {
            Ok(Ok((path, hash, photo_id, was_inserted))) => {
                if was_inserted {
                    new_photos.push((path.clone(), photo_id));
                }
                path_hash_id.push((path, hash, photo_id));
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "pipeline: file processing error");
                errors.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!(error = %e, "pipeline: task join error");
                errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // ── Stage 3: detect RAW+JPG pairs and write paired_photo_id ─────────────
    let paths_only: Vec<PathBuf> = path_hash_id.iter().map(|(p, _, _)| p.clone()).collect();
    let (pairs, _) = crate::import::pair::detect_pairs(paths_only);

    for pair in &pairs {
        // Look up photo_ids for RAW and JPG by path.
        let raw_str = pair.raw.to_string_lossy().to_string();
        let jpg_str = pair.jpg.to_string_lossy().to_string();

        let raw_id_opt = path_hash_id
            .iter()
            .find(|(p, _, _)| p.to_string_lossy() == raw_str)
            .map(|(_, _, id)| *id);
        let jpg_id_opt = path_hash_id
            .iter()
            .find(|(p, _, _)| p.to_string_lossy() == jpg_str)
            .map(|(_, _, id)| *id);

        if let (Some(raw_id), Some(jpg_id)) = (raw_id_opt, jpg_id_opt) {
            // Point the JPG's paired_photo_id → RAW (RAW is master).
            if let Err(e) = sqlx::query("UPDATE photos SET paired_photo_id = ?1 WHERE id = ?2")
                .bind(raw_id)
                .bind(jpg_id)
                .execute(&pool)
                .await
            {
                tracing::warn!(error = %e, "pipeline: pair link update failed");
            }
        }
    }

    // ── Stage 4: AI enrichment (NIMA + SigLIP, model-optional) ─────────────
    if !new_photos.is_empty() {
        if let Ok(models_dir) = crate::util::paths::models_dir() {
            let nima_session =
                crate::ai::aesthetic::get_or_load(&models_dir.join("nima.onnx")).ok();
            let siglip_session =
                crate::ai::siglip::get_or_load(&models_dir.join("siglip-b16-image.onnx")).ok();

            if nima_session.is_some() || siglip_session.is_some() {
                // Obtain the model row id for embeddings (created lazily).
                let siglip_model_id = if siglip_session.is_some() {
                    crate::catalog::ensure_model_row(&pool, "siglip-b16-image", "embedding").await
                } else {
                    None
                };

                for (path, photo_id) in &new_photos {
                    // NIMA aesthetic score → photos.aesthetic_score
                    if let Some(nima) = nima_session {
                        let p = path.clone();
                        match tokio::task::spawn_blocking(move || nima.score(&p)).await {
                            Ok(Ok(score)) => {
                                if let Err(e) = sqlx::query(
                                    "UPDATE photos SET aesthetic_score = ?1 WHERE id = ?2",
                                )
                                .bind(score)
                                .bind(photo_id)
                                .execute(&pool)
                                .await
                                {
                                    tracing::warn!(error = %e, photo_id, "aesthetic score update failed");
                                }
                            }
                            Ok(Err(e)) => {
                                tracing::debug!(error = %e, photo_id, "nima score failed")
                            }
                            Err(e) => {
                                tracing::debug!(error = %e, photo_id, "nima task join failed")
                            }
                        }
                    }

                    // SigLIP embedding → photo_embeddings (BLOB fallback)
                    if let (Some(siglip), Some(model_id)) = (siglip_session, siglip_model_id) {
                        let p = path.clone();
                        match tokio::task::spawn_blocking(move || siglip.embed_image(&p)).await {
                            Ok(Ok(vec)) => {
                                let bytes: Vec<u8> =
                                    vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                                let now_ts = Utc::now().to_rfc3339();
                                if let Err(e) = sqlx::query(
                                    "INSERT OR REPLACE INTO photo_embeddings \
                                     (photo_id, model_id, embedding, updated_at) \
                                     VALUES (?1, ?2, ?3, ?4)",
                                )
                                .bind(photo_id)
                                .bind(model_id)
                                .bind(&bytes)
                                .bind(&now_ts)
                                .execute(&pool)
                                .await
                                {
                                    tracing::warn!(error = %e, photo_id, "embedding insert failed");
                                }
                            }
                            Ok(Err(e)) => {
                                tracing::debug!(error = %e, photo_id, "siglip embed failed")
                            }
                            Err(e) => {
                                tracing::debug!(error = %e, photo_id, "siglip task join failed")
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Finalise the imports row ─────────────────────────────────────────────
    let imported_count = imported.load(Ordering::Relaxed);
    let skipped_count = skipped.load(Ordering::Relaxed);
    let error_count = errors.load(Ordering::Relaxed);
    let finished_at = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE imports SET finished_at = ?1, imported_count = ?2, \
         skipped_count = ?3, error_count = ?4 WHERE id = ?5",
    )
    .bind(&finished_at)
    .bind(imported_count as i64)
    .bind(skipped_count as i64)
    .bind(error_count as i64)
    .bind(import_id)
    .execute(&pool)
    .await?;

    // Final progress event (done == total).
    on_progress(ImportProgress {
        source_id,
        import_id,
        total,
        done: total,
        current_file: String::new(),
        eta_seconds: Some(0),
    });

    // Refresh smart album counts so the UI reflects newly imported photos.
    if let Err(e) = crate::commands::refresh_album_counts(&pool).await {
        tracing::warn!(error = %e, "smart album refresh failed after import");
    }

    Ok(ImportResult {
        import_id,
        imported_count,
        skipped_count,
        error_count,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use std::fs;
    use tempfile::TempDir;

    /// Create an in-memory (well, temp-file) catalog and return the pool.
    async fn make_pool(tmp: &TempDir) -> SqlitePool {
        let db = tmp.path().join("catalog.db");
        open_pool(PoolOptions::new(db)).await.expect("open_pool")
    }

    /// Insert a `sources` row so the pipeline FK check passes.
    async fn seed_source(pool: &SqlitePool) -> i64 {
        let now = Utc::now().to_rfc3339();
        sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('test', 'local', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("insert source")
    }

    fn noop_progress() -> Arc<dyn Fn(ImportProgress) + Send + Sync> {
        Arc::new(|_: ImportProgress| ())
    }

    async fn seed_import(pool: &SqlitePool, source_id: i64) -> i64 {
        let now = Utc::now().to_rfc3339();
        sqlx::query_scalar(
            "INSERT INTO imports (source_id, started_at, total_files, imported_count, \
             skipped_count, error_count) VALUES (?1, ?2, 0, 0, 0, 0) RETURNING id",
        )
        .bind(source_id)
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("insert import")
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_imports_files_and_creates_rows() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_pool(&tmp).await;
        let source_id = seed_source(&pool).await;
        let import_id = seed_import(&pool, source_id).await;

        let root = tmp.path().join("photos");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("a.jpg"), b"fake-jpg").expect("write");
        fs::write(root.join("b.arw"), b"fake-arw").expect("write");

        let result = execute_pipeline(source_id, import_id, root, pool.clone(), noop_progress())
            .await
            .expect("pipeline");

        assert_eq!(result.imported_count, 2);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.error_count, 0);

        let photo_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(photo_count, 2);

        let copy_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM source_copies")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(copy_count, 2);

        let finished: Option<String> =
            sqlx::query_scalar("SELECT finished_at FROM imports WHERE id = ?1")
                .bind(result.import_id)
                .fetch_one(&pool)
                .await
                .expect("imports row");
        assert!(finished.is_some(), "finished_at should be set");
    }

    // ── Duplicate detection ───────────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_skips_duplicate_sha256() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_pool(&tmp).await;
        let source_id = seed_source(&pool).await;
        let import_id = seed_import(&pool, source_id).await;

        let root = tmp.path().join("photos");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("orig.jpg"), b"identical").expect("write");
        fs::write(root.join("copy.jpg"), b"identical").expect("write");

        let result = execute_pipeline(source_id, import_id, root, pool.clone(), noop_progress())
            .await
            .expect("pipeline");

        assert_eq!(result.imported_count + result.skipped_count, 2);
        assert_eq!(result.error_count, 0);

        let photo_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(photo_count, 1, "only one unique sha256 should be stored");
    }

    // ── RAW+JPG pair linking ──────────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_links_raw_jpg_pairs() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_pool(&tmp).await;
        let source_id = seed_source(&pool).await;
        let import_id = seed_import(&pool, source_id).await;

        let root = tmp.path().join("photos");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("IMG_0001.ARW"), b"raw-bytes").expect("write");
        fs::write(root.join("IMG_0001.JPG"), b"jpg-bytes").expect("write");

        execute_pipeline(source_id, import_id, root, pool.clone(), noop_progress())
            .await
            .expect("pipeline");

        let paired: Option<i64> =
            sqlx::query_scalar("SELECT paired_photo_id FROM photos WHERE is_raw = 0")
                .fetch_optional(&pool)
                .await
                .expect("query");

        assert!(paired.is_some(), "JPG should have a paired_photo_id");

        let raw_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE is_raw = 1")
            .fetch_one(&pool)
            .await
            .expect("raw row");

        assert_eq!(paired.unwrap(), raw_id);
    }

    // ── Empty directory ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_on_empty_dir_returns_zero_counts() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_pool(&tmp).await;
        let source_id = seed_source(&pool).await;
        let import_id = seed_import(&pool, source_id).await;

        let root = tmp.path().join("empty");
        fs::create_dir_all(&root).expect("mkdir");

        let result = execute_pipeline(source_id, import_id, root, pool.clone(), noop_progress())
            .await
            .expect("pipeline");

        assert_eq!(result.imported_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.error_count, 0);
    }

    // ── imports row total_files ───────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_records_total_files_in_imports_row() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_pool(&tmp).await;
        let source_id = seed_source(&pool).await;
        let import_id = seed_import(&pool, source_id).await;

        let root = tmp.path().join("photos");
        fs::create_dir_all(&root).expect("mkdir");
        for i in 0..5u8 {
            fs::write(root.join(format!("img{i}.jpg")), [i]).expect("write");
        }

        let result = execute_pipeline(source_id, import_id, root, pool.clone(), noop_progress())
            .await
            .expect("pipeline");

        let total: i64 = sqlx::query_scalar("SELECT total_files FROM imports WHERE id = ?1")
            .bind(result.import_id)
            .fetch_one(&pool)
            .await
            .expect("total_files");
        assert_eq!(total, 5);
    }
}
