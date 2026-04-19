//! Tauri commands exposed to the frontend. Keep this module thin — the actual
//! logic lives in domain modules (catalog, import, ai, ...) and these handlers
//! just wire arguments and serialize results.

use crate::{dedupe::confirm::DuplicateGroup, import, state::AppState, AppError, AppResult};
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

// ── Dedupe commands ───────────────────────────────────────────────────────────

/// Find groups of semantically similar photos using SigLIP cosine similarity.
///
/// `min_similarity` defaults to `0.90` when `None`. Pass `0.95` to restrict
/// to exact duplicates only.
///
/// RAW+JPG pairs are excluded from consideration — they are intentional
/// stacks, not duplicates.
#[tauri::command]
pub async fn find_duplicates(
    state: State<'_, AppState>,
    min_similarity: Option<f64>,
) -> AppResult<Vec<DuplicateGroup>> {
    let threshold = min_similarity.unwrap_or(0.90);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(AppError::InvalidInput(format!(
            "min_similarity must be between 0.0 and 1.0, got {threshold}"
        )));
    }
    crate::dedupe::confirm::find_duplicate_groups(&state.pool, threshold).await
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
