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
///
/// `finished` is the authoritative end-of-pipeline signal. The frontend
/// can't infer "done" from `done == total` alone — an empty import (scan
/// finds zero files) ends with `done=0, total=0` which is also the
/// initial registered state, so it would otherwise stay visible forever.
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgress {
    pub source_id: i64,
    pub import_id: i64,
    pub total: usize,
    pub done: usize,
    pub current_file: String,
    pub eta_seconds: Option<u64>,
    pub finished: bool,
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

/// Maximum concurrent AI-inference tasks (Stage 4 NIMA+SigLIP, Stage 5 face
/// detect+embed). Higher values let image decode + preprocessing overlap with
/// ort CPU inference on other photos, at the cost of oversubscribing cores
/// when ort's intra-op thread pool is already wide. 4 matches Stage 2 and
/// empirically keeps a modern 6–8 core CPU saturated without thrashing.
const MAX_CONCURRENT_AI_TASKS: usize = 4;

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

/// Headless variant of `run_pipeline` that accepts an explicit progress
/// callback instead of a `tauri::AppHandle`. Used by integration tests and
/// offline tools (CLI, benchmarks) that don't have a running Tauri runtime.
///
/// Creates the `imports` row and drives the full pipeline; progress events
/// go to `on_progress` instead of a Tauri event channel.
pub async fn run_pipeline_headless(
    source_id: i64,
    root: PathBuf,
    pool: SqlitePool,
    on_progress: Arc<dyn Fn(ImportProgress) + Send + Sync>,
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
    let pipeline_start = Instant::now();
    tracing::info!(
        source_id,
        import_id,
        root = %root.display(),
        "import pipeline: begin"
    );

    // ── Stage 1: scan ────────────────────────────────────────────────────────
    let stage1_start = Instant::now();
    let root_clone = root.clone();
    let entries = tokio::task::spawn_blocking(move || scan_dir(&ScanOptions::new(root_clone)))
        .await
        .map_err(|e| AppError::Internal(format!("scan task join: {e}")))??;

    let total = entries.len();
    tracing::info!(
        import_id,
        total,
        elapsed_ms = stage1_start.elapsed().as_millis() as u64,
        "import pipeline: stage 1 (scan) done"
    );

    // Update total_files in the imports row immediately so the UI can show it.
    sqlx::query("UPDATE imports SET total_files = ?1 WHERE id = ?2")
        .bind(total as i64)
        .bind(import_id)
        .execute(&pool)
        .await?;

    // ── Stage 2: hash + insert ───────────────────────────────────────────────
    let stage2_start = Instant::now();
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

            let photo_timer = Instant::now();
            let hash_start = Instant::now();
            let path_clone = path.clone();
            let hash = tokio::task::spawn_blocking(move || crate::import::sha256_file(&path_clone))
                .await
                .map_err(|e| AppError::Internal(format!("hash task join: {e}")))??;
            let hash_ms = hash_start.elapsed().as_millis() as u64;

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

                // Phase 4 §7 — XMP metadata import. Sidecar first (Lightroom
                // convention), falling back to the embedded XMP packet in
                // the photo itself (JPEG APP1 / TIFF-ARW tag 700). Best-
                // effort; malformed data logs a warn but never fails the
                // import.
                let xmp_data = match crate::xmp::sidecar_for(&path) {
                    Some(sidecar) => match crate::xmp::read_sidecar(&sidecar) {
                        Ok(Some(data)) if !data.is_empty() => Some(data),
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                sidecar = %sidecar.display(),
                                "xmp: read_sidecar failed"
                            );
                            None
                        }
                        _ => None,
                    },
                    None => match crate::xmp::read_embedded(&path) {
                        Ok(Some(data)) if !data.is_empty() => Some(data),
                        Err(e) => {
                            tracing::warn!(error = %e, path = %path.display(), "xmp: read_embedded failed");
                            None
                        }
                        _ => None,
                    },
                };
                if let Some(data) = xmp_data {
                    if let Err(e) = crate::xmp::apply_to_photo(&pool, photo_id, &data).await {
                        tracing::warn!(
                            error = %e,
                            path = %path.display(),
                            "xmp: apply_to_photo failed"
                        );
                    }
                }

                // Stage 2.5: extract EXIF + pHash for this photo.
                let meta_start = Instant::now();
                let meta_path = path.clone();
                let (exif, phash) = match tokio::task::spawn_blocking(move || {
                    let mut exif = crate::import::exif::read(&meta_path);
                    exif.orientation = crate::ai::image_util::effective_orientation_for_path(
                        &meta_path,
                        exif.orientation,
                    );
                    // Stage 2.5 runs before the AI-preview cache is written
                    // in stage 2.6, so we pass None and let phash decode via
                    // the full open_any cascade. For HEIC this is one WIC
                    // decode here; the cache benefits stages 4 and 5.
                    let phash = crate::dedupe::phash::compute(&meta_path, None);
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
                let meta_ms = meta_start.elapsed().as_millis() as u64;

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
                     phash            = COALESCE(?14, phash), \
                     orientation      = COALESCE(?15, orientation) \
                     WHERE id = ?16",
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
                .bind(exif.orientation.map(|v| v as i64))
                .bind(photo_id)
                .execute(&pool)
                .await
                {
                    tracing::warn!(error = %e, photo_id, "metadata UPDATE failed");
                }

                // Phase 4 §5 — nearest-city label for photos with GPS.
                if exif.gps_lat.is_some() && exif.gps_lng.is_some() {
                    if let Err(e) = crate::map::geocode::label_photo(&pool, photo_id).await {
                        tracing::warn!(error = %e, photo_id, "place_label write failed");
                    }
                }

                // Stage 2.6: apply EXIF orientation, cache a 320 px thumbnail,
                // and compute a Laplacian-variance sharpness score from the
                // same decoded + resized image. All in one spawn_blocking so
                // we decode the JPG exactly once.
                let thumb_start = Instant::now();
                let thumb_path = path.clone();
                let thumb_sha = hash.clone();
                let thumb_orientation = exif.orientation;
                let thumb_task = tokio::task::spawn_blocking(move || -> AppResult<Option<f32>> {
                    let result = crate::ai::image_util::write_thumbnail_cache_from_source(
                        &thumb_path,
                        &thumb_sha,
                        thumb_orientation,
                        320,
                        false,
                    );
                    Ok(Some(result?.sharpness))
                })
                .await;

                // Persist sharpness score if we got one. Best-effort.
                if let Ok(Ok(Some(score))) = &thumb_task {
                    if let Err(e) =
                        sqlx::query("UPDATE photos SET sharpness_score = ?1 WHERE id = ?2")
                            .bind(*score as f64)
                            .bind(photo_id)
                            .execute(&pool)
                            .await
                    {
                        tracing::warn!(error = %e, photo_id, "sharpness UPDATE failed");
                    }
                }
                let thumb_ms = thumb_start.elapsed().as_millis() as u64;

                tracing::debug!(
                    photo_id,
                    size_bytes,
                    is_raw = is_raw == 1,
                    total_ms = photo_timer.elapsed().as_millis() as u64,
                    hash_ms,
                    meta_ms,
                    thumb_ms,
                    "import: stage-2 per-photo timing"
                );
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
                finished: false,
            });

            Ok::<(PathBuf, String, i64, bool), AppError>((path, hash, photo_id, rows_affected > 0))
        });

        handles.push(handle);
    }

    // Collect results; track newly-inserted photos for the AI stage.
    // `new_photos` carries the sha256 so stages 4 and 5 can pass it as a
    // hint to `open_for_ai` and read the AI-preview cache instead of
    // re-decoding HEIC/RAW from the original.
    let mut path_hash_id: Vec<(PathBuf, String, i64)> = Vec::new();
    let mut new_photos: Vec<(PathBuf, String, i64)> = Vec::new();
    for h in handles {
        match h.await {
            Ok(Ok((path, hash, photo_id, was_inserted))) => {
                if was_inserted {
                    new_photos.push((path.clone(), hash.clone(), photo_id));
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

    tracing::info!(
        import_id,
        imported = imported.load(Ordering::Relaxed),
        skipped = skipped.load(Ordering::Relaxed),
        errors = errors.load(Ordering::Relaxed),
        elapsed_ms = stage2_start.elapsed().as_millis() as u64,
        per_photo_ms = if total > 0 {
            (stage2_start.elapsed().as_millis() as u64) / total as u64
        } else {
            0
        },
        "import pipeline: stage 2 (hash+EXIF+thumb) done"
    );

    // ── Stage 3: detect RAW+JPG pairs and write paired_photo_id ─────────────
    let stage3_start = Instant::now();
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
            // Pairing is bidirectional. The catalog grid uses the RAW as
            // the displayed master, but every preview / thumbnail /
            // develop path reads the *RAW row's* `paired_photo_id` to
            // decide "open the paired JPG instead of demosaicing the
            // RAW". Without the reverse link rawler tries to extract a
            // preview from the ARW directly — which the rawler 0.7 Sony
            // decoder can't always do (returns Ok(None) on A7 IV files
            // and falls back to a low-res embedded-JPEG byte scan).
            for (target, source) in [(raw_id, jpg_id), (jpg_id, raw_id)] {
                if let Err(e) = sqlx::query("UPDATE photos SET paired_photo_id = ?1 WHERE id = ?2")
                    .bind(target)
                    .bind(source)
                    .execute(&pool)
                    .await
                {
                    tracing::warn!(error = %e, "pipeline: pair link update failed");
                }
            }
        }
    }

    tracing::info!(
        import_id,
        pairs = pairs.len(),
        elapsed_ms = stage3_start.elapsed().as_millis() as u64,
        "import pipeline: stage 3 (pairs) done"
    );

    // ── Stage 4: AI enrichment (NIMA + SigLIP, model-optional) ─────────────
    let stage4_start = Instant::now();
    let new_photo_count = new_photos.len();
    //
    // SigLIP: prefer the process-wide global session (init'd at boot from
    // bundled models) over the legacy `get_or_load` path. Falls back to
    // `get_or_load` when the global has not been seeded (e.g. CLI / headless).
    //
    // Each photo runs NIMA → SigLIP sequentially inside its own task, and up
    // to `MAX_CONCURRENT_AI_TASKS` tasks run in parallel across photos. The
    // static session references are `Copy`, so the tokio tasks share them.
    if !new_photos.is_empty() {
        if let Ok(models_dir) = crate::util::paths::models_dir() {
            let nima_session =
                crate::ai::aesthetic::get_or_load(&models_dir.join("nima.onnx")).ok();

            // Prefer the memoised global session; fall back to get_or_load for
            // headless / test environments that never called init_global_siglip_session.
            let siglip_session: Option<&'static crate::ai::siglip::SigLipSession> =
                crate::ai::siglip::global_siglip_session().or_else(|| {
                    crate::ai::siglip::get_or_load(&models_dir.join("siglip2-b16-image.onnx")).ok()
                });

            if nima_session.is_some() || siglip_session.is_some() {
                // Obtain the model row id for embeddings (created lazily).
                let siglip_model_id = if siglip_session.is_some() {
                    crate::catalog::ensure_model_row(&pool, "siglip2-b16-image", "embedding").await
                } else {
                    None
                };

                let ai_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_AI_TASKS));
                let mut handles = Vec::with_capacity(new_photos.len());

                for (path, sha256, photo_id) in &new_photos {
                    let path = path.clone();
                    let sha256 = sha256.clone();
                    let photo_id = *photo_id;
                    let pool = pool.clone();
                    let sem = Arc::clone(&ai_sem);
                    let nima_opt = nima_session;
                    let siglip_opt = siglip_session;
                    let model_id_opt = siglip_model_id;

                    let handle = tokio::spawn(async move {
                        let ai_timer = Instant::now();
                        let _permit = match sem.acquire_owned().await {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::warn!(error = %e, photo_id, "ai semaphore closed");
                                return;
                            }
                        };

                        // NIMA aesthetic score → photos.aesthetic_score
                        let nima_start = Instant::now();
                        if let Some(nima) = nima_opt {
                            let p = path.clone();
                            let sha = sha256.clone();
                            match tokio::task::spawn_blocking(move || nima.score(&p, Some(&sha)))
                                .await
                            {
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

                        let nima_ms = nima_start.elapsed().as_millis() as u64;

                        // SigLIP embedding → photo_embeddings (BLOB) + vec_photo_embeddings (KNN).
                        let siglip_start = Instant::now();
                        if let (Some(siglip), Some(model_id)) = (siglip_opt, model_id_opt) {
                            let p = path.clone();
                            let sha = sha256.clone();
                            match tokio::task::spawn_blocking(move || {
                                siglip.embed_image(&p, Some(&sha))
                            })
                            .await
                            {
                                Ok(Ok(vec)) => {
                                    let bytes: Vec<u8> =
                                        vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                                    let now_ts = Utc::now().to_rfc3339();

                                    // Primary BLOB store (search_photos BLOB fallback path).
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

                                    // sqlite-vec f32 KNN store — retained for rare paths that
                                    // need exact cosine ordering / re-rank.
                                    if let Err(e) = sqlx::query(
                                        "INSERT OR REPLACE INTO vec_photo_embeddings \
                                         (rowid, embedding) VALUES (?1, ?2)",
                                    )
                                    .bind(photo_id)
                                    .bind(&bytes)
                                    .execute(&pool)
                                    .await
                                    {
                                        tracing::debug!(
                                            error = %e,
                                            photo_id,
                                            "vec_photo_embeddings insert skipped (sqlite-vec absent?)"
                                        );
                                    }

                                    // sqlite-vec int8 KNN store — the primary search_photos path.
                                    // 4× smaller bytes scanned per query → ~4× lower p95 at the
                                    // same catalog size; distance ordering preserved.
                                    let i8_bytes =
                                        crate::catalog::db::quantize_unit_f32_to_i8_bytes(&vec);
                                    if let Err(e) = sqlx::query(
                                        "INSERT OR REPLACE INTO vec_photo_embeddings_int8 \
                                         (rowid, embedding) VALUES (?1, vec_int8(?2))",
                                    )
                                    .bind(photo_id)
                                    .bind(&i8_bytes)
                                    .execute(&pool)
                                    .await
                                    {
                                        tracing::debug!(
                                            error = %e,
                                            photo_id,
                                            "vec_photo_embeddings_int8 insert skipped"
                                        );
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
                        let siglip_ms = siglip_start.elapsed().as_millis() as u64;

                        tracing::debug!(
                            photo_id,
                            total_ms = ai_timer.elapsed().as_millis() as u64,
                            nima_ms,
                            siglip_ms,
                            "import: stage-4 per-photo timing"
                        );
                    });

                    handles.push(handle);
                }

                for h in handles {
                    if let Err(e) = h.await {
                        tracing::warn!(error = %e, "stage-4 task join error");
                    }
                }
            }
        }
    }

    tracing::info!(
        import_id,
        new_photo_count,
        elapsed_ms = stage4_start.elapsed().as_millis() as u64,
        per_photo_ms = if new_photo_count > 0 {
            (stage4_start.elapsed().as_millis() as u64) / new_photo_count as u64
        } else {
            0
        },
        "import pipeline: stage 4 (NIMA + SigLIP) done"
    );

    // ── Stage 5: face detection + embedding (model-optional) ───────────────
    let stage5_start = Instant::now();
    //
    // Uses the process-wide `FacesSession` initialised once at app boot
    // (src-tauri/src/main.rs calls `init_global_faces_session`). When absent
    // — tests, fresh installs without bundled models, or CHRONIMAGE_MODELS_DIR
    // pointing at an empty dir — `global_faces_session` returns None and
    // stage-5 is skipped cleanly.
    //
    // Previously this block called `FacesSession::load(scrfd, arcface)` on
    // every pipeline invocation, committing ~190 MB of ONNX through ort
    // (~2 s per import batch). Memoising at boot eliminates the repeated init.
    //
    // Photos fan out to up to `MAX_CONCURRENT_AI_TASKS` parallel tasks. Face
    // embeddings within a single photo stay serial (typically 0–3 faces, so
    // per-face parallelism is not worth the coordination).
    if !new_photos.is_empty() {
        if let Some(session) = crate::ai::faces::global_faces_session() {
            let face_sem = Arc::new(Semaphore::new(MAX_CONCURRENT_AI_TASKS));
            let mut handles = Vec::with_capacity(new_photos.len());

            for (path, sha256, photo_id) in &new_photos {
                let path = path.clone();
                let sha256 = sha256.clone();
                let photo_id = *photo_id;
                let pool = pool.clone();
                let sem = Arc::clone(&face_sem);
                let faces_session = session;

                let handle = tokio::spawn(async move {
                    let _permit = match sem.acquire_owned().await {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!(error = %e, photo_id, "face semaphore closed");
                            return;
                        }
                    };

                    let path_c = path.clone();
                    let sha_for_detect = sha256.clone();
                    let detect_result = tokio::task::spawn_blocking(move || {
                        faces_session.detect_faces(&path_c, Some(&sha_for_detect))
                    })
                    .await;
                    let faces = match detect_result {
                        Ok(Ok(f)) => f,
                        Ok(Err(e)) => {
                            tracing::debug!(error = %e, photo_id, "scrfd detect failed");
                            return;
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, photo_id, "scrfd task join failed");
                            return;
                        }
                    };
                    let now_ts = Utc::now().to_rfc3339();
                    for face in faces {
                        let path_c = path.clone();
                        let sha_for_embed = sha256.clone();
                        let face_for_embed = face.clone();
                        let embed_result = tokio::task::spawn_blocking(move || {
                            faces_session.embed_face(&path_c, Some(&sha_for_embed), &face_for_embed)
                        })
                        .await;
                        let embedding = match embed_result {
                            Ok(Ok(v)) => v,
                            Ok(Err(e)) => {
                                tracing::debug!(error = %e, photo_id, "arcface embed failed");
                                continue;
                            }
                            Err(e) => {
                                tracing::debug!(error = %e, photo_id, "arcface task join failed");
                                continue;
                            }
                        };
                        let embedding_bytes: Vec<u8> =
                            embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                        if let Err(e) = sqlx::query(
                            "INSERT INTO faces \
                             (photo_id, bbox_x, bbox_y, bbox_w, bbox_h, quality, \
                              embedding, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        )
                        .bind(photo_id)
                        .bind(face.x as f64)
                        .bind(face.y as f64)
                        .bind(face.w as f64)
                        .bind(face.h as f64)
                        .bind(face.score as f64)
                        .bind(&embedding_bytes)
                        .bind(&now_ts)
                        .execute(&pool)
                        .await
                        {
                            tracing::warn!(error = %e, photo_id, "faces insert failed");
                        }
                    }
                });

                handles.push(handle);
            }

            for h in handles {
                if let Err(e) = h.await {
                    tracing::warn!(error = %e, "stage-5 task join error");
                }
            }
        }
    }

    tracing::info!(
        import_id,
        new_photo_count,
        elapsed_ms = stage5_start.elapsed().as_millis() as u64,
        per_photo_ms = if new_photo_count > 0 {
            (stage5_start.elapsed().as_millis() as u64) / new_photo_count as u64
        } else {
            0
        },
        "import pipeline: stage 5 (faces) done"
    );

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

    // Final progress event. `finished: true` is the authoritative end-of-
    // pipeline signal — the frontend listener uses it (not `done == total`)
    // because an empty import legitimately ends with `done=0, total=0`.
    on_progress(ImportProgress {
        source_id,
        import_id,
        total,
        done: total,
        current_file: String::new(),
        eta_seconds: Some(0),
        finished: true,
    });

    // Refresh smart album counts so the UI reflects newly imported photos.
    if let Err(e) = crate::commands::refresh_album_counts(&pool).await {
        tracing::warn!(error = %e, "smart album refresh failed after import");
    }

    // Re-run face clustering over all faces now that Stage 5 has added new
    // embeddings. Best-effort — log and continue on failure. The People
    // screen depends on this; without it `faces.cluster_id` stays NULL and
    // `face_clusters_list()` returns empty.
    let recluster_start = Instant::now();
    match crate::ai::cluster_persist::reeval_clusters(&pool).await {
        Ok(receipt) => tracing::info!(
            import_id,
            total_faces = receipt.total_faces,
            clustered_faces = receipt.clustered_faces,
            cluster_count = receipt.cluster_count,
            named_preserved = receipt.named_preserved,
            new_clusters = receipt.new_clusters,
            pruned_empty = receipt.pruned_empty,
            elapsed_ms = recluster_start.elapsed().as_millis() as u64,
            "reeval_clusters: done"
        ),
        Err(e) => tracing::warn!(error = %e, "reeval_clusters: failed"),
    }

    // Phase 4 §5 — recompute trips whenever new GPS-tagged photos land.
    // Best-effort; trips are a derived view.
    match crate::map::trips::recompute_trips(&pool).await {
        Ok(r) => tracing::info!(
            import_id,
            trip_count = r.trip_count,
            photo_count = r.photo_count,
            elapsed_ms = r.elapsed_ms,
            "recompute_trips: done"
        ),
        Err(e) => tracing::warn!(error = %e, "recompute_trips: failed"),
    }

    tracing::info!(
        import_id,
        total,
        new_photo_count,
        imported_count,
        skipped_count,
        error_count,
        total_elapsed_ms = pipeline_start.elapsed().as_millis() as u64,
        per_photo_ms = if total > 0 {
            (pipeline_start.elapsed().as_millis() as u64) / total as u64
        } else {
            0
        },
        "import pipeline: done"
    );

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
    use std::sync::Once;
    use tempfile::TempDir;

    /// Point `models_dir()` at an empty path for the entire test binary so
    /// stage-4 AI enrichment short-circuits instead of loading the developer's
    /// real models and running real inference against synthetic fixtures.
    /// Writes via `set_var` are UB if done after threads are spawned — this
    /// runs before any `tokio::test` body executes.
    fn isolate_models_dir() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // SAFETY: called before any tokio runtime or thread is spawned.
            unsafe {
                std::env::set_var(
                    "CHRONIMAGE_MODELS_DIR",
                    "\\nonexistent\\chronimage-test-models",
                );
            }
        });
    }

    /// Create an in-memory (well, temp-file) catalog and return the pool.
    async fn make_pool(tmp: &TempDir) -> SqlitePool {
        isolate_models_dir();
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

        let raw_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE is_raw = 1")
            .fetch_one(&pool)
            .await
            .expect("raw row");
        let jpg_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE is_raw = 0")
            .fetch_one(&pool)
            .await
            .expect("jpg row");

        let jpg_paired: Option<i64> =
            sqlx::query_scalar("SELECT paired_photo_id FROM photos WHERE id = ?1")
                .bind(jpg_id)
                .fetch_optional(&pool)
                .await
                .expect("query jpg paired");
        let raw_paired: Option<i64> =
            sqlx::query_scalar("SELECT paired_photo_id FROM photos WHERE id = ?1")
                .bind(raw_id)
                .fetch_optional(&pool)
                .await
                .expect("query raw paired");

        // Pairing must be bidirectional — every consumer (thumbnail
        // generator, develop preview, AI stages) reads the *RAW row's*
        // paired_photo_id to decide whether to fall through to rawler.
        assert_eq!(jpg_paired, Some(raw_id), "JPG should point at the RAW");
        assert_eq!(raw_paired, Some(jpg_id), "RAW should point back at the JPG");
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
