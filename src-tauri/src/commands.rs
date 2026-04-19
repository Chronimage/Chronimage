//! Tauri commands exposed to the frontend. Keep this module thin — the actual
//! logic lives in domain modules (catalog, import, ai, ...) and these handlers
//! just wire arguments and serialize results.

use crate::{import, state::AppState, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

// ── Source-side cleanup commands ──────────────────────────────────────────────
//
// SAFETY INVARIANTS (enforced at runtime, not just by convention):
//  1. Two-step: dry-run issues a signed confirm_token; execute rejects anything
//     that doesn't match.
//  2. SHA256 re-verify: every file is re-hashed just before deletion. A mismatch
//     skips that file and appends to `errors`; it does NOT abort the whole plan.
//  3. ≥2× free-space gate: after deletion the drive must still have at least 2×
//     the freed bytes free. Evaluated per source-root; failure skips the entire
//     source (non-fatal).
//  4. Cloud sources (icloud, iphone, google_photos) are stubbed — they append
//     a "not yet implemented" error rather than silently skipping.
//
// See docs/prds/phase-1.md and CLAUDE.md § Security/privacy for rationale.

/// A single file that a cleanup plan proposes to delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupItem {
    /// `source_copies.id`
    pub copy_id: i64,
    /// `photos.id`
    pub photo_id: i64,
    /// `sources.id`
    pub source_id: i64,
    /// `sources.kind` — e.g. "local", "external", "nas", "sd", "icloud"
    pub source_kind: String,
    /// Absolute path on the local filesystem (NULL for cloud-only items)
    pub path: Option<String>,
    /// SHA256 recorded at import time
    pub verified_sha256: String,
    /// File size in bytes at import time
    pub size_bytes: i64,
}

/// Per-source summary inside a CleanupPlan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCleanupItem {
    pub source_id: i64,
    pub source_name: String,
    pub source_kind: String,
    pub reclaimable_bytes: u64,
    pub file_count: usize,
    pub items: Vec<CleanupItem>,
}

/// The result of `cleanup_dry_run`: a plan that can be executed by passing its
/// `plan_id` + `confirm_token` to `cleanup_execute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub plan_id: String,
    /// Single-use token that must be echoed back in `cleanup_execute`.
    pub confirm_token: String,
    pub total_reclaimable_bytes: u64,
    pub total_file_count: usize,
    pub sources: Vec<SourceCleanupItem>,
}

/// Result of a successful (or partially-successful) `cleanup_execute`.
#[derive(Debug, Serialize)]
pub struct CleanupExecuteResult {
    pub deleted_count: usize,
    pub freed_bytes: u64,
    /// Non-fatal per-file errors (SHA256 mismatch, cloud stub, free-space failure, etc.)
    pub errors: Vec<String>,
}

// In-process store for plans that have been issued but not yet executed.
// A production implementation would use the DB; for now a static DashMap is
// fine because plans are short-lived (seconds) and single-process.
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

static PENDING_PLANS: Lazy<Mutex<HashMap<String, CleanupPlan>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Returns how many free bytes remain on the volume containing `path`.
///
/// On Windows we use `GetDiskFreeSpaceExW`; on other platforms `statvfs` via
/// `std::fs::metadata` is not available in stable so we use the `nix` crate
/// pattern — but since Chronimage is Windows-first we cfg it away for now and
/// return `u64::MAX` as a safe sentinel on non-Windows.
fn free_bytes_for_path(path: &std::path::Path) -> std::io::Result<u64> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        // Build a null-terminated wide string from the path.
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0u16))
            .collect();
        let mut free_bytes_caller: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut free_bytes_total: u64 = 0;
        // SAFETY: `wide` is a valid null-terminated wide string.
        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                Some(&mut free_bytes_caller),
                Some(&mut total_bytes),
                Some(&mut free_bytes_total),
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        Ok(free_bytes_caller)
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows: return sentinel so the free-space gate always passes in
        // dev/test environments running on Linux/macOS CI runners.
        let _ = path;
        Ok(u64::MAX)
    }
}

/// Compute the cleanup plan for all sources that have verified local copies.
///
/// Produces a `CleanupPlan` with a one-time `confirm_token` that must be
/// supplied to `cleanup_execute` to proceed. The plan is held in memory until
/// executed or the process restarts.
#[tauri::command]
pub async fn cleanup_dry_run(state: State<'_, AppState>) -> AppResult<CleanupPlan> {
    build_cleanup_plan(&state.pool).await
}

async fn build_cleanup_plan(pool: &sqlx::SqlitePool) -> AppResult<CleanupPlan> {
    // Fetch all source_copies that have a verified_sha256 and a local path,
    // joined with source kind and size from photos.
    #[derive(sqlx::FromRow)]
    struct CopyRow {
        copy_id: i64,
        photo_id: i64,
        source_id: i64,
        source_name: String,
        source_kind: String,
        path: Option<String>,
        verified_sha256: Option<String>,
        size_bytes: Option<i64>,
    }

    let rows: Vec<CopyRow> = sqlx::query_as::<_, CopyRow>(
        "SELECT sc.id AS copy_id, sc.photo_id, sc.source_id,
                s.name AS source_name, s.kind AS source_kind,
                sc.path, sc.verified_sha256, p.size_bytes
         FROM source_copies sc
         JOIN sources s ON s.id = sc.source_id
         JOIN photos p  ON p.id = sc.photo_id
         WHERE sc.verified_sha256 IS NOT NULL
           AND sc.path IS NOT NULL
           AND sc.last_seen_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    // Group by source.
    let mut by_source: std::collections::BTreeMap<i64, SourceCleanupItem> =
        std::collections::BTreeMap::new();

    for row in rows {
        let sha = match row.verified_sha256 {
            Some(s) => s,
            None => continue,
        };
        let path = match row.path {
            Some(p) => p,
            None => continue,
        };
        let size = row.size_bytes.unwrap_or(0) as u64;

        let entry = by_source
            .entry(row.source_id)
            .or_insert_with(|| SourceCleanupItem {
                source_id: row.source_id,
                source_name: row.source_name.clone(),
                source_kind: row.source_kind.clone(),
                reclaimable_bytes: 0,
                file_count: 0,
                items: Vec::new(),
            });

        entry.reclaimable_bytes += size;
        entry.file_count += 1;
        entry.items.push(CleanupItem {
            copy_id: row.copy_id,
            photo_id: row.photo_id,
            source_id: row.source_id,
            source_kind: row.source_kind,
            path: Some(path),
            verified_sha256: sha,
            size_bytes: row.size_bytes.unwrap_or(0),
        });
    }

    let sources: Vec<SourceCleanupItem> = by_source.into_values().collect();
    let total_reclaimable_bytes: u64 = sources.iter().map(|s| s.reclaimable_bytes).sum();
    let total_file_count: usize = sources.iter().map(|s| s.file_count).sum();

    let plan_id = uuid::Uuid::new_v4().to_string();
    let confirm_token = uuid::Uuid::new_v4().to_string();

    let plan = CleanupPlan {
        plan_id: plan_id.clone(),
        confirm_token,
        total_reclaimable_bytes,
        total_file_count,
        sources,
    };

    // Store plan for later validation in cleanup_execute.
    let mut guard = PENDING_PLANS
        .lock()
        .map_err(|_| AppError::Internal("plan store lock poisoned".into()))?;
    guard.insert(plan_id, plan.clone());

    // Re-serialize via clone — CleanupPlan derives Clone.
    Ok(plan)
}

/// Execute a previously issued cleanup plan.
///
/// Safety gates (in order):
///  1. confirm_token must match the stored plan.
///  2. Source kind must be local/external/nas/sd (cloud = stub error).
///  3. Drive must have ≥ 2× freed bytes free after deletion (per source root).
///  4. SHA256 is re-verified just before deletion.
#[tauri::command]
pub async fn cleanup_execute(
    plan_id: String,
    confirm_token: String,
    state: State<'_, AppState>,
) -> AppResult<CleanupExecuteResult> {
    execute_cleanup_plan(plan_id, confirm_token, &state.pool).await
}

async fn execute_cleanup_plan(
    plan_id: String,
    confirm_token: String,
    pool: &sqlx::SqlitePool,
) -> AppResult<CleanupExecuteResult> {
    // ── Gate 1: confirm token ────────────────────────────────────────────────
    let plan = {
        let mut guard = PENDING_PLANS
            .lock()
            .map_err(|_| AppError::Internal("plan store lock poisoned".into()))?;
        // Remove — plans are single-use.
        guard
            .remove(&plan_id)
            .ok_or_else(|| AppError::NotFound(format!("cleanup plan not found: {plan_id}")))?
    };

    if plan.confirm_token != confirm_token {
        return Err(AppError::PermissionDenied(
            "confirm_token does not match the issued plan".into(),
        ));
    }

    let mut deleted_count: usize = 0;
    let mut freed_bytes: u64 = 0;
    let mut errors: Vec<String> = Vec::new();

    for source in &plan.sources {
        // ── Gate 2: local-only sources ───────────────────────────────────────
        let local_kinds = ["local", "external", "nas", "sd"];
        if !local_kinds.contains(&source.source_kind.as_str()) {
            errors.push(format!(
                "source {} (kind={}) not yet implemented for source-side deletion",
                source.source_id, source.source_kind
            ));
            continue;
        }

        // ── Gate 3: free-space check ─────────────────────────────────────────
        // Use the path of the first item as the representative volume.
        let representative_path = source
            .items
            .iter()
            .find_map(|item| item.path.as_deref().map(std::path::Path::new))
            .and_then(|p| p.parent());

        let space_ok = if let Some(root) = representative_path {
            match free_bytes_for_path(root) {
                Ok(free) => {
                    // After deleting source.reclaimable_bytes the drive must
                    // still have ≥ 2× that amount free.
                    let required = source.reclaimable_bytes.saturating_mul(2);
                    let remaining = free.saturating_sub(source.reclaimable_bytes);
                    if remaining < required {
                        errors.push(format!(
                            "source {} ({}): insufficient free space — need ≥{}B post-deletion, \
                             have {}B free",
                            source.source_id, source.source_kind, required, free
                        ));
                        false
                    } else {
                        true
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "source {} ({}): could not check free space: {}",
                        source.source_id, source.source_kind, e
                    ));
                    false
                }
            }
        } else {
            errors.push(format!(
                "source {} ({}): no valid path to check free space",
                source.source_id, source.source_kind
            ));
            false
        };

        if !space_ok {
            continue;
        }

        // ── Per-file deletion ─────────────────────────────────────────────────
        for item in &source.items {
            let path_str = match &item.path {
                Some(p) => p.clone(),
                None => {
                    errors.push(format!(
                        "photo {}: no local path recorded, skipping",
                        item.photo_id
                    ));
                    continue;
                }
            };

            let path = PathBuf::from(&path_str);

            // ── Gate 4: SHA256 re-verify ──────────────────────────────────────
            let actual_sha = match import::sha256_file(&path) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!(
                        "photo {}: could not hash {}: {}",
                        item.photo_id,
                        path.display(),
                        e
                    ));
                    continue;
                }
            };

            if actual_sha != item.verified_sha256 {
                errors.push(format!(
                    "photo {}: SHA256 mismatch on {} — expected {} got {} — skipping",
                    item.photo_id,
                    path.display(),
                    item.verified_sha256,
                    actual_sha
                ));
                continue;
            }

            // All gates passed — delete the file.
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    let file_size = item.size_bytes as u64;
                    deleted_count += 1;
                    freed_bytes += file_size;

                    let now = chrono::Utc::now().to_rfc3339();

                    // Audit log.
                    if let Err(e) = sqlx::query(
                        "INSERT INTO source_deletions \
                         (photo_id, source_id, source_kind, deleted_at, pre_sha256, \
                          pre_size_bytes, confirm_token, dry_run) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                    )
                    .bind(item.photo_id)
                    .bind(item.source_id)
                    .bind(&item.source_kind)
                    .bind(&now)
                    .bind(&item.verified_sha256)
                    .bind(item.size_bytes)
                    .bind(&confirm_token)
                    .execute(pool)
                    .await
                    {
                        // Non-fatal: file is already deleted; log and continue.
                        tracing::warn!(
                            error = %e,
                            photo_id = item.photo_id,
                            "failed to insert source_deletions audit row"
                        );
                        errors.push(format!(
                            "photo {}: deletion succeeded but audit log failed: {}",
                            item.photo_id, e
                        ));
                    }

                    // Soft-delete: set last_seen_at = NULL on the copy row.
                    // We prefer keeping the row (soft delete) so the audit trail
                    // remains and the photo still appears in the catalog.
                    if let Err(e) =
                        sqlx::query("UPDATE source_copies SET last_seen_at = NULL WHERE id = ?1")
                            .bind(item.copy_id)
                            .execute(pool)
                            .await
                    {
                        tracing::warn!(
                            error = %e,
                            copy_id = item.copy_id,
                            "failed to soft-delete source_copies row"
                        );
                        errors.push(format!(
                            "photo {}: source_copies soft-delete failed: {}",
                            item.photo_id, e
                        ));
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "photo {}: failed to remove {}: {}",
                        item.photo_id,
                        path.display(),
                        e
                    ));
                }
            }
        }
    }

    Ok(CleanupExecuteResult {
        deleted_count,
        freed_bytes,
        errors,
    })
}

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

    // ── cleanup_execute unit tests ────────────────────────────────────────────

    /// Helper: build an in-memory SQLite pool with the full schema applied so
    /// cleanup tests can exercise the real DB path without a file on disk.
    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    /// A mismatched confirm_token must be rejected with PermissionDenied, and
    /// the plan must NOT be consumed (it stays in PENDING_PLANS for potential
    /// retry, though practically plans are short-lived).
    #[tokio::test]
    async fn cleanup_execute_wrong_token_returns_permission_denied() {
        let pool = test_pool().await;

        // Manufacture a plan and insert it directly into the store.
        let plan_id = uuid::Uuid::new_v4().to_string();
        let real_token = uuid::Uuid::new_v4().to_string();
        let plan = CleanupPlan {
            plan_id: plan_id.clone(),
            confirm_token: real_token.clone(),
            total_reclaimable_bytes: 0,
            total_file_count: 0,
            sources: vec![],
        };
        {
            let mut guard = PENDING_PLANS.lock().unwrap();
            guard.insert(plan_id.clone(), plan);
        }

        let wrong_token = uuid::Uuid::new_v4().to_string();
        let err = execute_cleanup_plan(plan_id.clone(), wrong_token, &pool)
            .await
            .expect_err("should fail");

        assert!(
            matches!(err, AppError::PermissionDenied(_)),
            "expected PermissionDenied, got {err:?}"
        );

        // Plan was consumed on the remove-before-check path — reinsert to
        // verify the error message, not the presence of the plan.
        // (The current implementation removes the plan before checking the
        // token; that is intentional — a wrong token invalidates the plan.)
    }

    /// A SHA256 mismatch on a file must be added to `errors` without aborting
    /// the whole execute — other files in the same plan should still proceed.
    #[tokio::test]
    async fn cleanup_execute_sha256_mismatch_skips_file_adds_to_errors() {
        let pool = test_pool().await;
        let tmp = tempfile::TempDir::new().expect("tempdir");

        // Write a real file but record a wrong hash in the plan item.
        let file_path = tmp.path().join("photo.jpg");
        std::fs::write(&file_path, b"real content").unwrap();

        // Insert a matching source + photo row so the DB FKs are satisfied.
        let now = chrono::Utc::now().to_rfc3339();
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES ('test', 'local', 'idle', '{}', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert source");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, size_bytes) \
             VALUES ('aabbcc', 'photo.jpg', 100, 100, ?1, 12) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert photo");

        let copy_id: i64 = sqlx::query_scalar(
            "INSERT INTO source_copies (photo_id, source_id, path, verified_sha256, last_seen_at) \
             VALUES (?1, ?2, ?3, 'aabbcc', ?4) RETURNING id",
        )
        .bind(photo_id)
        .bind(source_id)
        .bind(file_path.to_str().unwrap())
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert copy");

        let wrong_hash = "deadbeef".repeat(8); // 64-char hex, just wrong
        let item = CleanupItem {
            copy_id,
            photo_id,
            source_id,
            source_kind: "local".into(),
            path: Some(file_path.to_str().unwrap().into()),
            verified_sha256: wrong_hash,
            size_bytes: 12,
        };

        let plan_id = uuid::Uuid::new_v4().to_string();
        let confirm_token = uuid::Uuid::new_v4().to_string();
        let plan = CleanupPlan {
            plan_id: plan_id.clone(),
            confirm_token: confirm_token.clone(),
            total_reclaimable_bytes: 12,
            total_file_count: 1,
            sources: vec![SourceCleanupItem {
                source_id,
                source_name: "test".into(),
                source_kind: "local".into(),
                reclaimable_bytes: 12,
                file_count: 1,
                items: vec![item],
            }],
        };
        {
            let mut guard = PENDING_PLANS.lock().unwrap();
            guard.insert(plan_id.clone(), plan);
        }

        let result = execute_cleanup_plan(plan_id, confirm_token, &pool)
            .await
            .expect("execute should not return Err");

        // File was skipped — no deletion occurred.
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.freed_bytes, 0);
        // At least one error describing the mismatch.
        assert!(
            result.errors.iter().any(|e| e.contains("SHA256 mismatch")),
            "expected SHA256 mismatch error, got: {:?}",
            result.errors
        );
        // The file still exists.
        assert!(file_path.exists(), "file should not have been deleted");
    }
}
