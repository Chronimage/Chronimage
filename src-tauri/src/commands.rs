//! Tauri commands exposed to the frontend. Keep this module thin — the actual
//! logic lives in domain modules (catalog, import, ai, ...) and these handlers
//! just wire arguments and serialize results.

use crate::{
    ai::{dot_product, l2_normalise, SigLipSession},
    import,
    state::AppState,
    AppError, AppResult,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

/// Smoke command used by the frontend at boot to verify the IPC bridge.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// Current binary version. Read from Cargo at compile time so it's always in sync.
#[tauri::command]
pub async fn app_version() -> AppResult<String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[derive(Debug, Serialize)]
pub struct CurrentChannel {
    pub channel: &'static str,
}

#[tauri::command]
pub async fn current_channel() -> AppResult<CurrentChannel> {
    let channel = option_env!("CHRONIMAGE_RELEASE_CHANNEL").unwrap_or("dev");
    Ok(CurrentChannel { channel })
}

// ── Phase 1 scaffolding commands ──────────────────────────────────────────
//
// These currently operate against no DB — they walk the filesystem and
// report what a real import would find. Wiring them to the catalog pool
// happens in the next Phase 1 session once the pool is lifetime-bound to
// Tauri's `State`.

#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub root: PathBuf,
    pub total_files: usize,
    pub raw_jpg_pairs: usize,
    pub unpaired: usize,
    pub by_extension: Vec<ExtCount>,
}

#[derive(Debug, Serialize)]
pub struct ExtCount {
    pub ext: String,
    pub count: usize,
}

/// Preview a scan of a directory without writing to the catalog.
/// Counts files by extension and detects RAW+JPG pairs.
#[tauri::command]
pub async fn import_dry_run(_state: State<'_, AppState>, root: String) -> AppResult<ScanReport> {
    dry_scan(root).await
}

async fn dry_scan(root: String) -> AppResult<ScanReport> {
    let root_path = PathBuf::from(&root);
    let opts = import::scanner::ScanOptions::new(root_path.clone());
    let entries = tokio::task::spawn_blocking(move || import::scanner::scan_dir(&opts))
        .await
        .map_err(|e| AppError::Internal(format!("scan task join error: {e}")))??;

    let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    let (pairs, leftovers) = import::pair::detect_pairs(paths);

    let mut by_ext: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &entries {
        *by_ext.entry(e.ext_lowercase.clone()).or_default() += 1;
    }
    let mut by_extension: Vec<ExtCount> = by_ext
        .into_iter()
        .map(|(ext, count)| ExtCount { ext, count })
        .collect();
    by_extension.sort_by_key(|e| std::cmp::Reverse(e.count));

    Ok(ScanReport {
        root: root_path,
        total_files: entries.len(),
        raw_jpg_pairs: pairs.len(),
        unpaired: leftovers.len(),
        by_extension,
    })
}

// ── Phase 1 pipeline commands ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StartImportResponse {
    pub import_id: i64,
}

/// Launch the import pipeline as a detached task. Returns `{ import_id }`
/// immediately; progress comes via `"chronimage://import-progress"` events.
///
/// The `sources` row for `source_id` must already exist.
#[tauri::command]
pub async fn start_import(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    source_id: i64,
    root: String,
) -> AppResult<StartImportResponse> {
    let root_path = PathBuf::from(&root);
    if !root_path.exists() {
        return Err(AppError::NotFound(format!(
            "import root does not exist: {root}"
        )));
    }

    let pool = state.pool.clone();

    // Create the imports row synchronously so the caller gets the id before
    // the background task even starts scanning.
    let now = chrono::Utc::now().to_rfc3339();
    let import_id: i64 = sqlx::query_scalar(
        "INSERT INTO imports (source_id, started_at, total_files, imported_count, \
         skipped_count, error_count) VALUES (?1, ?2, 0, 0, 0, 0) RETURNING id",
    )
    .bind(source_id)
    .bind(&now)
    .fetch_one(&pool)
    .await?;

    // Detach — errors are logged inside run_pipeline_detached.
    let pool2 = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = import::pipeline::run_pipeline_from_import_id(
            source_id, import_id, root_path, pool2, app_handle,
        )
        .await
        {
            tracing::error!(error = %e, import_id, "import pipeline failed");
        }
    });

    Ok(StartImportResponse { import_id })
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ImportSummary {
    pub id: i64,
    pub source_id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub total_files: i64,
    pub imported_count: i64,
    pub skipped_count: i64,
    pub error_count: i64,
    pub last_seen_path: Option<String>,
}

/// List imports, optionally filtered by `source_id`.
#[tauri::command]
pub async fn list_imports(
    state: State<'_, AppState>,
    source_id: Option<i64>,
) -> AppResult<Vec<ImportSummary>> {
    query_imports(&state.pool, source_id).await
}

async fn query_imports(
    pool: &sqlx::SqlitePool,
    source_id: Option<i64>,
) -> AppResult<Vec<ImportSummary>> {
    let rows: Vec<ImportSummary> = if let Some(sid) = source_id {
        sqlx::query_as::<_, ImportSummary>(
            "SELECT id, source_id, started_at, finished_at, total_files, \
             imported_count, skipped_count, error_count, last_seen_path \
             FROM imports WHERE source_id = ?1 ORDER BY id DESC",
        )
        .bind(sid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, ImportSummary>(
            "SELECT id, source_id, started_at, finished_at, total_files, \
             imported_count, skipped_count, error_count, last_seen_path \
             FROM imports ORDER BY id DESC",
        )
        .fetch_all(pool)
        .await?
    };

    Ok(rows)
}

// ── Natural-language search ───────────────────────────────────────────────

/// A photo row returned by search (and eventually the catalog grid).
///
/// Field names mirror the `photos` table columns that the frontend grid
/// already understands via `FixturePhoto`. Optional Phase 1 columns are
/// included when populated.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PhotoRow {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub width: i64,
    pub height: i64,
    pub captured_at: Option<String>,
    pub imported_at: String,
    pub is_raw: bool,
    pub paired_photo_id: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub aesthetic_score: Option<f64>,
    pub size_bytes: Option<i64>,
    pub raw_format: Option<String>,
}

/// Encode `query` with the SigLIP text encoder and return the `limit`
/// (default 50) most similar photos ordered by cosine similarity desc.
///
/// Uses the BLOB fallback path: reads `embedding` from `photo_embeddings`,
/// L2-normalises, computes dot product with the query vector. This handles
/// the case where `vec_rowid` is NULL (sqlite-vec unavailable or not yet
/// populated).
///
/// Returns an empty vec — not an error — when:
/// - No photos have stored embeddings yet.
/// - The SigLIP model is absent (stub emits a zero-vector → all scores 0.0).
#[tauri::command]
pub async fn search_photos(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<Vec<PhotoRow>> {
    let max_results = limit.unwrap_or(50).clamp(1, 1000);

    // 1. Encode the query text into a 768-dim vector via the SigLIP stub.
    //    Model path: `{data_local_dir}/app.chronimage.desktop/models/siglip_text.onnx`
    //    We pass None for now — the stub will be used if the file is absent.
    let models_dir = crate::util::paths::models_dir().ok();
    let model_path = models_dir.as_deref().map(|d| d.join("siglip_text.onnx"));
    let session = SigLipSession::load_or_stub(model_path.as_deref());

    let mut query_vec = session.embed_text(&query)?;

    // 2. L2-normalise. If the model is absent this stays a zero-vector and
    //    all dot products will be 0.0 — effectively returning no results.
    l2_normalise(&mut query_vec);

    // If the query vector is all-zero (stub) there's nothing meaningful to
    // rank; return empty rather than an arbitrary ordering.
    let is_zero = query_vec.iter().all(|x| *x == 0.0);
    if is_zero {
        tracing::debug!(
            query = %query,
            "search_photos: SigLIP stub returned zero vector — no results"
        );
        return Ok(Vec::new());
    }

    // 3. Fetch all photo_id + embedding BLOBs (BLOB fallback path).
    //    We only pull rows that have a non-null embedding BLOB.
    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT photo_id, embedding FROM photo_embeddings \
         WHERE embedding IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // 4. Decode each BLOB (768 × f32 LE), L2-normalise, compute dot product.
    let expected_bytes = crate::ai::EMBED_DIM * std::mem::size_of::<f32>();
    let mut scored: Vec<(i64, f32)> = rows
        .into_iter()
        .filter_map(|(photo_id, blob)| {
            if blob.len() != expected_bytes {
                tracing::warn!(
                    photo_id,
                    blob_len = blob.len(),
                    expected = expected_bytes,
                    "photo_embeddings BLOB has wrong length — skipping"
                );
                return None;
            }
            let mut emb: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            l2_normalise(&mut emb);
            let score = dot_product(&query_vec, &emb);
            Some((photo_id, score))
        })
        .collect();

    // 5. Sort by score descending, take top N.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_results as usize);

    if scored.is_empty() {
        return Ok(Vec::new());
    }

    // 6. Fetch full PhotoRow data for the ranked photo IDs.
    //    We preserve the score order by fetching all and re-sorting.
    let photo_ids: Vec<i64> = scored.iter().map(|(id, _)| *id).collect();

    // Build a parameterised IN clause. sqlx doesn't support dynamic IN with
    // query_as!, so we fall back to the dynamic form.
    let placeholders: String = photo_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, sha256, filename, width, height, captured_at, imported_at, \
         is_raw, paired_photo_id, camera_make, camera_model, aesthetic_score, \
         size_bytes, raw_format \
         FROM photos WHERE id IN ({placeholders})"
    );

    let mut q = sqlx::query_as::<_, PhotoRow>(&sql);
    for id in &photo_ids {
        q = q.bind(id);
    }
    let mut photos: Vec<PhotoRow> = q.fetch_all(&state.pool).await?;

    // Re-sort to match the score ordering from step 5.
    let order: std::collections::HashMap<i64, usize> = photo_ids
        .iter()
        .enumerate()
        .map(|(rank, &id)| (id, rank))
        .collect();
    photos.sort_by_key(|p| order.get(&p.id).copied().unwrap_or(usize::MAX));

    Ok(photos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong() {
        assert_eq!(ping(), "pong");
    }

    #[tokio::test]
    async fn app_version_matches_cargo() {
        let v = app_version().await.expect("version");
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn current_channel_defaults_to_dev() {
        let c = current_channel().await.expect("channel");
        assert!(
            matches!(c.channel, "dev" | "stable" | "beta" | "nightly" | "insider"),
            "unexpected channel: {}",
            c.channel
        );
    }

    #[tokio::test]
    async fn import_dry_run_on_empty_dir_returns_zero() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let report = dry_scan(tmp.path().to_str().unwrap().to_string())
            .await
            .expect("report");
        assert_eq!(report.total_files, 0);
        assert_eq!(report.raw_jpg_pairs, 0);
    }

    #[tokio::test]
    async fn import_dry_run_detects_pairs_and_singles() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let r = tmp.path();
        std::fs::write(r.join("IMG_0001.ARW"), b"x").unwrap();
        std::fs::write(r.join("IMG_0001.JPG"), b"x").unwrap();
        std::fs::write(r.join("IMG_0002.HEIC"), b"x").unwrap();
        let report = dry_scan(r.to_str().unwrap().to_string())
            .await
            .expect("report");
        assert_eq!(report.total_files, 3);
        assert_eq!(report.raw_jpg_pairs, 1);
        assert_eq!(report.unpaired, 1);
    }
}
