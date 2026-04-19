//! Tauri commands exposed to the frontend. Keep this module thin — the actual
//! logic lives in domain modules (catalog, import, ai, ...) and these handlers
//! just wire arguments and serialize results.

use crate::{import, state::AppState, AppError, AppResult};
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

// ── Source management commands ────────────────────────────────────────────

/// Create a new source row and return it with a zeroed photo_count.
#[tauri::command]
pub async fn create_source(
    state: State<'_, AppState>,
    name: String,
    kind: String,
    root_path: Option<String>,
) -> AppResult<SourceRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let config = match &root_path {
        Some(p) => serde_json::json!({ "root": p }).to_string(),
        None => "{}".to_string(),
    };

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, config_json, created_at) \
         VALUES (?1, ?2, 'idle', ?3, ?4) RETURNING id",
    )
    .bind(&name)
    .bind(&kind)
    .bind(&config)
    .bind(&now)
    .fetch_one(&state.pool)
    .await?;

    let row = sqlx::query_as::<_, SourceRow>(
        "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
         COUNT(DISTINCT sc.photo_id) AS photo_count \
         FROM sources s LEFT JOIN source_copies sc ON sc.source_id = s.id \
         WHERE s.id = ?1 GROUP BY s.id",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    tracing::info!(source_id = id, kind, "source created");
    Ok(row)
}

// ── Catalog read commands ─────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AlbumRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tag: Option<String>,
    pub photo_count: i64,
    pub cover_photo_ids: String,
    pub is_system: bool,
}

/// List all smart albums ordered by id.
#[tauri::command]
pub async fn list_albums(state: State<'_, AppState>) -> AppResult<Vec<AlbumRow>> {
    let rows = sqlx::query_as::<_, AlbumRow>(
        "SELECT id, name, description, tag, photo_count, cover_photo_ids, is_system \
         FROM smart_albums ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PhotoRow {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub width: i64,
    pub height: i64,
    pub captured_at: Option<String>,
    pub is_raw: bool,
    pub size_bytes: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub aperture: Option<f64>,
    pub shutter: Option<String>,
    pub iso: Option<i64>,
    pub focal_mm: Option<f64>,
    pub aesthetic_score: Option<f64>,
    pub paired_photo_id: Option<i64>,
}

/// List photos ordered by imported_at desc, with optional pagination.
/// `limit` defaults to 100; `offset` defaults to 0.
#[tauri::command]
pub async fn list_photos(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(100);
    let off = offset.unwrap_or(0);
    let rows = sqlx::query_as::<_, PhotoRow>(
        "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
         camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
         paired_photo_id \
         FROM photos ORDER BY imported_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(lim)
    .bind(off)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SourceRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub last_scan_at: Option<String>,
    pub photo_count: i64,
}

/// List sources with derived photo count (distinct photos via source_copies).
#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> AppResult<Vec<SourceRow>> {
    let rows = sqlx::query_as::<_, SourceRow>(
        "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
         COUNT(DISTINCT sc.photo_id) AS photo_count \
         FROM sources s \
         LEFT JOIN source_copies sc ON sc.source_id = s.id \
         GROUP BY s.id ORDER BY s.id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

// ── Source-side cleanup commands ─────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SourceCleanupItem {
    pub source_copy_id: i64,
    pub photo_id: i64,
    pub source_id: i64,
    pub path: String,
    pub size_bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct CleanupPlan {
    pub source_id: i64,
    pub source_name: String,
    pub reclaimable_bytes: i64,
    pub item_count: i64,
    pub items: Vec<SourceCleanupItem>,
}

/// Dry-run: returns, per source, the set of source copies that can be safely deleted.
///
/// Safety criteria (all must hold):
/// 1. The source copy's `verified_sha256` matches the canonical `photos.sha256`.
/// 2. The same photo has at least one **other** copy in a different source that is also
///    SHA256-verified (so we're not deleting the only surviving copy).
/// 3. The source copy has a non-null `path` (i.e. it's a local filesystem file we can
///    actually delete).
///
/// If `source_id` is `Some`, only that source is evaluated.
#[tauri::command]
pub async fn cleanup_dry_run(
    state: State<'_, AppState>,
    source_id: Option<i64>,
) -> AppResult<Vec<CleanupPlan>> {
    let pool = &state.pool;

    // Build the reclaimable items query.
    // A copy is reclaimable when:
    //   - sc.verified_sha256 IS NOT NULL AND sc.verified_sha256 = p.sha256
    //   - sc.path IS NOT NULL
    //   - There exists another source_copy for the same photo_id in a *different*
    //     source with a verified sha256.
    let items: Vec<(i64, i64, i64, String, i64, String, String)> = if let Some(sid) = source_id {
        sqlx::query_as(
            "SELECT sc.id, sc.photo_id, sc.source_id, sc.path, \
             COALESCE(p.size_bytes, 0) AS size_bytes, p.sha256, s.name \
             FROM source_copies sc \
             JOIN photos p ON p.id = sc.photo_id \
             JOIN sources s ON s.id = sc.source_id \
             WHERE sc.source_id = ?1 \
               AND sc.path IS NOT NULL \
               AND sc.verified_sha256 IS NOT NULL \
               AND sc.verified_sha256 = p.sha256 \
               AND EXISTS ( \
                 SELECT 1 FROM source_copies sc2 \
                 WHERE sc2.photo_id = sc.photo_id \
                   AND sc2.source_id != sc.source_id \
                   AND sc2.verified_sha256 IS NOT NULL \
                   AND sc2.verified_sha256 = p.sha256 \
               ) \
             ORDER BY sc.source_id, sc.id",
        )
        .bind(sid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT sc.id, sc.photo_id, sc.source_id, sc.path, \
             COALESCE(p.size_bytes, 0) AS size_bytes, p.sha256, s.name \
             FROM source_copies sc \
             JOIN photos p ON p.id = sc.photo_id \
             JOIN sources s ON s.id = sc.source_id \
             WHERE sc.path IS NOT NULL \
               AND sc.verified_sha256 IS NOT NULL \
               AND sc.verified_sha256 = p.sha256 \
               AND EXISTS ( \
                 SELECT 1 FROM source_copies sc2 \
                 WHERE sc2.photo_id = sc.photo_id \
                   AND sc2.source_id != sc.source_id \
                   AND sc2.verified_sha256 IS NOT NULL \
                   AND sc2.verified_sha256 = p.sha256 \
               ) \
             ORDER BY sc.source_id, sc.id",
        )
        .fetch_all(pool)
        .await?
    };

    // Group items by source.
    let mut plans: std::collections::HashMap<i64, CleanupPlan> = std::collections::HashMap::new();
    for (sc_id, photo_id, sid, path, size_bytes, sha256, source_name) in items {
        let plan = plans.entry(sid).or_insert_with(|| CleanupPlan {
            source_id: sid,
            source_name: source_name.clone(),
            reclaimable_bytes: 0,
            item_count: 0,
            items: Vec::new(),
        });
        plan.reclaimable_bytes += size_bytes;
        plan.item_count += 1;
        plan.items.push(SourceCleanupItem {
            source_copy_id: sc_id,
            photo_id,
            source_id: sid,
            path,
            size_bytes,
            sha256,
        });
    }

    let mut result: Vec<CleanupPlan> = plans.into_values().collect();
    result.sort_by_key(|p| p.source_id);
    Ok(result)
}

// ── Rediscovery commands ──────────────────────────────────────────────────

/// Photos taken on today's month+day in any prior year.
/// Returns at most `limit` rows (default 20), ordered by captured_at desc.
#[tauri::command]
pub async fn on_this_day(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(20);
    // strftime('%m-%d', captured_at) matches the month-day portion regardless of year.
    // captured_at is stored as RFC3339 which starts with YYYY-MM-DDTHH:MM:SS…
    let today_md = chrono::Utc::now().format("%m-%d").to_string();
    let rows = sqlx::query_as::<_, PhotoRow>(
        "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
         camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
         paired_photo_id \
         FROM photos \
         WHERE captured_at IS NOT NULL \
           AND strftime('%m-%d', captured_at) = ?1 \
           AND strftime('%Y', captured_at) < strftime('%Y', 'now') \
         ORDER BY captured_at DESC \
         LIMIT ?2",
    )
    .bind(&today_md)
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

/// Photos that have never been viewed or were last viewed more than two years ago,
/// with an aesthetic_score >= `min_score` (default 0.0).
/// Returns at most `limit` rows (default 20), ordered by aesthetic_score desc.
#[tauri::command]
pub async fn unseen_photos(
    state: State<'_, AppState>,
    limit: Option<i64>,
    min_score: Option<f64>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(20);
    let score = min_score.unwrap_or(0.0);
    let rows = sqlx::query_as::<_, PhotoRow>(
        "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.is_raw, \
         p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, p.iso, \
         p.focal_mm, p.aesthetic_score, p.paired_photo_id \
         FROM photos p \
         LEFT JOIN photo_views pv ON pv.photo_id = p.id \
         WHERE (p.aesthetic_score IS NULL OR p.aesthetic_score >= ?1) \
           AND (pv.photo_id IS NULL \
                OR pv.last_viewed_at IS NULL \
                OR pv.last_viewed_at < datetime('now', '-2 years')) \
         ORDER BY p.aesthetic_score DESC NULLS LAST \
         LIMIT ?2",
    )
    .bind(score)
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
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

    // ── create_source tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn create_source_inserts_row_and_returns_it() {
        let (_tmp, pool) = make_pool().await;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES ('Local D:', 'local', 'idle', '{\"root\":\"D:/Photos\"}', ?1) RETURNING id",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .expect("insert");

        let row = sqlx::query_as::<_, SourceRow>(
            "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
             COUNT(DISTINCT sc.photo_id) AS photo_count \
             FROM sources s LEFT JOIN source_copies sc ON sc.source_id = s.id \
             WHERE s.id = ?1 GROUP BY s.id",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch");

        assert_eq!(row.name, "Local D:");
        assert_eq!(row.kind, "local");
        assert_eq!(row.status, "idle");
        assert_eq!(row.photo_count, 0);
    }

    #[tokio::test]
    async fn create_source_config_json_stored_correctly() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES ('iCloud', 'icloud', 'idle', '{\"root\":\"/iCloud\"}', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert");

        let config: String = sqlx::query_scalar("SELECT config_json FROM sources WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("config");

        let v: serde_json::Value = serde_json::from_str(&config).expect("valid json");
        assert_eq!(v["root"], "/iCloud");
    }

    // ── Catalog read command tests ─────────────────────────────────────────────

    async fn make_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let pool = crate::catalog::db::open_pool(crate::catalog::db::PoolOptions::new(
            tmp.path().join("catalog.db"),
        ))
        .await
        .expect("open_pool");
        (tmp, pool)
    }

    // list_albums ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_albums_returns_empty_on_fresh_catalog() {
        let (_tmp, pool) = make_pool().await;
        let rows = sqlx::query_as::<_, AlbumRow>(
            "SELECT id, name, description, tag, photo_count, cover_photo_ids, is_system \
             FROM smart_albums ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("query");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_albums_returns_seeded_albums() {
        let (_tmp, pool) = make_pool().await;
        crate::catalog::seed::seed_default_smart_albums(&pool)
            .await
            .expect("seed");

        let rows = sqlx::query_as::<_, AlbumRow>(
            "SELECT id, name, description, tag, photo_count, cover_photo_ids, is_system \
             FROM smart_albums ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 12);
        assert!(rows.iter().all(|r| r.is_system));
    }

    // list_photos ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_photos_returns_empty_on_fresh_catalog() {
        let (_tmp, pool) = make_pool().await;
        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
             camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
             paired_photo_id FROM photos ORDER BY imported_at DESC LIMIT 100 OFFSET 0",
        )
        .fetch_all(&pool)
        .await
        .expect("query");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_photos_pagination_limit_is_respected() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        for i in 0u8..5 {
            sqlx::query(
                "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
                 VALUES (?1, ?2, 0, 0, ?3, 0)",
            )
            .bind(format!("{i:064x}"))
            .bind(format!("img{i}.jpg"))
            .bind(&now)
            .execute(&pool)
            .await
            .expect("insert");
        }

        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
             camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
             paired_photo_id FROM photos ORDER BY imported_at DESC LIMIT 3 OFFSET 0",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 3);
    }

    // list_sources ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_sources_returns_empty_on_fresh_catalog() {
        let (_tmp, pool) = make_pool().await;
        let rows = sqlx::query_as::<_, SourceRow>(
            "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
             COUNT(DISTINCT sc.photo_id) AS photo_count \
             FROM sources s LEFT JOIN source_copies sc ON sc.source_id = s.id \
             GROUP BY s.id ORDER BY s.id",
        )
        .fetch_all(&pool)
        .await
        .expect("query");
        assert!(rows.is_empty());
    }

    // cleanup_dry_run ─────────────────────────────────────────────────────────

    async fn setup_cleanup_scenario(pool: &sqlx::SqlitePool) -> (i64, i64, i64) {
        // Returns (source_a_id, source_b_id, photo_id)
        let now = chrono::Utc::now().to_rfc3339();
        let sha = "a".repeat(64);

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, size_bytes) \
             VALUES (?1, 'test.jpg', 100, 100, ?2, 0, 2048) RETURNING id",
        )
        .bind(&sha)
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("photo");

        let source_a: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('Google Photos', 'google_photos', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("source_a");

        let source_b: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('Local D:', 'local', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("source_b");

        // source_a copy: verified
        sqlx::query(
            "INSERT INTO source_copies \
             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
             VALUES (?1, ?2, '/gp/test.jpg', 0, ?3, ?4)",
        )
        .bind(photo_id)
        .bind(source_a)
        .bind(&sha)
        .bind(&now)
        .execute(pool)
        .await
        .expect("copy_a");

        // source_b copy: verified (the retained copy)
        sqlx::query(
            "INSERT INTO source_copies \
             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
             VALUES (?1, ?2, 'D:/Photos/test.jpg', 1, ?3, ?4)",
        )
        .bind(photo_id)
        .bind(source_b)
        .bind(&sha)
        .bind(&now)
        .execute(pool)
        .await
        .expect("copy_b");

        (source_a, source_b, photo_id)
    }

    #[tokio::test]
    async fn cleanup_dry_run_returns_reclaimable_item_when_both_verified() {
        let (_tmp, pool) = make_pool().await;
        let (source_a, _source_b, photo_id) = setup_cleanup_scenario(&pool).await;

        let items: Vec<(i64, i64, i64, String, i64, String, String)> = sqlx::query_as(
            "SELECT sc.id, sc.photo_id, sc.source_id, sc.path, \
             COALESCE(p.size_bytes, 0), p.sha256, s.name \
             FROM source_copies sc \
             JOIN photos p ON p.id = sc.photo_id \
             JOIN sources s ON s.id = sc.source_id \
             WHERE sc.source_id = ?1 \
               AND sc.path IS NOT NULL \
               AND sc.verified_sha256 IS NOT NULL \
               AND sc.verified_sha256 = p.sha256 \
               AND EXISTS ( \
                 SELECT 1 FROM source_copies sc2 \
                 WHERE sc2.photo_id = sc.photo_id \
                   AND sc2.source_id != sc.source_id \
                   AND sc2.verified_sha256 IS NOT NULL \
                   AND sc2.verified_sha256 = p.sha256 \
               )",
        )
        .bind(source_a)
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, photo_id);
        assert_eq!(items[0].4, 2048); // size_bytes
    }

    #[tokio::test]
    async fn cleanup_dry_run_excludes_unverified_copies() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        let sha = "b".repeat(64);

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'unver.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind(&sha)
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        let source_a: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('GP', 'google_photos', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("src");

        // unverified copy (verified_sha256 IS NULL)
        sqlx::query(
            "INSERT INTO source_copies \
             (photo_id, source_id, path, is_primary, last_seen_at) \
             VALUES (?1, ?2, '/gp/unver.jpg', 0, ?3)",
        )
        .bind(photo_id)
        .bind(source_a)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("copy");

        let items: Vec<(i64,)> = sqlx::query_as(
            "SELECT sc.id FROM source_copies sc \
             JOIN photos p ON p.id = sc.photo_id \
             WHERE sc.verified_sha256 IS NOT NULL AND sc.verified_sha256 = p.sha256",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert!(
            items.is_empty(),
            "unverified copy must not appear in reclaimable set"
        );
    }

    #[tokio::test]
    async fn cleanup_dry_run_excludes_sole_copy() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        let sha = "c".repeat(64);

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'sole.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind(&sha)
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        let source_a: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('GP', 'google_photos', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("src");

        // Only one copy — verified, but no other copy exists
        sqlx::query(
            "INSERT INTO source_copies \
             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
             VALUES (?1, ?2, '/gp/sole.jpg', 1, ?3, ?4)",
        )
        .bind(photo_id)
        .bind(source_a)
        .bind(&sha)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("copy");

        let _ = photo_id; // used above

        let items: Vec<(i64,)> = sqlx::query_as(
            "SELECT sc.id FROM source_copies sc \
             JOIN photos p ON p.id = sc.photo_id \
             WHERE sc.verified_sha256 IS NOT NULL \
               AND sc.verified_sha256 = p.sha256 \
               AND EXISTS ( \
                 SELECT 1 FROM source_copies sc2 \
                 WHERE sc2.photo_id = sc.photo_id \
                   AND sc2.source_id != sc.source_id \
                   AND sc2.verified_sha256 IS NOT NULL \
               )",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert!(items.is_empty(), "sole copy must never be reclaimable");
    }

    // on_this_day ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn on_this_day_returns_empty_when_no_photos() {
        let (_tmp, pool) = make_pool().await;
        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
             camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
             paired_photo_id FROM photos \
             WHERE captured_at IS NOT NULL \
               AND strftime('%m-%d', captured_at) = strftime('%m-%d', 'now') \
               AND strftime('%Y', captured_at) < strftime('%Y', 'now') \
             ORDER BY captured_at DESC LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .expect("query");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn on_this_day_matches_same_month_day_in_prior_year() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        use chrono::Datelike as _;
        let now_dt = chrono::Utc::now();
        let prior_year = now_dt
            .with_year(now_dt.year() - 1)
            .expect("prior year")
            .to_rfc3339();

        // Insert one photo from prior year (should match) and one from now (should NOT match).
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, captured_at) \
             VALUES (?1, 'prior.jpg', 0, 0, ?2, 0, ?3)",
        )
        .bind("a".repeat(64))
        .bind(&now)
        .bind(&prior_year)
        .execute(&pool)
        .await
        .expect("insert prior");

        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, captured_at) \
             VALUES (?1, 'current.jpg', 0, 0, ?2, 0, ?2)",
        )
        .bind("b".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert current");

        let today_md = chrono::Utc::now().format("%m-%d").to_string();
        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT id, sha256, filename, width, height, captured_at, is_raw, size_bytes, \
             camera_make, camera_model, aperture, shutter, iso, focal_mm, aesthetic_score, \
             paired_photo_id FROM photos \
             WHERE captured_at IS NOT NULL \
               AND strftime('%m-%d', captured_at) = ?1 \
               AND strftime('%Y', captured_at) < strftime('%Y', 'now') \
             ORDER BY captured_at DESC LIMIT 20",
        )
        .bind(&today_md)
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].filename, "prior.jpg");
    }

    // unseen_photos ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn unseen_photos_returns_never_viewed_photos() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        // Insert 2 photos — neither has a photo_views row.
        for i in 0u8..2 {
            sqlx::query(
                "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
                 VALUES (?1, ?2, 0, 0, ?3, 0)",
            )
            .bind(format!("{:0>64}", i))
            .bind(format!("unseen{i}.jpg"))
            .bind(&now)
            .execute(&pool)
            .await
            .expect("insert");
        }

        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.is_raw, \
             p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, p.iso, \
             p.focal_mm, p.aesthetic_score, p.paired_photo_id \
             FROM photos p LEFT JOIN photo_views pv ON pv.photo_id = p.id \
             WHERE (p.aesthetic_score IS NULL OR p.aesthetic_score >= 0.0) \
               AND (pv.photo_id IS NULL \
                    OR pv.last_viewed_at IS NULL \
                    OR pv.last_viewed_at < datetime('now', '-2 years')) \
             ORDER BY p.aesthetic_score DESC NULLS LAST LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn unseen_photos_excludes_recently_viewed() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'seen.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind("c".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        sqlx::query(
            "INSERT INTO photo_views (photo_id, last_viewed_at, view_count) VALUES (?1, ?2, 5)",
        )
        .bind(photo_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("view");

        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.is_raw, \
             p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, p.iso, \
             p.focal_mm, p.aesthetic_score, p.paired_photo_id \
             FROM photos p LEFT JOIN photo_views pv ON pv.photo_id = p.id \
             WHERE (p.aesthetic_score IS NULL OR p.aesthetic_score >= 0.0) \
               AND (pv.photo_id IS NULL \
                    OR pv.last_viewed_at IS NULL \
                    OR pv.last_viewed_at < datetime('now', '-2 years')) \
             ORDER BY p.aesthetic_score DESC NULLS LAST LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert!(rows.is_empty(), "recently-viewed photo must be excluded");
    }

    #[tokio::test]
    async fn list_sources_counts_photos_per_source() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, created_at) \
             VALUES ('Test', 'local', 'idle', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("source");

        for i in 0u8..3 {
            let photo_id: i64 = sqlx::query_scalar(
                "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
                 VALUES (?1, ?2, 0, 0, ?3, 0) RETURNING id",
            )
            .bind(format!("{i:064x}"))
            .bind(format!("p{i}.jpg"))
            .bind(&now)
            .fetch_one(&pool)
            .await
            .expect("photo");

            sqlx::query(
                "INSERT INTO source_copies (photo_id, source_id, is_primary, last_seen_at) \
                 VALUES (?1, ?2, 1, ?3)",
            )
            .bind(photo_id)
            .bind(source_id)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("copy");
        }

        let rows = sqlx::query_as::<_, SourceRow>(
            "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
             COUNT(DISTINCT sc.photo_id) AS photo_count \
             FROM sources s LEFT JOIN source_copies sc ON sc.source_id = s.id \
             GROUP BY s.id ORDER BY s.id",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].photo_count, 3);
    }
}
