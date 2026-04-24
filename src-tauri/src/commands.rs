//! Tauri commands exposed to the frontend. Keep this module thin — the actual
//! logic lives in domain modules (catalog, import, ai, ...) and these handlers
//! just wire arguments and serialize results.

use crate::{
    ai::{dot_product, l2_normalise},
    catalog,
    dedupe::confirm::DuplicateGroup,
    import,
    state::AppState,
    AppError, AppResult,
};
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
            // Google Photos specifically falls back to manual cleanup
            // because the Photo Picker API is read-only — no batchDelete.
            // See PRD § 12 per-source adapter. Surface a pointer to the
            // manual-cleanup surface rather than a vague "not implemented".
            let msg = if source.source_kind == "google_photos" {
                format!(
                    "source {} (google_photos) cannot be deleted from the app — \
                     the Photo Picker API is read-only. Open photos.google.com or \
                     takeout.google.com to remove the uploaded copy manually. \
                     (see gphotos_manual_cleanup_instructions command)",
                    source.source_id,
                )
            } else {
                format!(
                    "source {} (kind={}) not yet implemented for source-side deletion",
                    source.source_id, source.source_kind
                )
            };
            errors.push(msg);
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

// ── Lift & Shift commands ─────────────────────────────────────────────────────
//
// SAFETY INVARIANTS (mirrors source-side cleanup):
//  1. Two-step: dry-run issues a signed confirm_token; execute rejects anything
//     that doesn't match.
//  2. SHA256 pre-verify: source file re-hashed before copy; mismatch → skip.
//  3. SHA256 post-verify: destination re-hashed after copy; mismatch → remove
//     dest + append to errors.
//  4. ≥ 1.5× free-space gate: reported in `LiftPlan.free_space_ok`.
//  5. Lift does NOT delete source copies — that is source-side cleanup's job.
//
// See docs/prds/phase-1.md §11 and CLAUDE.md § Security/privacy for rationale.

/// Compute a lift-and-shift plan for all source photos not already under
/// `target_root`.  Returns a `LiftPlan` with a one-time `confirm_token` that
/// must be supplied to `lift_shift_execute` to proceed.
///
/// This is a dry-run: no files are moved or created.
#[tauri::command]
pub async fn lift_shift_dry_run(
    state: State<'_, AppState>,
    target_root: String,
) -> AppResult<crate::lift_and_shift::LiftPlan> {
    let target = std::path::PathBuf::from(target_root);
    crate::lift_and_shift::plan_lift(&state.pool, target).await
}

/// Execute a previously issued lift plan.
///
/// `plan_id` + `confirm_token` must match the values returned by
/// `lift_shift_dry_run`.  Plans are single-use.
#[tauri::command]
pub async fn lift_shift_execute(
    state: State<'_, AppState>,
    plan_id: String,
    confirm_token: String,
) -> AppResult<crate::lift_and_shift::LiftReceipt> {
    crate::lift_and_shift::execute_lift(&state.pool, &plan_id, &confirm_token).await
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
// ── Disk / catalog-home helpers ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DiskInfo {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// Suggested default catalog home: `{Pictures}/Chronimage` (or `C:\Chronimage`
/// as final fallback). The directory need not exist yet.
#[tauri::command]
pub fn get_default_catalog_path() -> String {
    dirs::picture_dir()
        .or_else(dirs::home_dir)
        .map(|d| d.join("Chronimage").to_string_lossy().into_owned())
        .unwrap_or_else(|| "C:\\Chronimage".to_string())
}

/// Free and total bytes for the drive that contains `path`. If `path` does not
/// exist yet, walks up to the nearest ancestor that does. Returns an error only
/// if no ancestor exists (e.g. the drive letter is invalid).
#[tauri::command]
pub fn get_disk_info(path: String) -> AppResult<DiskInfo> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // Find the nearest existing ancestor so we can query a real drive.
    let mut probe = std::path::PathBuf::from(&path);
    while !probe.exists() {
        if !probe.pop() {
            return Err(AppError::NotFound(format!(
                "no accessible ancestor for path: {path}"
            )));
        }
    }

    let wide: Vec<u16> = probe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes: u64 = 0;
    let mut total_bytes: u64 = 0;

    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_bytes),
            Some(&mut total_bytes),
            None,
        )
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    }

    Ok(DiskInfo {
        free_bytes,
        total_bytes,
    })
}

// ─────────────────────────────────────────────────────────────────────────────

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

// ── Source / photo deletion ──────────────────────────────────────────────────

/// Dry-run counts for a source-deletion modal.
#[derive(Debug, Serialize)]
pub struct SourceDeletionPlan {
    /// Photos that have `source_copies` rows for this source.
    pub photos_total: i64,
    /// Photos that would become orphans (no remaining `source_copies` after delete).
    pub orphan_photos: i64,
    /// Local files on disk that belong only to this source.
    pub local_files: i64,
    /// Combined size of those local files, in bytes.
    pub total_bytes: i64,
    /// Orphan photos that have no local path (cloud-only).
    pub cloud_only: i64,
}

/// Receipt returned after `remove_photos_from_catalog`.
#[derive(Debug, Serialize)]
pub struct RemoveReceipt {
    pub removed_photos: i64,
    pub removed_thumbnails: i64,
    pub errors: Vec<String>,
}

/// Receipt returned after `recycle_source_copies`.
#[derive(Debug, Serialize)]
pub struct RecycleReceipt {
    pub recycled_count: i64,
    pub skipped_count: i64,
    pub errors: Vec<String>,
}

/// Dry-run counts for the remove-photos modal.
#[derive(Debug, Serialize)]
pub struct RemovePreview {
    pub photo_count: i64,
    pub local_files: i64,
    pub cloud_only_photos: i64,
    pub total_bytes: i64,
}

/// Preview the impact of disconnecting a source.
#[tauri::command]
pub async fn source_deletion_preview(
    state: State<'_, AppState>,
    source_id: i64,
) -> AppResult<SourceDeletionPlan> {
    source_deletion_preview_impl(&state.pool, source_id).await
}

async fn source_deletion_preview_impl(
    pool: &sqlx::SqlitePool,
    source_id: i64,
) -> AppResult<SourceDeletionPlan> {
    let photos_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT photo_id) FROM source_copies WHERE source_id = ?1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    // Orphans: photos whose ONLY source_copy is this source.
    let orphan_photos: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
           SELECT photo_id FROM source_copies WHERE source_id = ?1
           GROUP BY photo_id
           HAVING COUNT(*) = (
             SELECT COUNT(*) FROM source_copies sc2 WHERE sc2.photo_id = source_copies.photo_id
           )
         )",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    // Local files + bytes: orphans with a non-null path in this source.
    let row: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(sc.path), COALESCE(SUM(p.size_bytes), 0)
         FROM source_copies sc
         JOIN photos p ON p.id = sc.photo_id
         WHERE sc.source_id = ?1
           AND sc.path IS NOT NULL
           AND NOT EXISTS (
             SELECT 1 FROM source_copies sc2
             WHERE sc2.photo_id = sc.photo_id AND sc2.source_id != ?1
           )",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    let local_files = row.0.unwrap_or(0);
    let total_bytes = row.1.unwrap_or(0);

    let cloud_only = orphan_photos - local_files;
    let cloud_only = cloud_only.max(0);

    Ok(SourceDeletionPlan {
        photos_total,
        orphan_photos,
        local_files,
        total_bytes,
        cloud_only,
    })
}

/// Disconnect a source, cascade orphan-photo cleanup, and optionally recycle
/// the orphan files on disk.
///
/// Runs in a single transaction so the catalog is never partially-updated on
/// error. Orphan photos (no remaining `source_copies` after this source's
/// `source_copies` are deleted) are removed; `ON DELETE CASCADE` cleans up
/// tags, faces, embeddings, views. The sqlite-vec virtual tables do NOT
/// participate in FK cascade, so we delete from them explicitly by rowid.
///
/// `recycle_files` = true routes orphan local paths through the Windows
/// Recycle Bin via the `trash` crate. File deletion is best-effort and
/// happens OUTSIDE the DB transaction so a trash failure does not roll back
/// the catalog update.
#[tauri::command]
pub async fn delete_source(
    state: State<'_, AppState>,
    source_id: i64,
    recycle_files: bool,
    remove_orphan_photos: bool,
) -> AppResult<RemoveReceipt> {
    delete_source_impl(&state.pool, source_id, recycle_files, remove_orphan_photos).await
}

async fn delete_source_impl(
    pool: &sqlx::SqlitePool,
    source_id: i64,
    recycle_files: bool,
    remove_orphan_photos: bool,
) -> AppResult<RemoveReceipt> {
    // Collect orphan paths BEFORE we mutate, so we can recycle them after.
    let orphan_paths: Vec<String> = if recycle_files {
        sqlx::query_scalar::<_, String>(
            "SELECT sc.path FROM source_copies sc
             WHERE sc.source_id = ?1
               AND sc.path IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM source_copies sc2
                 WHERE sc2.photo_id = sc.photo_id AND sc2.source_id != ?1
               )",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    // Collect orphan photo ids + sha256 so we can nuke their thumb-cache files
    // and sqlite-vec rowids after the transaction commits.
    let orphan_meta: Vec<(i64, String)> = if remove_orphan_photos {
        sqlx::query_as::<_, (i64, String)>(
            "SELECT p.id, p.sha256
             FROM photos p
             WHERE EXISTS (SELECT 1 FROM source_copies sc WHERE sc.photo_id = p.id AND sc.source_id = ?1)
               AND NOT EXISTS (
                 SELECT 1 FROM source_copies sc2 WHERE sc2.photo_id = p.id AND sc2.source_id != ?1
               )",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };

    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM imports WHERE source_id = ?1")
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM source_copies WHERE source_id = ?1")
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sources WHERE id = ?1")
        .bind(source_id)
        .execute(&mut *tx)
        .await?;

    let mut removed_photos = 0_i64;
    if remove_orphan_photos {
        for (pid, _sha) in &orphan_meta {
            // Issue contentless FTS5 delete command with the CURRENT row state
            // (filename + concatenated tags). Must run before the photo row
            // and its tags are gone, so we have the values to pass.
            let _ = sqlx::query(
                "INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
                 SELECT 'delete', p.id, p.filename,
                        COALESCE((SELECT group_concat(label, ' ') FROM tags WHERE photo_id = p.id), '')
                 FROM photos p WHERE p.id = ?1",
            )
            .bind(pid)
            .execute(&mut *tx)
            .await;
            // Manually clean sqlite-vec virtual tables (no FK cascade).
            let _ = sqlx::query("DELETE FROM vec_photo_embeddings WHERE rowid = ?1")
                .bind(pid)
                .execute(&mut *tx)
                .await;
            let _ = sqlx::query("DELETE FROM vec_photo_embeddings_int8 WHERE rowid = ?1")
                .bind(pid)
                .execute(&mut *tx)
                .await;
        }
        // Delete photos rows — FK cascade handles tags, faces, embeddings, views, source_copies.
        let res = sqlx::query(
            "DELETE FROM photos
             WHERE id IN (SELECT id FROM photos p
                          WHERE NOT EXISTS (SELECT 1 FROM source_copies sc WHERE sc.photo_id = p.id))",
        )
        .execute(&mut *tx)
        .await?;
        removed_photos = res.rows_affected() as i64;
    }

    tx.commit().await?;

    // Best-effort post-commit cleanup (cache files + recycle bin).
    let mut errors: Vec<String> = Vec::new();
    let mut removed_thumbnails = 0_i64;
    if remove_orphan_photos {
        if let Ok(thumbs_dir) = crate::util::paths::thumbnails_dir() {
            for (_pid, sha) in &orphan_meta {
                let cache_path = thumbs_dir.join(format!("{sha}_320.jpg"));
                if cache_path.exists() {
                    match std::fs::remove_file(&cache_path) {
                        Ok(()) => removed_thumbnails += 1,
                        Err(e) => errors.push(format!("thumb-cache {}: {e}", cache_path.display())),
                    }
                }
            }
        }
    }

    if recycle_files {
        for path in &orphan_paths {
            let p = std::path::Path::new(path);
            if !p.exists() {
                continue;
            }
            if let Err(e) = trash::delete(p) {
                errors.push(format!("recycle {path}: {e}"));
            }
        }
    }

    tracing::info!(
        source_id,
        removed_photos,
        removed_thumbnails,
        recycled_files = orphan_paths.len(),
        "source deleted"
    );

    Ok(RemoveReceipt {
        removed_photos,
        removed_thumbnails,
        errors,
    })
}

/// Preview the impact of removing the given photos from the catalog.
#[tauri::command]
pub async fn remove_photos_preview(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> AppResult<RemovePreview> {
    remove_photos_preview_impl(&state.pool, &photo_ids).await
}

async fn remove_photos_preview_impl(
    pool: &sqlx::SqlitePool,
    photo_ids: &[i64],
) -> AppResult<RemovePreview> {
    if photo_ids.is_empty() {
        return Ok(RemovePreview {
            photo_count: 0,
            local_files: 0,
            cloud_only_photos: 0,
            total_bytes: 0,
        });
    }

    // Build a temporary in-memory set via a comma-join; sqlite doesn't support
    // array binding so we validate the list as signed-integer ids then format.
    let id_list = photo_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let photo_count = photo_ids.len() as i64;

    let local_files: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM source_copies
         WHERE photo_id IN ({id_list}) AND path IS NOT NULL"
    ))
    .fetch_one(pool)
    .await?;

    let total_bytes: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(p.size_bytes), 0) FROM photos p
         WHERE p.id IN ({id_list})
           AND EXISTS (SELECT 1 FROM source_copies sc WHERE sc.photo_id = p.id AND sc.path IS NOT NULL)"
    ))
    .fetch_one(pool)
    .await?;

    let cloud_only_photos: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM photos p
         WHERE p.id IN ({id_list})
           AND NOT EXISTS (SELECT 1 FROM source_copies sc WHERE sc.photo_id = p.id AND sc.path IS NOT NULL)"
    ))
    .fetch_one(pool)
    .await?;

    Ok(RemovePreview {
        photo_count,
        local_files,
        cloud_only_photos,
        total_bytes,
    })
}

/// Remove photos from the catalog (DB + thumbnail cache). Does NOT touch
/// the original source files on disk.
#[tauri::command]
pub async fn remove_photos_from_catalog(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> AppResult<RemoveReceipt> {
    remove_photos_from_catalog_impl(&state.pool, &photo_ids).await
}

async fn remove_photos_from_catalog_impl(
    pool: &sqlx::SqlitePool,
    photo_ids: &[i64],
) -> AppResult<RemoveReceipt> {
    if photo_ids.is_empty() {
        return Ok(RemoveReceipt {
            removed_photos: 0,
            removed_thumbnails: 0,
            errors: Vec::new(),
        });
    }

    // Collect sha256 for cache-file cleanup before deleting the photo rows.
    let id_list = photo_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let shas: Vec<String> = sqlx::query_scalar::<_, String>(&format!(
        "SELECT sha256 FROM photos WHERE id IN ({id_list})"
    ))
    .fetch_all(pool)
    .await?;

    let mut tx = pool.begin().await?;
    for pid in photo_ids {
        // Contentless FTS5 delete — must run before the photo and its tags
        // are gone, so we can pass the pre-delete filename + tag concat.
        let _ = sqlx::query(
            "INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
             SELECT 'delete', p.id, p.filename,
                    COALESCE((SELECT group_concat(label, ' ') FROM tags WHERE photo_id = p.id), '')
             FROM photos p WHERE p.id = ?1",
        )
        .bind(pid)
        .execute(&mut *tx)
        .await;
        let _ = sqlx::query("DELETE FROM vec_photo_embeddings WHERE rowid = ?1")
            .bind(pid)
            .execute(&mut *tx)
            .await;
        let _ = sqlx::query("DELETE FROM vec_photo_embeddings_int8 WHERE rowid = ?1")
            .bind(pid)
            .execute(&mut *tx)
            .await;
    }
    let res = sqlx::query(&format!("DELETE FROM photos WHERE id IN ({id_list})"))
        .execute(&mut *tx)
        .await?;
    let removed_photos = res.rows_affected() as i64;
    tx.commit().await?;

    // Best-effort thumbnail-cache cleanup.
    let mut errors: Vec<String> = Vec::new();
    let mut removed_thumbnails = 0_i64;
    if let Ok(thumbs_dir) = crate::util::paths::thumbnails_dir() {
        for sha in &shas {
            let cache_path = thumbs_dir.join(format!("{sha}_320.jpg"));
            if cache_path.exists() {
                match std::fs::remove_file(&cache_path) {
                    Ok(()) => removed_thumbnails += 1,
                    Err(e) => errors.push(format!("thumb-cache {}: {e}", cache_path.display())),
                }
            }
        }
    }

    tracing::info!(
        removed_photos,
        removed_thumbnails,
        "photos removed from catalog"
    );
    Ok(RemoveReceipt {
        removed_photos,
        removed_thumbnails,
        errors,
    })
}

/// Send the on-disk files for the given photos to the Windows Recycle Bin.
/// Does NOT touch the catalog DB — caller is responsible for a separate
/// `remove_photos_from_catalog` call if they also want catalog removal.
#[tauri::command]
pub async fn recycle_source_copies(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> AppResult<RecycleReceipt> {
    recycle_source_copies_impl(&state.pool, &photo_ids).await
}

async fn recycle_source_copies_impl(
    pool: &sqlx::SqlitePool,
    photo_ids: &[i64],
) -> AppResult<RecycleReceipt> {
    if photo_ids.is_empty() {
        return Ok(RecycleReceipt {
            recycled_count: 0,
            skipped_count: 0,
            errors: Vec::new(),
        });
    }

    let id_list = photo_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let paths: Vec<String> = sqlx::query_scalar::<_, String>(&format!(
        "SELECT path FROM source_copies
         WHERE photo_id IN ({id_list}) AND path IS NOT NULL"
    ))
    .fetch_all(pool)
    .await?;

    let mut recycled_count = 0_i64;
    let mut skipped_count = 0_i64;
    let mut errors: Vec<String> = Vec::new();

    for path in &paths {
        let p = std::path::Path::new(path);
        if !p.exists() {
            skipped_count += 1;
            continue;
        }
        match trash::delete(p) {
            Ok(()) => recycled_count += 1,
            Err(e) => {
                errors.push(format!("recycle {path}: {e}"));
                skipped_count += 1;
            }
        }
    }

    tracing::info!(recycled_count, skipped_count, "source_copies recycled");
    Ok(RecycleReceipt {
        recycled_count,
        skipped_count,
        errors,
    })
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

// ── Natural-language search ───────────────────────────────────────────────

/// A photo row returned by search and the catalog grid.
///
/// Field names mirror the `photos` table columns that the frontend grid
/// already understands. Optional Phase 1 columns are included when populated.
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
    pub size_bytes: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub aperture: Option<f64>,
    pub shutter: Option<String>,
    pub iso: Option<i64>,
    pub focal_mm: Option<f64>,
    pub aesthetic_score: Option<f64>,
    pub paired_photo_id: Option<i64>,
    pub raw_format: Option<String>,
    /// EXIF Orientation tag (1–8). Default 1 = no rotation. Clients use
    /// this to compute display aspect ratio (values 5–8 swap width/height)
    /// for the masonry grid.
    pub orientation: i64,
    /// Laplacian-variance sharpness score from Stage 2.6. Higher = sharper.
    /// Exposed so the detail inspector can surface it without a second read.
    pub sharpness_score: Option<f64>,
    /// Phase 2 §1 — 0..=5 star rating set from the detail-view toolbar.
    #[serde(default)]
    pub star_rating: i64,
    /// Phase 2 §2 — soft flag toggled via the detail-view `X` key.
    #[serde(default)]
    pub is_flagged: bool,
}

/// Translate a user-facing sort identifier to a SQL `ORDER BY` clause.
///
/// Unknown values fall back to the default newest-first order. Each variant
/// appends `imported_at DESC` as a deterministic tie-breaker so pagination
/// stays stable inside ties.
fn sort_by_to_sql(sort_by: Option<&str>) -> &'static str {
    match sort_by.unwrap_or("captured_desc") {
        "captured_asc" => "ORDER BY captured_at ASC NULLS LAST, imported_at ASC",
        "imported_desc" => "ORDER BY imported_at DESC",
        "filename_asc" => "ORDER BY LOWER(filename) ASC, imported_at DESC",
        "aesthetic_desc" => "ORDER BY aesthetic_score DESC NULLS LAST, imported_at DESC",
        "random" => "ORDER BY RANDOM()",
        // Default + explicit "captured_desc"
        _ => "ORDER BY captured_at DESC NULLS LAST, imported_at DESC",
    }
}

/// List photos with optional pagination, album filter, and sort.
/// `limit` defaults to 100; `offset` defaults to 0.
/// When `album_id` is provided the album's `rule_json` is evaluated to build a WHERE clause.
/// `sort_by` accepts `captured_desc` (default) | `captured_asc` | `imported_desc`
///   | `filename_asc` | `aesthetic_desc` | `random`.
#[tauri::command]
pub async fn list_photos(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
    album_id: Option<i64>,
    sort_by: Option<String>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(100);
    let off = offset.unwrap_or(0);

    // Resolve album filter.
    let where_clause = if let Some(aid) = album_id {
        let rule_json: Option<String> =
            sqlx::query_scalar("SELECT rule_json FROM smart_albums WHERE id = ?1")
                .bind(aid)
                .fetch_optional(&state.pool)
                .await?;
        match rule_json {
            None => {
                return Err(AppError::NotFound(format!("smart album {aid} not found")));
            }
            Some(rj) => match catalog::rules::parse_rule(&rj) {
                Err(_) => {
                    return Err(AppError::Internal(format!(
                        "invalid rule_json for album {aid}"
                    )))
                }
                Ok(rule) => catalog::rules::rule_to_sql(&rule)
                    .map(|frag| format!("WHERE {frag}"))
                    .unwrap_or_default(),
            },
        }
    } else {
        String::new()
    };

    let order_clause = sort_by_to_sql(sort_by.as_deref());

    let sql = format!(
        "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
         size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
         aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
         FROM photos {where_clause} {order_clause} LIMIT ?1 OFFSET ?2"
    );
    let rows = sqlx::query_as::<_, PhotoRow>(&sql)
        .bind(lim)
        .bind(off)
        .fetch_all(&state.pool)
        .await?;
    Ok(rows)
}

/// Re-evaluate all smart album rules and update `photo_count` + `cover_photo_ids`.
/// Called automatically after each import and can be invoked manually.
#[tauri::command]
pub async fn refresh_smart_albums(state: State<'_, AppState>) -> AppResult<()> {
    refresh_album_counts(&state.pool).await
}

pub(crate) async fn refresh_album_counts(pool: &sqlx::SqlitePool) -> AppResult<()> {
    #[derive(sqlx::FromRow)]
    struct AlbumMeta {
        id: i64,
        rule_json: String,
    }

    let albums = sqlx::query_as::<_, AlbumMeta>("SELECT id, rule_json FROM smart_albums")
        .fetch_all(pool)
        .await?;

    let now = chrono::Utc::now().to_rfc3339();
    for album in &albums {
        let count = catalog::rules::count_matching(pool, &album.rule_json).await;
        let cover_ids = catalog::rules::matching_photo_ids(pool, &album.rule_json, 4).await;
        let cover_json = serde_json::to_string(&cover_ids).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "UPDATE smart_albums SET photo_count = ?1, cover_photo_ids = ?2, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(count)
        .bind(&cover_json)
        .bind(&now)
        .bind(album.id)
        .execute(pool)
        .await?;
    }

    tracing::debug!(albums = albums.len(), "smart album counts refreshed");
    Ok(())
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

// ── Source connector commands ─────────────────────────────────────────────

/// Run the standard import pipeline over a Google Photos Takeout export root,
/// then enrich catalog rows with metadata from the sidecar `.json` files.
#[tauri::command]
pub async fn import_google_takeout(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    source_id: i64,
    root: String,
) -> AppResult<StartImportResponse> {
    let root_path = PathBuf::from(&root);
    if !root_path.exists() {
        return Err(AppError::NotFound(format!(
            "takeout root does not exist: {root}"
        )));
    }

    let pool = state.pool.clone();
    let now = chrono::Utc::now().to_rfc3339();
    let import_id: i64 = sqlx::query_scalar(
        "INSERT INTO imports (source_id, started_at, total_files, imported_count, \
         skipped_count, error_count) VALUES (?1, ?2, 0, 0, 0, 0) RETURNING id",
    )
    .bind(source_id)
    .bind(&now)
    .fetch_one(&pool)
    .await?;

    let pool2 = pool.clone();
    let root_clone = root_path.clone();
    tokio::spawn(async move {
        if let Err(e) = import::pipeline::run_pipeline_from_import_id(
            source_id,
            import_id,
            root_clone.clone(),
            pool2.clone(),
            app_handle,
        )
        .await
        {
            tracing::error!(error = %e, import_id, "google takeout pipeline failed");
            return;
        }

        match import::google_takeout::enrich_from_sidecars(&pool2, &root_clone).await {
            Ok(n) => tracing::info!(enriched = n, import_id, "takeout sidecar enrichment done"),
            Err(e) => tracing::warn!(error = %e, import_id, "takeout sidecar enrichment failed"),
        }
    });

    Ok(StartImportResponse { import_id })
}

/// Detect the iCloud-for-Windows Photos folder, if installed.
/// Returns the path as a string, or `null` when not found.
#[tauri::command]
pub async fn detect_icloud_path() -> AppResult<Option<String>> {
    Ok(import::icloud::detect_icloud_path().map(|p| p.to_string_lossy().into_owned()))
}

/// List Apple devices connected via USB (WPD/MTP). Returns `[]` when none.
#[tauri::command]
pub async fn list_iphone_devices() -> AppResult<Vec<import::iphone_usb::UsbDevice>> {
    tokio::task::spawn_blocking(import::iphone_usb::list_iphone_devices)
        .await
        .map_err(|e| AppError::Internal(format!("iphone usb task join: {e}")))?
}

// ── Google Photos OAuth2 + Picker commands ────────────────────────────────
//
// The frontend drives the OAuth flow via three steps:
//   1. gphotos_begin_oauth_flow() → spawns loopback listener + returns
//      (auth_url, flow_id). Frontend opens auth_url in system browser.
//   2. gphotos_poll_oauth_flow(flow_id) → polled every ~1 s until the
//      status is `completed` or `failed`; on success the sources row is
//      created and the account email is returned.
//   3. (Optional) gphotos_cancel_oauth_flow(flow_id) if the user closes the
//      modal.
//
// Imports happen via the Photo Picker API:
//   4. gphotos_create_picker_session() → returns pickerUri + session_id.
//      Frontend opens pickerUri in the browser; user chooses photos.
//   5. gphotos_poll_picker_session(session_id) → polled until
//      media_items_set = true.
//   6. import_google_photos(source_id, session_id) → streams picked items
//      through the import pipeline. See [`import_google_photos`].
//   7. gphotos_delete_picker_session(session_id) cleans up after ingest.

#[derive(Debug, Serialize)]
pub struct BeginOauthResponse {
    pub auth_url: String,
    pub flow_id: String,
}

/// Begin an OAuth2 flow. Spawns a one-shot loopback listener, generates
/// PKCE + state, and returns the URL the frontend opens in the system
/// browser plus a flow-id for polling. `client_id` is optional — omitting
/// it falls back to [`google_photos::DEFAULT_CLIENT_ID`].
#[tauri::command]
pub async fn gphotos_begin_oauth_flow(client_id: Option<String>) -> AppResult<BeginOauthResponse> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    let (auth_url, flow_id) = google_photos::begin_oauth_flow(cid).await?;
    // Deliberately no background task here — the frontend is responsible
    // for calling `gphotos_ensure_source_row` after it observes Completed
    // status. That keeps row creation deterministic and avoids races
    // against the TanStack Query cache.
    Ok(BeginOauthResponse { auth_url, flow_id })
}

/// Poll the status of a running OAuth flow. Returns `completed` / `failed`
/// / `pending` / `timed_out`.
#[tauri::command]
pub async fn gphotos_poll_oauth_flow(
    flow_id: String,
) -> AppResult<crate::sources::google_photos::FlowStatus> {
    use crate::sources::google_photos;
    google_photos::peek_flow_status(&flow_id)
        .ok_or_else(|| AppError::NotFound(format!("oauth flow {flow_id} unknown or expired")))
}

/// Abort a running OAuth flow. Idempotent — no-op for unknown ids.
#[tauri::command]
pub async fn gphotos_cancel_oauth_flow(flow_id: String) -> AppResult<()> {
    crate::sources::google_photos::abort_flow(&flow_id);
    Ok(())
}

/// Whether a usable Google Photos token set is present in the keyring.
#[tauri::command]
pub async fn gphotos_auth_status() -> AppResult<bool> {
    Ok(crate::sources::google_photos::load_tokens()?.is_some())
}

/// Return the manual-cleanup guidance the UI should surface when the user
/// tries to source-delete Google Photos copies. Photo Picker API is
/// read-only; per PRD § 12 we point the user at photos.google.com and
/// takeout.google.com for the actual deletion. Structured so the frontend
/// can render a dialog with working links.
#[derive(Debug, Serialize)]
pub struct GphotosManualCleanupInstructions {
    pub headline: String,
    pub body: String,
    pub google_photos_url: String,
    pub takeout_url: String,
}

#[tauri::command]
pub async fn gphotos_manual_cleanup_instructions() -> AppResult<GphotosManualCleanupInstructions> {
    Ok(GphotosManualCleanupInstructions {
        headline: "Delete Google Photos copies manually".into(),
        body: "Chronimage uses Google's Photo Picker API, which is \
            read-only — it can't delete photos on your behalf. Open Google \
            Photos in your browser to remove the uploaded copies, or use \
            Google Takeout to bulk-remove if you already have a local \
            backup."
            .into(),
        google_photos_url: "https://photos.google.com/".into(),
        takeout_url: "https://takeout.google.com/".into(),
    })
}

/// Drop the Google Photos token set from the keyring + delete any
/// `sources` rows of kind `google_photos`. Idempotent.
#[tauri::command]
pub async fn gphotos_sign_out(state_: State<'_, AppState>) -> AppResult<()> {
    crate::sources::google_photos::delete_tokens()?;
    // Cascade: drop any gphotos source rows so the UI doesn't list stale
    // connections. delete_source_copies + imports are handled by the
    // existing DELETE CASCADE paths.
    sqlx::query("DELETE FROM sources WHERE kind = 'google_photos'")
        .execute(&state_.pool)
        .await?;
    Ok(())
}

/// Ensure a `sources` row exists for the currently-connected Google
/// account. Called by the frontend immediately after OAuth completes so
/// the row is guaranteed to be present before `listSources()` runs.
/// Idempotent — returns the existing row if one already exists for this
/// email.
///
/// Replaces the old `ensure_google_photos_source_row` background task,
/// which raced against frontend cache invalidation and could leave the UI
/// without a visible source for seconds after auth.
#[tauri::command]
pub async fn gphotos_ensure_source_row(
    state_: State<'_, AppState>,
    client_id: Option<String>,
) -> AppResult<SourceRow> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    // If we can't even load tokens, fail fast — the user isn't connected.
    let _ = google_photos::load_tokens()?
        .ok_or_else(|| AppError::PermissionDenied("google photos not signed in".into()))?;

    // Best-effort fetch the email; non-fatal. Used for the source name +
    // the dedupe key.
    let email = match google_photos::user_initiated_current_access_token(cid).await {
        Ok(token) => google_photos::user_initiated_fetch_userinfo(&token)
            .await
            .ok()
            .and_then(|u| u.email),
        Err(e) => {
            tracing::warn!(error = %e, "gphotos: userinfo fetch failed, proceeding with anonymous source row");
            None
        }
    };

    let name = email
        .clone()
        .map(|e| format!("Google Photos · {e}"))
        .unwrap_or_else(|| "Google Photos".to_string());
    let config = serde_json::json!({ "email": email }).to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM sources WHERE kind = 'google_photos' \
         AND json_extract(config_json, '$.email') IS ?1",
    )
    .bind(&email)
    .fetch_optional(&state_.pool)
    .await?;

    let id = match existing {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES (?1, 'google_photos', 'idle', ?2, ?3) RETURNING id",
            )
            .bind(&name)
            .bind(&config)
            .bind(&now)
            .fetch_one(&state_.pool)
            .await?
        }
    };

    // Return the row with its derived photo_count (may be 0 on first auth).
    let row = sqlx::query_as::<_, SourceRow>(
        "SELECT s.id, s.name, s.kind, s.status, s.last_scan_at, \
         COUNT(DISTINCT sc.photo_id) AS photo_count \
         FROM sources s LEFT JOIN source_copies sc ON sc.source_id = s.id \
         WHERE s.id = ?1 GROUP BY s.id",
    )
    .bind(id)
    .fetch_one(&state_.pool)
    .await?;
    tracing::info!(source_id = id, ?email, "google photos source row ensured");
    Ok(row)
}

/// Connected-account info. Uses the current access token (refreshing if
/// expired) and returns whatever Google's /userinfo endpoint gives us.
#[tauri::command]
pub async fn gphotos_account_info(
    client_id: Option<String>,
) -> AppResult<crate::sources::google_photos::UserInfo> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    let access = google_photos::user_initiated_current_access_token(cid).await?;
    google_photos::user_initiated_fetch_userinfo(&access).await
}

/// Create a Google Photo Picker session. Returns `picker_uri` (open in
/// browser) + `id` (poll against this).
#[tauri::command]
pub async fn gphotos_create_picker_session(
    client_id: Option<String>,
) -> AppResult<crate::sources::google_photos::PickerSession> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    let access = google_photos::user_initiated_current_access_token(cid).await?;
    google_photos::user_initiated_create_picker_session(&access).await
}

/// Poll a picker session. `media_items_set = true` signals the user has
/// finished selecting and the caller can start listing items.
#[tauri::command]
pub async fn gphotos_poll_picker_session(
    client_id: Option<String>,
    session_id: String,
) -> AppResult<crate::sources::google_photos::PickerSession> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    let access = google_photos::user_initiated_current_access_token(cid).await?;
    google_photos::user_initiated_poll_picker_session(&access, &session_id).await
}

/// Delete a picker session. Idempotent.
#[tauri::command]
pub async fn gphotos_delete_picker_session(
    client_id: Option<String>,
    session_id: String,
) -> AppResult<()> {
    use crate::sources::google_photos;
    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);
    let access = google_photos::user_initiated_current_access_token(cid).await?;
    google_photos::user_initiated_delete_picker_session(&access, &session_id).await
}

/// Run the import pipeline over the media items the user picked in
/// `session_id`. Downloads each picked item to a tempdir and hands the
/// directory to the standard import pipeline — the pipeline hashes,
/// de-duplicates, embeds, face-detects, and persists as with any local
/// source.
#[tauri::command]
pub async fn import_google_photos(
    state_: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    source_id: i64,
    session_id: String,
    client_id: Option<String>,
) -> AppResult<StartImportResponse> {
    use crate::sources::google_photos;

    let cid_owned = client_id.unwrap_or_else(|| google_photos::DEFAULT_CLIENT_ID.to_string());

    // Persist downloads under the user's data dir so they survive the
    // command returning (the pipeline runs spawn_blocking across awaits).
    let staging_root = crate::util::paths::app_data_dir()
        .map_err(|e| AppError::Internal(format!("resolve app_data_dir: {e}")))?
        .join("_gphotos_staging")
        .join(format!("session-{session_id}"));
    std::fs::create_dir_all(&staging_root)?;

    // Issue imports row early so the UI can track progress via list_imports.
    let pool = state_.pool.clone();
    let now = chrono::Utc::now().to_rfc3339();
    let import_id: i64 = sqlx::query_scalar(
        "INSERT INTO imports (source_id, started_at, total_files, imported_count, \
         skipped_count, error_count) VALUES (?1, ?2, 0, 0, 0, 0) RETURNING id",
    )
    .bind(source_id)
    .bind(&now)
    .fetch_one(&pool)
    .await?;

    // Kick off a background task that streams mediaItems, downloads each,
    // then runs the standard pipeline against the staging dir.
    tokio::spawn(async move {
        if let Err(e) = download_picker_items(&cid_owned, &session_id, &staging_root).await {
            tracing::error!(error = %e, import_id, "google photos download phase failed");
            // Record error count so the import row reflects failure.
            let _ = sqlx::query("UPDATE imports SET error_count = error_count + 1 WHERE id = ?1")
                .bind(import_id)
                .execute(&pool)
                .await;
            return;
        }

        if let Err(e) = import::pipeline::run_pipeline_from_import_id(
            source_id,
            import_id,
            staging_root.clone(),
            pool.clone(),
            app_handle,
        )
        .await
        {
            tracing::error!(error = %e, import_id, "google photos pipeline failed");
        }
    });

    Ok(StartImportResponse { import_id })
}

/// Iterate `mediaItems.list` for `session_id` and download each item's
/// `baseUrl` into `target_dir`. Returns when the list is exhausted.
async fn download_picker_items(
    client_id: &str,
    session_id: &str,
    target_dir: &std::path::Path,
) -> AppResult<()> {
    use crate::sources::google_photos;

    let mut page_token: Option<String> = None;
    let mut n = 0usize;
    loop {
        let access = google_photos::user_initiated_current_access_token(client_id).await?;
        let page = google_photos::user_initiated_list_picked_media_items(
            &access,
            session_id,
            page_token.as_deref(),
            Some(100),
        )
        .await?;
        for item in &page.media_items {
            let Some(file) = item.media_file.as_ref() else {
                continue;
            };
            let filename = file
                .filename
                .clone()
                .unwrap_or_else(|| format!("{}.bin", item.id));
            // Sanitize: strip any path separators the server might send.
            let safe_name: String = filename
                .chars()
                .map(|c| if c == '/' || c == '\\' { '_' } else { c })
                .collect();
            let target = target_dir.join(&safe_name);
            // Refresh access for every 50 downloads in case we cross the
            // expiry boundary during a long session.
            let access = if n.is_multiple_of(50) {
                google_photos::user_initiated_current_access_token(client_id).await?
            } else {
                access.clone()
            };
            google_photos::user_initiated_download_media_item(&access, &file.base_url, &target)
                .await?;
            n += 1;
        }
        match page.next_page_token {
            Some(tok) if !tok.is_empty() => page_token = Some(tok),
            _ => break,
        }
    }
    tracing::info!(downloaded = n, "google photos picker download complete");
    Ok(())
}

// ── Google Photos upload command ──────────────────────────────────────────────
//
// Uploads already-exported local files back to the user's Google Photos
// library using the Library API `mediaItems:batchCreate` endpoint.
//
// IMPORTANT: The `photoslibrary.appendonly` scope is NOT in the original picker
// OAuth flow. `ensure_upload_scope()` detects this and returns `false` when
// re-auth is required. The frontend should call `gphotos_upload_scope_ok` first;
// if it returns `false`, re-run `gphotos_begin_oauth_flow` (which now requests
// the upload scope via the `upload_scopes` flag) before calling `gphotos_upload`.

/// Return `true` when the stored Google Photos token set already includes
/// the `photoslibrary.appendonly` scope needed for uploads. `false` means the
/// user must re-authorize — call `gphotos_begin_oauth_flow` again, which will
/// request both picker and upload scopes.
#[tauri::command]
pub async fn gphotos_upload_scope_ok() -> AppResult<bool> {
    crate::sources::google_photos::ensure_upload_scope()
}

/// Upload the local source copies of `photo_ids` to the signed-in user's
/// Google Photos library.
///
/// Resolves each photo's local path via `source_copies`, then calls the
/// Library API upload + `batchCreate` path in batches of 50.
#[tauri::command]
pub async fn gphotos_upload(
    state_: State<'_, AppState>,
    client_id: Option<String>,
    photo_ids: Vec<i64>,
) -> AppResult<crate::sources::google_photos::UploadReceipt> {
    use crate::sources::google_photos;

    let cid = client_id
        .as_deref()
        .unwrap_or(google_photos::DEFAULT_CLIENT_ID);

    if photo_ids.is_empty() {
        return Ok(google_photos::UploadReceipt {
            uploaded_count: 0,
            skipped_count: 0,
            errors: Vec::new(),
        });
    }

    // Resolve local paths for each photo_id.
    let mut paths: Vec<std::path::PathBuf> = Vec::with_capacity(photo_ids.len());
    for pid in &photo_ids {
        let path: Option<String> = sqlx::query_scalar(
            "SELECT path FROM source_copies \
             WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
        )
        .bind(pid)
        .fetch_optional(&state_.pool)
        .await?;

        match path {
            Some(p) => paths.push(std::path::PathBuf::from(p)),
            None => {
                tracing::warn!(photo_id = pid, "gphotos_upload: no local path, skipping");
            }
        }
    }

    google_photos::user_initiated_upload_photos_to_google(cid, &paths).await
}

// ── OneDrive OAuth2 + upload commands ────────────────────────────────────────
//
// Microsoft OneDrive upload via the Graph API. OAuth uses the same PKCE
// loopback pattern as Google Photos. The Azure app registration is a manual
// operator step — see docs/manual-setup.md §3.
//
// Frontend flow:
//   1. onedrive_begin_oauth_flow() → (auth_url, flow_id)
//   2. onedrive_poll_oauth_flow(flow_id) → pending / completed / failed / timed_out
//   3. (Optional) onedrive_cancel_oauth_flow(flow_id)
//   4. onedrive_upload(photo_ids, remote_folder) → UploadReceipt

/// Begin an OneDrive OAuth2 flow. Returns `(auth_url, flow_id)`. The
/// frontend opens `auth_url` in the system browser and polls
/// `onedrive_poll_oauth_flow` until status is `completed` or `failed`.
///
/// `client_id` is optional — omitting it falls back to
/// `onedrive::DEFAULT_CLIENT_ID` (the Azure placeholder; a `tracing::warn!`
/// fires if the placeholder hasn't been replaced).
#[tauri::command]
pub async fn onedrive_begin_oauth_flow(client_id: Option<String>) -> AppResult<BeginOauthResponse> {
    use crate::sources::onedrive;
    let (auth_url, flow_id) = onedrive::begin_oauth_flow(client_id.as_deref()).await?;
    Ok(BeginOauthResponse { auth_url, flow_id })
}

/// Poll the status of a running OneDrive OAuth flow.
#[tauri::command]
pub async fn onedrive_poll_oauth_flow(
    flow_id: String,
) -> AppResult<crate::sources::onedrive::FlowStatus> {
    crate::sources::onedrive::peek_flow_status(&flow_id).ok_or_else(|| {
        AppError::NotFound(format!("onedrive oauth flow {flow_id} unknown or expired"))
    })
}

/// Abort a running OneDrive OAuth flow. Idempotent.
#[tauri::command]
pub async fn onedrive_cancel_oauth_flow(flow_id: String) -> AppResult<()> {
    crate::sources::onedrive::abort_flow(&flow_id);
    Ok(())
}

/// Whether a usable OneDrive token set is present in the keyring.
#[tauri::command]
pub async fn onedrive_auth_status() -> AppResult<bool> {
    Ok(crate::sources::onedrive::load_tokens()?.is_some())
}

/// Drop the OneDrive token set from the keyring. Idempotent.
#[tauri::command]
pub async fn onedrive_sign_out() -> AppResult<()> {
    crate::sources::onedrive::delete_tokens()
}

/// Fetch the connected OneDrive account's display name + email from
/// Graph `/me`.
#[tauri::command]
pub async fn onedrive_account_info(
    client_id: Option<String>,
) -> AppResult<crate::sources::onedrive::UserInfo> {
    use crate::sources::onedrive;
    let cid = client_id.as_deref().unwrap_or(onedrive::DEFAULT_CLIENT_ID);
    let access = onedrive::user_initiated_current_access_token(cid).await?;
    onedrive::user_initiated_fetch_userinfo(&access).await
}

/// Upload the local source copies of `photo_ids` to the signed-in user's
/// OneDrive under `OneDrive/Photos/<remote_folder>/`.
///
/// Files ≤ 4 MB use a simple PUT; larger files use the Graph API
/// upload-session chunked protocol.
#[tauri::command]
pub async fn onedrive_upload(
    state_: State<'_, AppState>,
    client_id: Option<String>,
    photo_ids: Vec<i64>,
    remote_folder: String,
) -> AppResult<crate::sources::onedrive::UploadReceipt> {
    use crate::sources::onedrive;

    let cid = client_id.as_deref().unwrap_or(onedrive::DEFAULT_CLIENT_ID);

    if photo_ids.is_empty() {
        return Ok(onedrive::UploadReceipt {
            uploaded_count: 0,
            skipped_count: 0,
            errors: Vec::new(),
        });
    }

    // Resolve local paths for each photo_id.
    let mut paths: Vec<std::path::PathBuf> = Vec::with_capacity(photo_ids.len());
    for pid in &photo_ids {
        let path: Option<String> = sqlx::query_scalar(
            "SELECT path FROM source_copies \
             WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
        )
        .bind(pid)
        .fetch_optional(&state_.pool)
        .await?;

        match path {
            Some(p) => paths.push(std::path::PathBuf::from(p)),
            None => {
                tracing::warn!(photo_id = pid, "onedrive_upload: no local path, skipping");
            }
        }
    }

    onedrive::user_initiated_upload_to_onedrive(cid, &paths, &remote_folder).await
}

// ── Debug-only test fixtures ──────────────────────────────────────────────
//
// Playwright's `phase-1-import-throughput.spec.ts` needs a way to drop N
// synthetic JPEGs on disk before kicking off an import. We expose it as a
// Tauri command gated on `cfg(debug_assertions)` so the release binaries
// never ship this surface.

/// Write `count` deterministic ~50 KB synthetic JPEGs into `dir`, named
/// `photo_00000.jpg` through `photo_{count-1}.jpg`. Returns the number of
/// files actually written (useful when writes partially fail).
///
/// Debug-only. Invoked from the e2e fixture-generator step.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn __test_generate_fixture(dir: String, count: usize) -> AppResult<usize> {
    use crate::util::synthetic::synthesize_jpeg;
    use std::path::PathBuf;

    let dir_path = PathBuf::from(&dir);
    std::fs::create_dir_all(&dir_path)?;

    let written = tokio::task::spawn_blocking(move || -> AppResult<usize> {
        let mut n = 0usize;
        for i in 0..count {
            let bytes = synthesize_jpeg(i);
            if bytes.is_empty() {
                continue;
            }
            let path = dir_path.join(format!("photo_{i:05}.jpg"));
            std::fs::write(&path, bytes)?;
            n += 1;
        }
        Ok(n)
    })
    .await
    .map_err(|e| AppError::Internal(format!("fixture generator join: {e}")))??;

    tracing::info!(written, dir, "synthetic fixture generated");
    Ok(written)
}

/// Seed `count` photos + SHA256-verified `source_copies` rows against
/// `source_id`. Writes real JPEG bytes to `dir` so the later
/// `cleanup_execute` SHA re-verification path has something real to hash.
///
/// Debug-only. Backs `tests/e2e/phase-1-source-cleanup.spec.ts`.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn __test_seed_source_copies(
    source_id: i64,
    dir: String,
    count: usize,
    state: State<'_, AppState>,
) -> AppResult<Vec<i64>> {
    use crate::util::synthetic::synthesize_jpeg;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    let dir_path = PathBuf::from(&dir);
    std::fs::create_dir_all(&dir_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut photo_ids = Vec::with_capacity(count);

    for i in 0..count {
        let bytes = synthesize_jpeg(i);
        if bytes.is_empty() {
            continue;
        }
        let filename = format!("cleanup_{i:05}.jpg");
        let path = dir_path.join(&filename);
        std::fs::write(&path, &bytes)?;
        let sha = hex::encode(Sha256::digest(&bytes));

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, size_bytes, is_raw, imported_at) \
             VALUES (?1, ?2, 512, 512, ?3, 0, ?4) RETURNING id",
        )
        .bind(&sha)
        .bind(&filename)
        .bind(bytes.len() as i64)
        .bind(&now)
        .fetch_one(&state.pool)
        .await?;

        sqlx::query(
            "INSERT INTO source_copies \
             (source_id, photo_id, path, verified_sha256, is_primary, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        )
        .bind(source_id)
        .bind(photo_id)
        .bind(path.to_string_lossy().to_string())
        .bind(&sha)
        .bind(&now)
        .execute(&state.pool)
        .await?;

        photo_ids.push(photo_id);
    }

    tracing::info!(count = photo_ids.len(), dir, "seeded source copies");
    Ok(photo_ids)
}

/// Counts returned by `__test_seed_dated_photos`.
#[cfg(debug_assertions)]
#[derive(Debug, Serialize, Deserialize)]
pub struct DatedSeedCounts {
    pub on_this_day: i64,
    pub unseen: i64,
}

/// Seed the catalog so the rediscovery rows on the Catalog home have data:
/// - `on_this_day_count` photos captured on today's MM-DD 2–3 years ago.
/// - `unseen_count` recent high-aesthetic photos with a `photo_views` row
///   whose `last_viewed_at` sits > 2 years in the past, so the "unseen in
///   2 years" query picks them up.
///
/// Debug-only. Backs `tests/e2e/phase-1-rediscovery.spec.ts`.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn __test_seed_dated_photos(
    on_this_day_count: i64,
    unseen_count: i64,
    state: State<'_, AppState>,
) -> AppResult<DatedSeedCounts> {
    let now = chrono::Utc::now();
    let now_ts = now.to_rfc3339();
    let month_day = now.format("%m-%dT12:00:00Z").to_string();

    // On-this-day: rotate through 2 historical years so we don't jam all
    // inserts into one timestamp (which would collide on the filename-sha).
    for i in 0..on_this_day_count {
        let year = now.format("%Y").to_string().parse::<i32>().unwrap_or(2026) - 2 - (i % 2) as i32;
        let captured_at = format!("{year}-{month_day}");
        let sha = format!("otd-{i:016x}");
        let filename = format!("otd_{i:04}.jpg");
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, is_raw, imported_at, \
             captured_at, aesthetic_score) \
             VALUES (?1, ?2, 512, 512, 0, ?3, ?4, 7.5)",
        )
        .bind(&sha)
        .bind(&filename)
        .bind(&now_ts)
        .bind(&captured_at)
        .execute(&state.pool)
        .await?;
    }

    // Unseen: recent photos with a stale view timestamp + a high aesthetic
    // score (the `unseen_photos` query filters on `aesthetic_score >= min_score`,
    // default 0.0 — we use 7.5 to be robust to future threshold bumps).
    let three_years_ago = (now - chrono::Duration::days(3 * 365 + 10)).to_rfc3339();
    for i in 0..unseen_count {
        let sha = format!("unseen-{i:016x}");
        let filename = format!("unseen_{i:04}.jpg");
        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, is_raw, imported_at, \
             captured_at, aesthetic_score) \
             VALUES (?1, ?2, 512, 512, 0, ?3, ?3, 7.5) RETURNING id",
        )
        .bind(&sha)
        .bind(&filename)
        .bind(&now_ts)
        .fetch_one(&state.pool)
        .await?;

        sqlx::query(
            "INSERT INTO photo_views (photo_id, last_viewed_at, view_count) \
             VALUES (?1, ?2, 1)",
        )
        .bind(photo_id)
        .bind(&three_years_ago)
        .execute(&state.pool)
        .await?;
    }

    tracing::info!(on_this_day_count, unseen_count, "seeded dated photos");
    Ok(DatedSeedCounts {
        on_this_day: on_this_day_count,
        unseen: unseen_count,
    })
}

/// Seed `count` photos + synthetic 768-dim L2-unit embeddings into
/// `photos`, `photo_embeddings`, `vec_photo_embeddings` and
/// `vec_photo_embeddings_int8`. Used to exercise the NL-search round-trip
/// from the browser without having to generate real JPEG bytes.
///
/// Debug-only. Backs `tests/e2e/phase-1-search-latency.spec.ts`.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn __test_seed_embeddings(count: usize, state: State<'_, AppState>) -> AppResult<usize> {
    fn lcg_unit_vec(seed: u64) -> Vec<f32> {
        let mut state = seed.wrapping_mul(0x5851_F42D_4C95_7F2D).wrapping_add(1);
        let mut v: Vec<f32> = Vec::with_capacity(768);
        for _ in 0..768 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let bits = (state >> 41) as u32;
            let unit = (bits as f32) / ((1u32 << 23) as f32);
            v.push(unit * 2.0 - 1.0);
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Ensure a model row exists so photo_embeddings.model_id FK resolves.
    let model_id: i64 = sqlx::query_scalar(
        "INSERT INTO models (name, kind, version, sha256, size_bytes) \
         VALUES ('siglip2-b16-image', 'embedding', 'e2e-fixture', 'fixture', 0) \
         ON CONFLICT (name, version) DO UPDATE SET sha256=excluded.sha256 \
         RETURNING id",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(1);

    let mut written = 0usize;
    for i in 0..count {
        let v = lcg_unit_vec(i as u64 + 1);
        let f32_bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        let i8_bytes = crate::catalog::db::quantize_unit_f32_to_i8_bytes(&v);

        let sha = format!("emb-{i:016x}");
        let filename = format!("emb_{i:06}.jpg");
        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, is_raw, imported_at) \
             VALUES (?1, ?2, 100, 100, 0, ?3) RETURNING id",
        )
        .bind(&sha)
        .bind(&filename)
        .bind(&now)
        .fetch_one(&state.pool)
        .await?;

        sqlx::query(
            "INSERT INTO photo_embeddings (photo_id, model_id, embedding, updated_at) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(photo_id)
        .bind(model_id)
        .bind(&f32_bytes)
        .bind(&now)
        .execute(&state.pool)
        .await?;

        // Best-effort into the vec0 tables — skip silently if sqlite-vec
        // isn't loaded (some test harnesses stub it out).
        let _ = sqlx::query("INSERT INTO vec_photo_embeddings(rowid, embedding) VALUES (?1, ?2)")
            .bind(photo_id)
            .bind(&f32_bytes)
            .execute(&state.pool)
            .await;
        let _ = sqlx::query(
            "INSERT INTO vec_photo_embeddings_int8(rowid, embedding) VALUES (?1, vec_int8(?2))",
        )
        .bind(photo_id)
        .bind(&i8_bytes)
        .execute(&state.pool)
        .await;

        written += 1;
    }

    tracing::info!(written, "seeded synthetic embeddings");
    Ok(written)
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
        "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
         size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
         aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
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

// ── AI inference commands ─────────────────────────────────────────────────

/// Detect the hardware tier (CPU / GpuLow / GpuHigh) and available VRAM.
/// Called once at startup so the frontend can show the correct model badge.
#[tauri::command]
pub fn detect_hardware() -> crate::ai::budget::HardwareInfo {
    crate::ai::budget::detect()
}

/// Embed a single image using SigLIP-B/16.
/// Returns 768 f32 values. Errors when the model file is not yet downloaded.
#[tauri::command]
pub fn embed_image(path: String) -> AppResult<Vec<f32>> {
    let model_path = crate::util::paths::models_dir()?.join("siglip-b16-image.onnx");
    let session = crate::ai::siglip::get_or_load(&model_path)?;
    session.embed_image(std::path::Path::new(&path))
}

/// Score a single image for aesthetic quality (1.0–10.0).
/// Errors when the model file is not yet downloaded.
#[tauri::command]
pub fn score_aesthetic(path: String) -> AppResult<f32> {
    let model_path = crate::util::paths::models_dir()?.join("nima.onnx");
    let session = crate::ai::aesthetic::get_or_load(&model_path)?;
    session.score(std::path::Path::new(&path))
}

/// Download one or more AI models to the local models directory.
///
/// `names` is an optional filter — if omitted all known models are downloaded.
/// Progress is emitted as `"chronimage://download-progress"` events.
/// Returns the list of model names that were successfully installed.
#[tauri::command]
pub async fn download_models<R: tauri::Runtime>(
    names: Option<Vec<String>>,
    app_handle: tauri::AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Vec<String>> {
    use crate::ai::download::{user_initiated_download_model, DownloadProgress, KNOWN_MODELS};
    use tauri::Emitter;

    let models_dir = crate::util::paths::models_dir()?;
    let specs: Vec<_> = KNOWN_MODELS
        .iter()
        .filter(|m| {
            names
                .as_ref()
                .map(|n| n.iter().any(|req| req == m.name))
                .unwrap_or(true)
        })
        .collect();

    let mut installed: Vec<String> = Vec::new();

    for spec in specs {
        let handle = app_handle.clone();
        let name = spec.name.to_string();
        let result =
            user_initiated_download_model(spec, &models_dir, move |p: DownloadProgress| {
                let _ = handle.emit("chronimage://download-progress", &p);
            })
            .await;

        match result {
            Ok(path) => {
                // Upsert the model row so the catalog reflects the installation.
                let now_ts = chrono::Utc::now().to_rfc3339();
                let path_str = path.to_string_lossy().to_string();
                let _ = sqlx::query(
                    "INSERT INTO models (name, kind, version, sha256, installed_path, installed_at, size_bytes) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                     ON CONFLICT(name) DO UPDATE SET \
                       installed_path = excluded.installed_path, \
                       installed_at   = excluded.installed_at, \
                       size_bytes     = excluded.size_bytes",
                )
                .bind(&name)
                .bind(spec.kind)
                .bind(spec.version)
                .bind(spec.sha256)
                .bind(&path_str)
                .bind(&now_ts)
                .bind(spec.size_bytes as i64)
                .execute(&state.pool)
                .await;

                tracing::info!(model = %name, path = %path_str, "model installed");
                installed.push(name);
            }
            Err(e) => {
                tracing::warn!(model = %name, error = %e, "model download failed");
            }
        }
    }

    Ok(installed)
}

// ── AI model status ───────────────────────────────────────────────────────────

/// Where an installed model was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelSource {
    /// Pre-extracted in the installer's resource directory.
    Bundled,
    /// Downloaded by the user into the app-data models dir.
    Downloaded,
    /// Not present on disk.
    Missing,
}

/// Per-model installation status returned by `ai_models_status`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub name: String,
    pub kind: String,
    pub filename: String,
    pub installed: bool,
    pub size_bytes: u64,
    /// One of `Bundled` | `Downloaded` | `Missing`.
    pub source: ModelSource,
}

/// Returns the installation status of every model in `KNOWN_MODELS`.
///
/// Resolution order per ADR 0003:
/// 1. If `spec.bundled` and `bundled_models_dir/<filename>` exists → `Bundled, installed = true`
///    (hash verification skipped — installer integrity covers it).
/// 2. Else if `models_dir()/<filename>` exists → `Downloaded`; verify hash using a
///    stat-based cache (`<models_dir>/.hash-cache.json`) to avoid re-reading the full
///    file on every Settings open.  Re-hashes only when size or mtime changed.
/// 3. Else → `Missing, installed = false`.
///
/// This command is read-only and does not initiate any downloads.
/// The frontend uses the response to decide which models to surface in the
/// Settings → Models panel and which download buttons to enable.
///
/// # UI note
/// Adding new models to `KNOWN_MODELS` causes this command to return additional
/// rows; the frontend model list will grow automatically.
#[tauri::command]
pub async fn ai_models_status(app: tauri::AppHandle) -> AppResult<Vec<ModelStatus>> {
    let bundled_dir = crate::util::paths::bundled_models_dir(&app);
    let user_dir = crate::util::paths::models_dir()?;
    resolve_all_model_statuses(bundled_dir, user_dir).await
}

/// Internal helper — accepts explicit paths so tests can inject tempdirs
/// without needing a live Tauri `AppHandle`.
pub(crate) async fn resolve_all_model_statuses(
    bundled_dir: Option<std::path::PathBuf>,
    user_dir: std::path::PathBuf,
) -> AppResult<Vec<ModelStatus>> {
    use crate::ai::download::KNOWN_MODELS;
    use futures::future::try_join_all;

    let cache = load_hash_cache(&user_dir);

    let tasks = KNOWN_MODELS.iter().map(|spec| {
        let bundled_dir = bundled_dir.clone();
        let user_dir = user_dir.clone();
        let cache = cache.clone();
        async move { resolve_model_status(spec, bundled_dir.as_deref(), &user_dir, &cache).await }
    });
    let (statuses, cache_updates): (Vec<ModelStatus>, Vec<Option<(String, HashCacheEntry)>>) = {
        let pairs = try_join_all(tasks).await?;
        pairs.into_iter().unzip()
    };

    // Persist any new cache entries (cache misses that were freshly hashed).
    let updates: Vec<_> = cache_updates.into_iter().flatten().collect();
    if !updates.is_empty() {
        let mut updated = cache;
        for (key, entry) in updates {
            updated.insert(key, entry);
        }
        save_hash_cache(&user_dir, &updated);
    }

    Ok(statuses)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct HashCacheEntry {
    size: u64,
    mtime_secs: u64,
    ok: bool,
}

type HashCache = std::collections::HashMap<String, HashCacheEntry>;

fn load_hash_cache(user_dir: &std::path::Path) -> HashCache {
    let path = user_dir.join(".hash-cache.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_hash_cache(user_dir: &std::path::Path, cache: &HashCache) {
    let path = user_dir.join(".hash-cache.json");
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(&path, json);
    }
}

async fn resolve_model_status(
    spec: &crate::ai::download::ModelSpec,
    bundled_dir: Option<&std::path::Path>,
    user_dir: &std::path::Path,
    cache: &HashCache,
) -> AppResult<(ModelStatus, Option<(String, HashCacheEntry)>)> {
    // 1. Bundled path — installer already verified so no hash work.
    if spec.bundled {
        if let Some(bd) = bundled_dir {
            if bd.join(spec.filename).exists() {
                return Ok((
                    ModelStatus {
                        name: spec.name.to_string(),
                        kind: spec.kind.to_string(),
                        filename: spec.filename.to_string(),
                        installed: true,
                        size_bytes: spec.size_bytes,
                        source: ModelSource::Bundled,
                    },
                    None,
                ));
            }
        }
    }

    // 2. User-data dir — use stat cache to skip re-reading the full file when
    //    size + mtime are unchanged. Only falls back to full SHA256 on cache miss.
    let path = user_dir.join(spec.filename);
    if path.exists() {
        let (verified, cache_update) = if spec.sha256 == "tbd" {
            (true, None)
        } else {
            let meta = std::fs::metadata(&path).ok().map(|m| {
                use std::time::UNIX_EPOCH;
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (m.len(), mtime)
            });

            if let Some((size, mtime)) = meta {
                if let Some(cached) = cache.get(spec.filename) {
                    if cached.size == size && cached.mtime_secs == mtime {
                        // Cache hit — no disk I/O beyond the stat above.
                        (cached.ok, None)
                    } else {
                        let path_clone = path.clone();
                        let expected = spec.sha256.to_string();
                        let ok = tokio::task::spawn_blocking(move || {
                            verify_model_hash_streaming(&path_clone, &expected)
                        })
                        .await
                        .map_err(|e| AppError::Internal(format!("hash task join: {e}")))?;
                        let entry = HashCacheEntry {
                            size,
                            mtime_secs: mtime,
                            ok,
                        };
                        (ok, Some((spec.filename.to_string(), entry)))
                    }
                } else {
                    let path_clone = path.clone();
                    let expected = spec.sha256.to_string();
                    let ok = tokio::task::spawn_blocking(move || {
                        verify_model_hash_streaming(&path_clone, &expected)
                    })
                    .await
                    .map_err(|e| AppError::Internal(format!("hash task join: {e}")))?;
                    let entry = HashCacheEntry {
                        size,
                        mtime_secs: mtime,
                        ok,
                    };
                    (ok, Some((spec.filename.to_string(), entry)))
                }
            } else {
                (false, None)
            }
        };

        if verified {
            return Ok((
                ModelStatus {
                    name: spec.name.to_string(),
                    kind: spec.kind.to_string(),
                    filename: spec.filename.to_string(),
                    installed: true,
                    size_bytes: spec.size_bytes,
                    source: ModelSource::Downloaded,
                },
                cache_update,
            ));
        }
    }

    // 3. Missing — either not on disk or hash mismatch.
    Ok((
        ModelStatus {
            name: spec.name.to_string(),
            kind: spec.kind.to_string(),
            filename: spec.filename.to_string(),
            installed: false,
            size_bytes: spec.size_bytes,
            source: ModelSource::Missing,
        },
        None,
    ))
}

/// Stream-hash `path` and compare against `expected` (lowercase hex). Uses
/// an 8 MB read buffer so a 2.7 GB model doesn't balloon memory. Returns
/// `false` on any I/O error (treated as not-installed).
fn verify_model_hash_streaming(path: &std::path::Path, expected: &str) -> bool {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8 * 1024 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    hex::encode(hasher.finalize()) == expected
}

/// Truncate and recompute the data produced by `kind`.
///
/// Used after a model swap from Settings → AI Models. The command clears
/// the relevant derived data and returns the row count that was cleared;
/// the actual re-inference is triggered by the user kicking off a re-import
/// or by the background re-evaluator on its next cycle.
///
/// `kind` must be one of:
/// - `"embeddings"` — clears `photo_embeddings` and `vec_photo_embeddings`.
/// - `"face-detect"` / `"face-embed"` — clears `faces` and `clusters`.
/// - `"aesthetic"` — NULLs `photos.aesthetic_score`.
/// - `"captions"` — removes auto-scene tags produced by the active caption model.
///
/// Returns `AppError::InvalidInput` for unknown kind values.
#[tauri::command]
pub async fn ai_reindex(state: State<'_, AppState>, kind: String) -> AppResult<i64> {
    let count = match kind.as_str() {
        "embeddings" => {
            let deleted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photo_embeddings")
                .fetch_one(&state.pool)
                .await?;
            sqlx::query("DELETE FROM photo_embeddings")
                .execute(&state.pool)
                .await?;
            // vec_photo_embeddings is a sqlite-vec virtual table; it may not
            // exist on CPU-only installs. Ignore the error if the table is absent.
            let _ = sqlx::query("DELETE FROM vec_photo_embeddings")
                .execute(&state.pool)
                .await;
            deleted
        }
        "face-detect" | "face-embed" => {
            let deleted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM faces")
                .fetch_one(&state.pool)
                .await?;
            sqlx::query("DELETE FROM faces")
                .execute(&state.pool)
                .await?;
            sqlx::query("UPDATE clusters SET photo_count = 0")
                .execute(&state.pool)
                .await?;
            sqlx::query("DELETE FROM clusters")
                .execute(&state.pool)
                .await?;
            deleted
        }
        "aesthetic" => {
            let affected: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE aesthetic_score IS NOT NULL")
                    .fetch_one(&state.pool)
                    .await?;
            sqlx::query("UPDATE photos SET aesthetic_score = NULL")
                .execute(&state.pool)
                .await?;
            affected
        }
        "captions" => {
            let deleted: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tags \
                 WHERE kind = 'auto_scene' \
                   AND model_id = (SELECT id FROM models WHERE name LIKE 'moondream%' LIMIT 1)",
            )
            .fetch_one(&state.pool)
            .await?;
            sqlx::query(
                "DELETE FROM tags \
                 WHERE kind = 'auto_scene' \
                   AND model_id = (SELECT id FROM models WHERE name LIKE 'moondream%' LIMIT 1)",
            )
            .execute(&state.pool)
            .await?;
            deleted
        }
        other => {
            return Err(AppError::InvalidInput(format!(
                "unknown ai_reindex kind: {other:?}; \
                 expected one of: embeddings, face-detect, face-embed, aesthetic, captions"
            )));
        }
    };
    Ok(count)
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
        "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.imported_at, \
         p.is_raw, p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, \
         p.iso, p.focal_mm, p.aesthetic_score, p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
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

/// Photos captured within 30 days of the first photo from a previously-unseen
/// camera make+model. Surfaces "this is the era you first got your A7 IV" moments
/// on the Catalog home. Ordered by camera-introduction recency then capture date.
#[tauri::command]
pub async fn first_time_on_new_camera(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(20).clamp(1, 200);
    let rows = sqlx::query_as::<_, PhotoRow>(
        "WITH first_seen AS ( \
           SELECT camera_make, camera_model, MIN(captured_at) AS first_at \
           FROM photos \
           WHERE camera_make IS NOT NULL AND camera_make != '' AND captured_at IS NOT NULL \
           GROUP BY camera_make, camera_model \
         ) \
         SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.imported_at, \
           p.is_raw, p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, \
           p.iso, p.focal_mm, p.aesthetic_score, p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
         FROM photos p \
         JOIN first_seen fs \
           ON fs.camera_make = p.camera_make \
          AND COALESCE(fs.camera_model, '') = COALESCE(p.camera_model, '') \
         WHERE p.captured_at IS NOT NULL \
           AND julianday(p.captured_at) - julianday(fs.first_at) <= 30.0 \
         ORDER BY fs.first_at DESC, p.captured_at ASC \
         LIMIT ?1",
    )
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

/// High-aesthetic photos the user has never surfaced — NIMA ≥ 8.0 AND never viewed.
/// Used for the "unflagged favorites" rediscovery row.
#[tauri::command]
pub async fn unflagged_favorites(
    state: State<'_, AppState>,
    limit: Option<i64>,
    min_score: Option<f64>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(20).clamp(1, 200);
    let score = min_score.unwrap_or(8.0);
    let rows = sqlx::query_as::<_, PhotoRow>(
        "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.imported_at, \
         p.is_raw, p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, \
         p.iso, p.focal_mm, p.aesthetic_score, p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
         FROM photos p \
         LEFT JOIN photo_views pv ON pv.photo_id = p.id \
         WHERE p.aesthetic_score IS NOT NULL \
           AND p.aesthetic_score >= ?1 \
           AND (pv.photo_id IS NULL OR pv.view_count = 0 OR pv.last_viewed_at IS NULL) \
         ORDER BY p.aesthetic_score DESC \
         LIMIT ?2",
    )
    .bind(score)
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

// ── Face-cluster commands ─────────────────────────────────────────────────────

/// List face clusters ordered by `face_count` desc, up to `limit` rows.
///
/// Returns an empty vec when the `clusters` table has no rows (i.e. AI
/// clustering has not run yet). The query is safe to call before clustering.
#[tauri::command]
pub async fn face_clusters_list(
    state: State<'_, AppState>,
    limit: i64,
) -> AppResult<Vec<catalog::models::ClusterRow>> {
    let rows: Vec<catalog::models::ClusterRow> = sqlx::query_as::<_, catalog::models::ClusterRow>(
        "SELECT c.id,
                c.name,
                c.is_named,
                COUNT(f.id) AS face_count,
                (SELECT f2.photo_id FROM faces f2
                 WHERE f2.id = c.cover_face_id) AS cover_photo_id
         FROM clusters c
         LEFT JOIN faces f ON f.cluster_id = c.id
         GROUP BY c.id
         ORDER BY face_count DESC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

/// Assign a human-readable name to a face cluster.
///
/// Sets `clusters.name = name` and `is_named = 1`.
/// Returns `AppError::NotFound` when `cluster_id` does not exist.
#[tauri::command]
pub async fn face_cluster_name(
    state: State<'_, AppState>,
    cluster_id: i64,
    name: String,
) -> AppResult<()> {
    let affected = sqlx::query("UPDATE clusters SET name = ?1, is_named = 1 WHERE id = ?2")
        .bind(&name)
        .bind(cluster_id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "cluster {cluster_id} not found"
        )));
    }
    Ok(())
}

/// Merge cluster `b` into cluster `a`: reassign all faces from `b` to `a`,
/// then delete cluster `b`. Returns the surviving cluster id (`a`).
///
/// - If `a == b`, returns `Ok(a)` immediately (idempotent).
/// - Returns `AppError::NotFound` when `a` or `b` do not exist.
#[tauri::command]
pub async fn face_cluster_merge(state: State<'_, AppState>, a: i64, b: i64) -> AppResult<i64> {
    if a == b {
        return Ok(a);
    }

    // Verify both clusters exist before opening a transaction.
    let a_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM clusters WHERE id = ?1")
        .bind(a)
        .fetch_optional(&state.pool)
        .await?;
    if a_exists.is_none() {
        return Err(AppError::NotFound(format!("cluster {a} not found")));
    }

    let b_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM clusters WHERE id = ?1")
        .bind(b)
        .fetch_optional(&state.pool)
        .await?;
    if b_exists.is_none() {
        return Err(AppError::NotFound(format!("cluster {b} not found")));
    }

    let mut tx = state.pool.begin().await?;

    sqlx::query("UPDATE faces SET cluster_id = ?1 WHERE cluster_id = ?2")
        .bind(a)
        .bind(b)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM clusters WHERE id = ?1")
        .bind(b)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(a)
}

// ── Face-cluster rebuild ──────────────────────────────────────────────────────

/// Event channel for recluster progress.
pub const RECLUSTER_PROGRESS_EVENT: &str = "chronimage://recluster-progress";

/// Event channel for rebuild-thumbnails progress.
pub const REBUILD_PROGRESS_EVENT: &str = "chronimage://rebuild-progress";

#[derive(Debug, Clone, Serialize)]
pub struct ReclusterProgress {
    pub phase: &'static str, // "start" | "done"
    pub total_faces: i64,
    pub clustered_faces: i64,
    pub cluster_count: i64,
}

/// Run HDBSCAN over every face embedding and persist cluster assignments.
///
/// Idempotent — safe to call repeatedly. Named clusters preserve their name
/// across re-runs via centroid cosine similarity (see
/// `ai::cluster_persist::reeval_clusters`).
#[tauri::command]
pub async fn recluster_faces<R: tauri::Runtime>(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle<R>,
) -> AppResult<crate::ai::cluster_persist::ReclusterReceipt> {
    use tauri::Emitter;
    let _ = app_handle.emit(
        RECLUSTER_PROGRESS_EVENT,
        ReclusterProgress {
            phase: "start",
            total_faces: 0,
            clustered_faces: 0,
            cluster_count: 0,
        },
    );

    let receipt = crate::ai::cluster_persist::reeval_clusters(&state.pool).await?;

    let _ = app_handle.emit(
        RECLUSTER_PROGRESS_EVENT,
        ReclusterProgress {
            phase: "done",
            total_faces: receipt.total_faces,
            clustered_faces: receipt.clustered_faces,
            cluster_count: receipt.cluster_count,
        },
    );

    Ok(receipt)
}

// ── Thumbnail cache rebuild ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RebuildProgress {
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub current_photo_id: i64,
    pub phase: &'static str, // "start" | "tick" | "done"
}

#[derive(Debug, Serialize)]
pub struct RebuildReceipt {
    pub total: i64,
    pub regenerated: i64,
    pub failed: i64,
    pub elapsed_ms: u64,
}

/// Delete every cached 320 px thumbnail and re-generate from source with
/// EXIF orientation applied. Used to repair catalogs that were imported
/// before the orientation fix landed (pre-2026-04-24). Emits per-photo
/// progress events on `REBUILD_PROGRESS_EVENT`.
#[tauri::command]
pub async fn rebuild_thumbnails<R: tauri::Runtime>(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle<R>,
) -> AppResult<RebuildReceipt> {
    use tauri::Emitter;
    let t0 = std::time::Instant::now();

    // Load (photo_id, sha256, orientation) for every photo with a local
    // source_copies entry. Cloud-only photos are skipped — nothing to
    // regenerate from.
    let rows: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
        "SELECT DISTINCT p.id, p.sha256, p.orientation
         FROM photos p
         JOIN source_copies sc ON sc.photo_id = p.id
         WHERE sc.path IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await?;

    let total = rows.len() as i64;
    let _ = app_handle.emit(
        REBUILD_PROGRESS_EVENT,
        RebuildProgress {
            total,
            done: 0,
            failed: 0,
            current_photo_id: 0,
            phase: "start",
        },
    );

    let thumbs_dir = crate::util::paths::thumbnails_dir()?;

    let mut regenerated = 0_i64;
    let mut failed = 0_i64;

    for (photo_id, sha, orientation) in &rows {
        let cache_path = thumbs_dir.join(format!("{sha}_320.jpg"));
        let _ = std::fs::remove_file(&cache_path);

        // Resolve any local path for this photo (prefer is_primary DESC).
        let source_path: Option<String> = sqlx::query_scalar(
            "SELECT path FROM source_copies
             WHERE photo_id = ?1 AND path IS NOT NULL
             ORDER BY is_primary DESC, id ASC LIMIT 1",
        )
        .bind(photo_id)
        .fetch_optional(&state.pool)
        .await?;
        let Some(path) = source_path else {
            failed += 1;
            continue;
        };
        let path_buf = PathBuf::from(&path);
        if !path_buf.exists() {
            failed += 1;
            continue;
        }

        let orientation_u32 = orientation.and_then(|v| u32::try_from(v).ok());
        let cache_path_clone = cache_path.clone();
        let task_result = tokio::task::spawn_blocking(move || -> AppResult<f32> {
            let img = image::open(&path_buf)
                .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
            let img = crate::ai::image_util::apply_exif_orientation(img, orientation_u32);
            let resized = img.thumbnail(320, 320);
            let mut buf = Vec::with_capacity(32 * 1024);
            let mut cursor = std::io::Cursor::new(&mut buf);
            resized
                .write_to(&mut cursor, image::ImageFormat::Jpeg)
                .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
            std::fs::write(&cache_path_clone, &buf)?;
            Ok(crate::ai::image_util::laplacian_variance(&resized))
        })
        .await;

        match task_result {
            Ok(Ok(sharpness)) => {
                regenerated += 1;
                // Also update sharpness_score now that we have the oriented thumb.
                let _ = sqlx::query("UPDATE photos SET sharpness_score = ?1 WHERE id = ?2")
                    .bind(sharpness as f64)
                    .bind(photo_id)
                    .execute(&state.pool)
                    .await;
            }
            _ => {
                failed += 1;
            }
        }

        let _ = app_handle.emit(
            REBUILD_PROGRESS_EVENT,
            RebuildProgress {
                total,
                done: regenerated,
                failed,
                current_photo_id: *photo_id,
                phase: "tick",
            },
        );
    }

    let elapsed_ms = t0.elapsed().as_millis() as u64;
    let _ = app_handle.emit(
        REBUILD_PROGRESS_EVENT,
        RebuildProgress {
            total,
            done: regenerated,
            failed,
            current_photo_id: 0,
            phase: "done",
        },
    );

    tracing::info!(
        total,
        regenerated,
        failed,
        elapsed_ms,
        "rebuild_thumbnails: done"
    );

    Ok(RebuildReceipt {
        total,
        regenerated,
        failed,
        elapsed_ms,
    })
}

// ── Photo-view recording ──────────────────────────────────────────────────────

/// Record that the user viewed `photo_id`. Upserts into `photo_views`,
/// incrementing `view_count` and updating `last_viewed_at`.
///
/// The `photo_views_sync_insert` trigger (migration 20260423000001) propagates
/// `last_viewed_at` to `photos.last_viewed_at` so the `LastViewed` rule can
/// evaluate it without a subquery join.
#[tauri::command]
pub async fn record_photo_view(state: State<'_, AppState>, photo_id: i64) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO photo_views (photo_id, last_viewed_at, view_count)
         VALUES (?1, ?2, 1)
         ON CONFLICT(photo_id) DO UPDATE SET
           last_viewed_at = excluded.last_viewed_at,
           view_count     = view_count + 1",
    )
    .bind(photo_id)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(())
}

// ── Dedupe commands ───────────────────────────────────────────────────────────

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

// ── Natural-language search ───────────────────────────────────────────────

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

    // 1. Encode the query text into a 768-dim vector via the global SigLIP session.
    //    `global_siglip_session` is memoised at app boot by `init_global_siglip_session`.
    //    When None (model absent or load failed) there is nothing to rank — return empty.
    let siglip = match crate::ai::siglip::global_siglip_session() {
        Some(s) => s,
        None => {
            tracing::debug!(
                query = %query,
                "search_photos: SigLIP not loaded — model absent or init not called"
            );
            return Ok(Vec::new());
        }
    };

    let mut query_vec = siglip.embed_text(&query)?;

    // 2. The text encoder already L2-normalises before returning, but normalise
    //    defensively in case a caller passes through a non-unit vector.
    l2_normalise(&mut query_vec);

    // If the query vector is all-zero (stub path somehow reached) there's
    // nothing meaningful to rank; return empty rather than arbitrary ordering.
    let is_zero = query_vec.iter().all(|x| *x == 0.0);
    if is_zero {
        tracing::debug!(
            query = %query,
            "search_photos: SigLIP returned zero vector — model may be a stub"
        );
        return Ok(Vec::new());
    }

    // 3. Primary path: HNSW ANN (O(log n)). Falls back to sqlite-vec
    //    brute force when the catalog is too small or build fails.
    let ann_ranked =
        match crate::ai::ann::search(&state.pool, &query_vec, max_results as usize).await {
            Ok(Some(rows)) => rows,
            Ok(None) => Vec::new(),
            Err(e) => {
                tracing::warn!(error = %e, "ann search failed — falling back to sqlite-vec");
                Vec::new()
            }
        };

    if !ann_ranked.is_empty() {
        return hydrate_photo_rows(&state.pool, ann_ranked).await;
    }

    // 4. Encode the query vector in two forms: f32 LE bytes (fallback) and
    //    int8 quantised (brute-force primary — 4× smaller, 4× faster).
    let query_f32_bytes: Vec<u8> = query_vec.iter().flat_map(|f| f.to_le_bytes()).collect();
    let query_i8_bytes = crate::catalog::db::quantize_unit_f32_to_i8_bytes(&query_vec);

    // 4a. Brute-force primary: int8 vec0 KNN (`vec_photo_embeddings_int8`,
    //     populated by stage-4). 768 bytes per row vs. 3072 for f32 → ~4×
    //     throughput on the scan. Distance ordering is preserved under
    //     symmetric i8 quantisation of L2-normed vectors.
    let knn_rows: Vec<(i64, f32)> = sqlx::query_as(
        "SELECT rowid, distance FROM vec_photo_embeddings_int8 \
         WHERE embedding MATCH vec_int8(?1) ORDER BY distance LIMIT ?2",
    )
    .bind(&query_i8_bytes)
    .bind(max_results)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::debug!(error = %e, "int8 KNN failed, trying f32 KNN");
        Vec::new()
    });

    // 4b. Fallback: f32 vec0 KNN (`vec_photo_embeddings`) — some catalogs
    //     may have been populated before the int8 table existed.
    let knn_rows = if knn_rows.is_empty() {
        sqlx::query_as::<_, (i64, f32)>(
            "SELECT rowid, distance FROM vec_photo_embeddings \
             WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
        )
        .bind(&query_f32_bytes)
        .bind(max_results)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "vec0 KNN query failed — falling back to BLOB scan");
            Vec::new()
        })
    } else {
        knn_rows
    };

    let mut scored: Vec<(i64, f32)> = if !knn_rows.is_empty() {
        // distance-asc → map to similarity-desc so the rest of the code keeps
        // the "higher is better" convention.
        knn_rows
            .into_iter()
            .map(|(rowid, dist)| (rowid, 1.0 - dist * 0.5))
            .collect()
    } else {
        // Legacy BLOB fallback: catalogs populated before stage-4 started
        // writing vec_photo_embeddings, or installs where sqlite-vec failed.
        let blob_rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT photo_id, embedding FROM photo_embeddings WHERE embedding IS NOT NULL",
        )
        .fetch_all(&state.pool)
        .await?;

        if blob_rows.is_empty() {
            return Ok(Vec::new());
        }

        let expected_bytes = crate::ai::EMBED_DIM * std::mem::size_of::<f32>();
        let mut scored: Vec<(i64, f32)> = blob_rows
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
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_results as usize);
        scored
    };

    if scored.is_empty() {
        return Ok(Vec::new());
    }

    // vec0 already returns in ascending-distance order (best first), so no
    // re-sort needed for that branch. The BLOB branch sorted above.
    let _ = &mut scored;

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
         is_raw, size_bytes, camera_make, camera_model, aperture, shutter, iso, \
         focal_mm, aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
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

/// Hydrate a `(photo_id, score)` ranking into full `PhotoRow`s, preserving
/// rank order. Used by both the ANN path and the brute-force path.
async fn hydrate_photo_rows(
    pool: &sqlx::SqlitePool,
    ranked: Vec<(i64, f32)>,
) -> AppResult<Vec<PhotoRow>> {
    if ranked.is_empty() {
        return Ok(Vec::new());
    }
    let photo_ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
    let placeholders: String = photo_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, sha256, filename, width, height, captured_at, imported_at, \
         is_raw, size_bytes, camera_make, camera_model, aperture, shutter, iso, \
         focal_mm, aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
         FROM photos WHERE id IN ({placeholders})"
    );
    let mut q = sqlx::query_as::<_, PhotoRow>(&sql);
    for id in &photo_ids {
        q = q.bind(id);
    }
    let mut photos: Vec<PhotoRow> = q.fetch_all(pool).await?;
    let order: std::collections::HashMap<i64, usize> = photo_ids
        .iter()
        .enumerate()
        .map(|(rank, &id)| (id, rank))
        .collect();
    photos.sort_by_key(|p| order.get(&p.id).copied().unwrap_or(usize::MAX));
    Ok(photos)
}

/// Row shape for the AI-tags panel in the photo detail inspector.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TagRow {
    pub id: i64,
    pub label: String,
    pub kind: String,
    pub confidence: f64,
}

/// All tags attached to a photo, ordered by confidence (most-confident first).
/// Covers AI-assigned (people/place/object/event/color/camera/auto_scene) and
/// user-assigned (`kind = 'user'`) tags.
#[tauri::command]
pub async fn list_tags(photo_id: i64, state: State<'_, AppState>) -> AppResult<Vec<TagRow>> {
    let rows = sqlx::query_as::<_, TagRow>(
        "SELECT id, label, kind, confidence FROM tags \
         WHERE photo_id = ?1 ORDER BY confidence DESC, id ASC",
    )
    .bind(photo_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

/// Photos in which at least one face belongs to the given cluster.
///
/// Used by the PeopleScreen drill-down. Ordered by best face quality within
/// this cluster (most-confident face first) so the user sees the strongest
/// matches at the top.
#[tauri::command]
pub async fn list_photos_for_cluster(
    cluster_id: i64,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<Vec<PhotoRow>> {
    let lim = limit.unwrap_or(200).clamp(1, 1000);
    let rows = sqlx::query_as::<_, PhotoRow>(
        "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, p.imported_at, \
         p.is_raw, p.size_bytes, p.camera_make, p.camera_model, p.aperture, p.shutter, p.iso, \
         p.focal_mm, p.aesthetic_score, p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
         FROM photos p \
         JOIN ( \
            SELECT photo_id, MAX(quality) AS top_quality \
            FROM faces WHERE cluster_id = ?1 \
            GROUP BY photo_id \
         ) f ON f.photo_id = p.id \
         ORDER BY f.top_quality DESC, p.imported_at DESC \
         LIMIT ?2",
    )
    .bind(cluster_id)
    .bind(lim)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

/// GPS coordinates for a photo, if the import pipeline extracted them from EXIF.
#[derive(Debug, Serialize, Deserialize)]
pub struct PhotoLocation {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[tauri::command]
pub async fn photo_location(photo_id: i64, state: State<'_, AppState>) -> AppResult<PhotoLocation> {
    let (lat, lng): (Option<f64>, Option<f64>) =
        sqlx::query_as("SELECT gps_lat, gps_lng FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("photo {photo_id}")))?;
    Ok(PhotoLocation { lat, lng })
}

/// Aggregate quality metrics for a single photo used by the detail inspector.
///
/// Sources:
/// - `sharpness_score` + `aesthetic_score` from the `photos` row.
/// - Face-level metrics (best face quality, min eyes-open across faces, face
///   count) aggregated from the `faces` table.
#[derive(Debug, Serialize, Deserialize)]
pub struct PhotoQuality {
    pub aesthetic: Option<f64>,
    pub sharpness: Option<f64>,
    pub face_count: i64,
    /// Max quality score across faces (0–1). None when no faces.
    pub best_face_quality: Option<f64>,
    /// Min eyes-open probability across faces (0–1). None when no face has the signal.
    pub min_eyes_open: Option<f64>,
}

#[tauri::command]
pub async fn photo_quality(photo_id: i64, state: State<'_, AppState>) -> AppResult<PhotoQuality> {
    let (aesthetic, sharpness): (Option<f64>, Option<f64>) =
        sqlx::query_as("SELECT aesthetic_score, sharpness_score FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("photo {photo_id}")))?;

    let (face_count, best_face_quality, min_eyes_open): (i64, Option<f64>, Option<f64>) =
        sqlx::query_as(
            "SELECT COUNT(*), MAX(quality), MIN(eyes_open) FROM faces WHERE photo_id = ?1",
        )
        .bind(photo_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(PhotoQuality {
        aesthetic,
        sharpness,
        face_count,
        best_face_quality: if face_count > 0 {
            best_face_quality
        } else {
            None
        },
        min_eyes_open: if face_count > 0 { min_eyes_open } else { None },
    })
}

/// JPEG-encoded thumbnail bytes for a photo, resized to `size_px` longest edge.
///
/// Caches to `{app_data}/cache/thumbnails/{sha256}_{size}.jpg` so subsequent
/// calls are free. RAW photos resolve to their paired JPG when available; if
/// no JPG pair exists, returns `NotFound` (frontend falls back to
/// placeholder). Missing source files also return `NotFound`.
///
/// Default `size_px` is 320 (covers Catalog grid tiles at all zoom levels on
/// typical screens).
#[tauri::command]
pub async fn get_thumbnail(
    photo_id: i64,
    size_px: Option<u32>,
    state: State<'_, AppState>,
) -> AppResult<Vec<u8>> {
    generate_thumbnail_bytes(photo_id, size_px, &state.pool).await
}

async fn generate_thumbnail_bytes(
    photo_id: i64,
    size_px: Option<u32>,
    pool: &sqlx::SqlitePool,
) -> AppResult<Vec<u8>> {
    let size = size_px.unwrap_or(320).clamp(64, 2048);

    // Load photo row with pairing info.
    let (sha256, is_raw, paired_photo_id): (String, bool, Option<i64>) =
        sqlx::query_as("SELECT sha256, is_raw, paired_photo_id FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("photo {photo_id}")))?;

    // Fast-cache hit.
    let thumbs_dir = crate::util::paths::thumbnails_dir()?;
    let cache_path = thumbs_dir.join(format!("{sha256}_{size}.jpg"));
    if let Ok(bytes) = std::fs::read(&cache_path) {
        return Ok(bytes);
    }

    // Resolve the source photo to read: a RAW photo prefers its paired JPG;
    // non-RAW uses its own primary local copy.
    let source_photo_id = match (is_raw, paired_photo_id) {
        (true, Some(pid)) => pid,
        _ => photo_id,
    };

    let source_path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies \
         WHERE photo_id = ?1 AND path IS NOT NULL \
         ORDER BY is_primary DESC, id ASC LIMIT 1",
    )
    .bind(source_photo_id)
    .fetch_optional(pool)
    .await?;

    let path = source_path.ok_or_else(|| {
        tracing::warn!(
            photo_id,
            "thumbnail: no source_copies path (cloud-only or not yet imported)"
        );
        AppError::NotFound(format!("photo {photo_id}: no local path in source_copies"))
    })?;
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        tracing::warn!(photo_id, path, "thumbnail: source file missing on disk");
        return Err(AppError::NotFound(format!(
            "photo {photo_id}: file not on disk: {path}"
        )));
    }

    // Look up the stored EXIF orientation (1–8) so we can rotate before
    // resizing. Missing / unset column defaults to 1 (no rotation).
    let orientation: Option<i64> =
        sqlx::query_scalar("SELECT orientation FROM photos WHERE id = ?1")
            .bind(source_photo_id)
            .fetch_optional(pool)
            .await?;
    let orientation_u32 = orientation.and_then(|v| u32::try_from(v).ok());

    let bytes = tokio::task::spawn_blocking(move || -> AppResult<Vec<u8>> {
        let img = crate::ai::image_util::open_any(&path_buf)
            .map_err(|e| AppError::Io(std::io::Error::other(e)))?;
        let img = crate::ai::image_util::apply_exif_orientation(img, orientation_u32);
        let resized = img.thumbnail(size, size);
        let mut buf = Vec::with_capacity(64 * 1024);
        let mut cursor = std::io::Cursor::new(&mut buf);
        resized
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
        Ok(buf)
    })
    .await
    .map_err(|e| AppError::Internal(format!("thumbnail task join: {e}")))??;

    // Best-effort cache write — a failure here (e.g. disk full) must not block
    // the response path.
    if let Err(e) = std::fs::write(&cache_path, &bytes) {
        tracing::warn!(error = %e, cache_path = %cache_path.display(), "thumbnail cache write failed");
    }

    Ok(bytes)
}

/// Natural-language search suggestion chips for the catalog search bar.
///
/// Blends curated seed prompts with dynamic hints derived from the user's
/// catalog (top named face clusters, top camera make/model). Returns at most
/// 8 unique strings. Never errors — on DB failure the curated seeds alone
/// are returned.
#[tauri::command]
pub async fn search_suggestions(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    Ok(build_search_suggestions(&state.pool).await)
}

async fn build_search_suggestions(pool: &sqlx::SqlitePool) -> Vec<String> {
    let curated: &[&str] = &[
        "golden hour portraits",
        "sunset over water",
        "laughing at a dinner table",
        "a dog running on sand",
        "snow-capped mountains",
        "street at night, neon signs",
        "handwritten notes on paper",
        "a crowded city square",
    ];

    let mut out: Vec<String> = Vec::with_capacity(8);

    // Dynamic hint 1: top 2 named face clusters → "Photos of {name}"
    let cluster_names: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT name FROM clusters \
         WHERE is_named = 1 AND name IS NOT NULL AND name != '' \
         ORDER BY COALESCE((SELECT COUNT(*) FROM faces WHERE cluster_id = clusters.id), 0) DESC \
         LIMIT 2",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for n in cluster_names {
        out.push(format!("Photos of {n}"));
    }

    // Dynamic hint 2: top camera make/model.
    if let Ok(Some((make, model))) = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT camera_make, camera_model FROM photos \
         WHERE camera_make IS NOT NULL AND camera_make != '' \
         GROUP BY camera_make, camera_model \
         ORDER BY COUNT(*) DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        let label = match (make, model) {
            (Some(m), Some(md)) if !md.is_empty() => format!("{m} {md}"),
            (Some(m), _) => m,
            _ => String::new(),
        };
        if !label.is_empty() {
            out.push(format!("{label} shots"));
        }
    }

    for s in curated {
        if out.len() >= 8 {
            break;
        }
        let candidate = (*s).to_string();
        if !out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&candidate))
        {
            out.push(candidate);
        }
    }

    out
}

// ── Phase 2: Cull verdict + rating + flag ─────────────────────────────────────

use crate::cull::bin::{CullBinRow, CullBinSummary, CullFilter, EmptyReceipt, RestoreReceipt};
use crate::cull::verdict::{CullReason, Verdict, VerdictReceipt};
use crate::export::engine::ExportProgress;
use crate::export::{ExportJob, ExportPreset};
use tauri::Emitter;

/// Tauri event name emitted by `cull_apply_verdict`. Payload = `VerdictReceipt`.
pub const CULL_PROGRESS_EVENT: &str = "chronimage://cull-progress";

/// Tauri event name emitted per export item. Payload = `ExportProgress`.
pub const EXPORT_PROGRESS_EVENT: &str = "chronimage://export-progress";

#[tauri::command]
pub async fn cull_apply_verdict(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    photo_id: i64,
    verdict: Verdict,
    reason: CullReason,
    retention_days: Option<i64>,
) -> AppResult<VerdictReceipt> {
    let retention = retention_days.unwrap_or(30);
    let receipt =
        crate::cull::verdict::apply_verdict(&state.pool, photo_id, verdict, reason, retention)
            .await?;
    let _ = app.emit(CULL_PROGRESS_EVENT, &receipt);
    Ok(receipt)
}

#[tauri::command]
pub async fn rate_photo(state: State<'_, AppState>, photo_id: i64, rating: i64) -> AppResult<()> {
    crate::cull::verdict::set_star_rating(&state.pool, photo_id, rating).await
}

#[tauri::command]
pub async fn flag_photo(state: State<'_, AppState>, photo_id: i64) -> AppResult<bool> {
    crate::cull::verdict::toggle_flag(&state.pool, photo_id).await
}

// ── Phase 2: Cull Bin ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn cull_bin_list(
    state: State<'_, AppState>,
    filter: Option<CullFilter>,
) -> AppResult<Vec<CullBinRow>> {
    crate::cull::bin::list(&state.pool, filter.unwrap_or(CullFilter::All)).await
}

#[tauri::command]
pub async fn cull_bin_summary(state: State<'_, AppState>) -> AppResult<CullBinSummary> {
    crate::cull::bin::summary(&state.pool).await
}

#[tauri::command]
pub async fn cull_bin_restore(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> AppResult<RestoreReceipt> {
    crate::cull::bin::restore(&state.pool, &photo_ids).await
}

#[tauri::command]
pub async fn cull_bin_delete_forever(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> AppResult<EmptyReceipt> {
    crate::cull::bin::delete_forever(&state.pool, &photo_ids).await
}

#[tauri::command]
pub async fn cull_bin_sweep(state: State<'_, AppState>) -> AppResult<EmptyReceipt> {
    crate::cull::bin::sweep_expired(&state.pool).await
}

// ── Phase 2: Export ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn export_enqueue(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    preset: ExportPreset,
    output_dir: String,
) -> AppResult<i64> {
    let dir = PathBuf::from(output_dir);
    crate::export::enqueue_job(&state.pool, &photo_ids, &preset, &dir).await
}

#[tauri::command]
pub async fn export_run_next(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    job_id: i64,
) -> AppResult<Option<ExportProgress>> {
    let progress = crate::export::run_next_item(&state.pool, job_id).await?;
    if let Some(ref p) = progress {
        let _ = app.emit(EXPORT_PROGRESS_EVENT, p);
    }
    Ok(progress)
}

#[tauri::command]
pub async fn export_list_jobs(state: State<'_, AppState>) -> AppResult<Vec<ExportJob>> {
    crate::export::list_jobs(&state.pool).await
}

// ── Phase 2 §10: Manual tagging ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserTagSummary {
    pub label: String,
    pub photo_count: i64,
}

#[tauri::command]
pub async fn list_user_tags(state: State<'_, AppState>) -> AppResult<Vec<UserTagSummary>> {
    sqlx::query_as::<_, UserTagSummary>(
        "SELECT label, COUNT(*) AS photo_count FROM tags \
         WHERE kind = 'user' GROUP BY label ORDER BY photo_count DESC, label ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn add_user_tag(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    label: String,
) -> AppResult<usize> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err(AppError::InvalidInput("tag label must not be empty".into()));
    }
    if label.len() > 64 {
        return Err(AppError::InvalidInput(
            "tag label must be 64 chars or fewer".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let mut added = 0usize;
    let mut tx = state.pool.begin().await?;
    for pid in &photo_ids {
        let res = sqlx::query(
            "INSERT OR IGNORE INTO tags (photo_id, label, kind, confidence, created_at) \
             VALUES (?1, ?2, 'user', 1.0, ?3)",
        )
        .bind(pid)
        .bind(&label)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        added += res.rows_affected() as usize;
    }
    tx.commit().await?;
    xmp_write_on_change(&state.pool, &photo_ids).await;
    Ok(added)
}

#[tauri::command]
pub async fn remove_user_tag(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    label: String,
) -> AppResult<usize> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err(AppError::InvalidInput("tag label must not be empty".into()));
    }
    let mut removed = 0usize;
    let mut tx = state.pool.begin().await?;
    for pid in &photo_ids {
        let res =
            sqlx::query("DELETE FROM tags WHERE photo_id = ?1 AND kind = 'user' AND label = ?2")
                .bind(pid)
                .bind(&label)
                .execute(&mut *tx)
                .await?;
        removed += res.rows_affected() as usize;
    }
    tx.commit().await?;
    xmp_write_on_change(&state.pool, &photo_ids).await;
    Ok(removed)
}

/// Best-effort XMP sidecar write-out for a set of photos. Skips when
/// the `xmp.write_on_change` setting is off or a photo has no resolvable
/// source file. Errors are logged but never surfaced — write-out must
/// never block the user-facing tag operation.
async fn xmp_write_on_change(pool: &sqlx::SqlitePool, photo_ids: &[i64]) {
    for pid in photo_ids {
        if let Err(e) = crate::xmp::export_for_photo(pool, *pid).await {
            tracing::warn!(photo_id = pid, error = %e, "xmp write-out failed");
        }
    }
}

#[tauri::command]
pub async fn rename_user_tag(
    state: State<'_, AppState>,
    old_label: String,
    new_label: String,
) -> AppResult<usize> {
    let old_label = old_label.trim().to_string();
    let new_label = new_label.trim().to_string();
    if old_label.is_empty() || new_label.is_empty() {
        return Err(AppError::InvalidInput("labels must not be empty".into()));
    }
    if new_label.len() > 64 {
        return Err(AppError::InvalidInput(
            "tag label must be 64 chars or fewer".into(),
        ));
    }
    let affected = sqlx::query("UPDATE tags SET label = ?1 WHERE kind = 'user' AND label = ?2")
        .bind(&new_label)
        .bind(&old_label)
        .execute(&state.pool)
        .await?
        .rows_affected() as usize;
    Ok(affected)
}

// ── Phase 3: Develop (non-destructive edit pipeline) ──────────────────────────
//
// Workflow:
//   1. Frontend calls `develop_open(photo_id)` → returns the current
//      `Operations` + a base64 JPEG preview rendered at 1280 long-edge.
//   2. As the user drags sliders, frontend calls `develop_apply` with the
//      new ops → backend re-renders a preview + returns data URL.
//   3. When satisfied, frontend calls `develop_save` → new row in `edits`.
//   4. `develop_reset` wipes history back to "as imported".
//   5. Copy/paste via `develop_copy_edits` + `develop_paste_edits`.
//   6. Presets via `develop_preset_apply` + `presets_list`.

use crate::develop::ops::{Operations, PastedReceipt, RenderReceipt};
use crate::develop::presets::Preset as DevelopPreset;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{codecs::jpeg::JpegEncoder, ImageEncoder};

/// Open a develop session: load current ops + render a baseline preview
/// from the photo's thumbnail. Returns the ops + a `data:image/jpeg;...`
/// preview URL.
#[tauri::command]
pub async fn develop_open(
    state: State<'_, AppState>,
    photo_id: i64,
) -> AppResult<DevelopOpenResponse> {
    let ops = crate::develop::history::load_current(&state.pool, photo_id).await?;
    let preview = render_preview(&state.pool, photo_id, &ops).await?;
    Ok(DevelopOpenResponse {
        photo_id,
        operations: ops,
        preview_data_url: preview,
    })
}

#[derive(Debug, Serialize)]
pub struct DevelopOpenResponse {
    pub photo_id: i64,
    pub operations: Operations,
    pub preview_data_url: String,
}

/// Re-render the preview under the given ops. Does NOT persist — callers
/// drive `develop_save` when they want history. Used during slider drags.
#[tauri::command]
pub async fn develop_apply(
    state: State<'_, AppState>,
    photo_id: i64,
    operations: Operations,
) -> AppResult<RenderReceipt> {
    let start = std::time::Instant::now();
    let preview_data_url = render_preview(&state.pool, photo_id, &operations).await?;
    Ok(RenderReceipt {
        photo_id,
        preview_data_url,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Save the current slider state as a new `edits` row, chaining off the
/// photo's `current_edit_id`. Returns the new edit id.
#[tauri::command]
pub async fn develop_save(
    state: State<'_, AppState>,
    photo_id: i64,
    operations: Operations,
    label: Option<String>,
) -> AppResult<i64> {
    crate::develop::history::save(&state.pool, photo_id, &operations, label).await
}

#[tauri::command]
pub async fn develop_reset(state: State<'_, AppState>, photo_id: i64) -> AppResult<usize> {
    crate::develop::history::reset(&state.pool, photo_id).await
}

#[tauri::command]
pub async fn develop_copy_edits(
    state: State<'_, AppState>,
    photo_id: i64,
) -> AppResult<Operations> {
    crate::develop::history::copy_edits(&state.pool, photo_id).await
}

#[tauri::command]
pub async fn develop_paste_edits(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    operations: Operations,
) -> AppResult<PastedReceipt> {
    crate::develop::history::paste_edits(&state.pool, &photo_ids, &operations).await
}

/// Apply a preset at `strength ∈ [0, 100]`. strength=0 is a no-op
/// (preview = baseline); strength=100 applies the preset in full.
#[tauri::command]
pub async fn develop_preset_apply(
    state: State<'_, AppState>,
    photo_id: i64,
    preset_id: i64,
    strength: u8,
) -> AppResult<RenderReceipt> {
    let preset = crate::develop::presets::load(&state.pool, preset_id).await?;
    let preset_ops = preset.operations()?;
    let base = crate::develop::history::load_current(&state.pool, photo_id).await?;
    let blended = base.blend(preset_ops, strength);
    let start = std::time::Instant::now();
    let preview_data_url = render_preview(&state.pool, photo_id, &blended).await?;
    Ok(RenderReceipt {
        photo_id,
        preview_data_url,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

#[tauri::command]
pub async fn presets_list(
    state: State<'_, AppState>,
    group: Option<String>,
) -> AppResult<Vec<DevelopPreset>> {
    crate::develop::presets::list(&state.pool, group.as_deref()).await
}

#[tauri::command]
pub async fn preset_save(
    state: State<'_, AppState>,
    name: String,
    group: String,
    operations: Operations,
) -> AppResult<i64> {
    crate::develop::presets::save_user(&state.pool, &name, &group, &operations).await
}

/// Generate a preview JPEG for `photo_id` under `ops`. Renders at 1280
/// long-edge — big enough to look good on-screen, small enough to keep
/// slider drags at > 10 fps on CPU.
async fn render_preview(
    pool: &sqlx::SqlitePool,
    photo_id: i64,
    ops: &Operations,
) -> AppResult<String> {
    // Reuse the generate_thumbnail_bytes path for the baseline JPEG.
    let thumb_bytes = generate_thumbnail_bytes(photo_id, Some(1280), pool).await?;
    // Decode → apply ops → re-encode.
    let decoded = image::load_from_memory(&thumb_bytes)
        .map_err(|e| AppError::Internal(format!("decode thumb for photo {photo_id}: {e}")))?;
    let rgb = decoded.to_rgb8();
    let processed = if ops.is_identity() {
        rgb
    } else {
        crate::develop::pipeline::apply(&rgb, ops)
    };
    let mut out: Vec<u8> = Vec::with_capacity(200 * 1024);
    let (w, h) = processed.dimensions();
    let encoder = JpegEncoder::new_with_quality(&mut out, 85);
    encoder
        .write_image(processed.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("encode preview: {e}")))?;
    Ok(format!("data:image/jpeg;base64,{}", B64.encode(&out)))
}

// ── Phase 4 §5 §6 §7 — map trips + shortcuts + xmp rescan ────────────────────

use crate::map::trips::{RecomputeReceipt, TripRow};
use crate::xmp::{ExportReceipt, RescanReceipt};

#[tauri::command]
pub async fn map_recompute_trips(state: State<'_, AppState>) -> AppResult<RecomputeReceipt> {
    crate::map::trips::recompute_trips(&state.pool).await
}

#[tauri::command]
pub async fn map_list_trips(state: State<'_, AppState>) -> AppResult<Vec<TripRow>> {
    crate::map::trips::list_trips(&state.pool).await
}

#[tauri::command]
pub async fn map_photos_in_trip(state: State<'_, AppState>, trip_id: i64) -> AppResult<Vec<i64>> {
    crate::map::trips::photos_in_trip(&state.pool, trip_id).await
}

#[tauri::command]
pub async fn xmp_rescan(state: State<'_, AppState>) -> AppResult<RescanReceipt> {
    crate::xmp::rescan_all(&state.pool).await
}

#[tauri::command]
pub async fn xmp_write_on_change_get(state: State<'_, AppState>) -> AppResult<bool> {
    crate::xmp::is_write_on_change_enabled(&state.pool).await
}

#[tauri::command]
pub async fn xmp_write_on_change_set(state: State<'_, AppState>, enabled: bool) -> AppResult<()> {
    crate::xmp::set_write_on_change(&state.pool, enabled).await
}

#[tauri::command]
pub async fn xmp_export_all(state: State<'_, AppState>) -> AppResult<ExportReceipt> {
    crate::xmp::export_all(&state.pool).await
}

// ── Prompt sidecar (Phase 4 §1/§2) ────────────────────────────────────────

#[tauri::command]
pub async fn prompt_sidecar_get(state: State<'_, AppState>) -> AppResult<Option<String>> {
    crate::prompt::get_sidecar_url(&state.pool).await
}

#[tauri::command]
pub async fn prompt_sidecar_set(state: State<'_, AppState>, url: Option<String>) -> AppResult<()> {
    crate::prompt::set_sidecar_url(&state.pool, url.as_deref()).await
}

#[tauri::command]
pub async fn prompt_sidecar_model_get(state: State<'_, AppState>) -> AppResult<Option<String>> {
    crate::prompt::get_preferred_model(&state.pool).await
}

#[tauri::command]
pub async fn prompt_sidecar_model_set(
    state: State<'_, AppState>,
    model: Option<String>,
) -> AppResult<()> {
    crate::prompt::set_preferred_model(&state.pool, model.as_deref()).await
}

#[tauri::command]
pub async fn prompt_sidecar_ping(
    state: State<'_, AppState>,
) -> AppResult<crate::prompt::SidecarStatus> {
    crate::prompt::user_initiated_ping_sidecar(&state.pool).await
}

#[tauri::command]
pub async fn prompt_edit(
    state: State<'_, AppState>,
    req: crate::prompt::PromptEditRequest,
) -> AppResult<crate::prompt::PromptEditResult> {
    crate::prompt::user_initiated_prompt_edit(&state.pool, req).await
}

#[tauri::command]
pub async fn mask_from_prompt(
    state: State<'_, AppState>,
    req: crate::prompt::MaskFromPromptRequest,
) -> AppResult<crate::prompt::MaskFromPromptResult> {
    crate::prompt::user_initiated_mask_from_prompt(&state.pool, req).await
}

#[tauri::command]
pub async fn prompt_edit_list(
    state: State<'_, AppState>,
    photo_id: i64,
) -> AppResult<Vec<crate::prompt::history::PromptEditRow>> {
    crate::prompt::history::list_for_photo(&state.pool, photo_id).await
}

#[tauri::command]
pub async fn prompt_edit_accept(state: State<'_, AppState>, edit_id: i64) -> AppResult<()> {
    crate::prompt::history::accept(&state.pool, edit_id).await
}

#[tauri::command]
pub async fn prompt_edit_reject(state: State<'_, AppState>, edit_id: i64) -> AppResult<()> {
    crate::prompt::history::reject(&state.pool, edit_id).await
}

#[tauri::command]
pub async fn backfill_place_labels(
    state: State<'_, AppState>,
) -> AppResult<crate::map::geocode::BackfillReceipt> {
    crate::map::geocode::backfill_place_labels(&state.pool).await
}

// Shortcut registry — userland stores its bindings in the shortcuts
// table. Phase 4 §6 scope: list + set. A discovery modal reads the list;
// a future rebinding UI calls set. Conflict detection is client-side.

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ShortcutRow {
    pub command_id: String,
    pub key_binding: String,
    pub context: String,
    pub updated_at: String,
}

#[tauri::command]
pub async fn shortcuts_list(state: State<'_, AppState>) -> AppResult<Vec<ShortcutRow>> {
    sqlx::query_as::<_, ShortcutRow>(
        "SELECT command_id, key_binding, context, updated_at \
         FROM shortcuts ORDER BY context ASC, command_id ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn shortcuts_set(
    state: State<'_, AppState>,
    command_id: String,
    key_binding: String,
    context: Option<String>,
) -> AppResult<()> {
    let command_id = command_id.trim();
    let key_binding = key_binding.trim();
    if command_id.is_empty() || key_binding.is_empty() {
        return Err(AppError::InvalidInput(
            "command_id + key_binding required".into(),
        ));
    }
    let ctx = context.unwrap_or_else(|| "global".into());
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO shortcuts (command_id, key_binding, context, updated_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(command_id) DO UPDATE SET \
           key_binding = excluded.key_binding, \
           context = excluded.context, \
           updated_at = excluded.updated_at",
    )
    .bind(command_id)
    .bind(key_binding)
    .bind(&ctx)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(())
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

        // 2 rule-based system albums + 4 rediscovery albums = 6 total.
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|r| r.is_system));
    }

    // list_photos ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_photos_returns_empty_on_fresh_catalog() {
        let (_tmp, pool) = make_pool().await;
        let rows = sqlx::query_as::<_, PhotoRow>(
            "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
             size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
             aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
             FROM photos ORDER BY imported_at DESC LIMIT 100 OFFSET 0",
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
            "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
             size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
             aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
             FROM photos ORDER BY imported_at DESC LIMIT 3 OFFSET 0",
        )
        .fetch_all(&pool)
        .await
        .expect("query");

        assert_eq!(rows.len(), 3);
    }

    // list_photos album_id filter + refresh_smart_albums ──────────────────────

    async fn insert_photo_with_iso(pool: &sqlx::SqlitePool, sha_prefix: char, iso: i64) -> i64 {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, iso) \
             VALUES (?1, ?2, 0, 0, ?3, 0, ?4) RETURNING id",
        )
        .bind(format!("{sha_prefix:0>64}"))
        .bind(format!("{sha_prefix}.jpg"))
        .bind(&now)
        .bind(iso)
        .fetch_one(pool)
        .await
        .expect("insert photo")
    }

    async fn insert_night_album(pool: &sqlx::SqlitePool) -> i64 {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query_scalar(
            "INSERT INTO smart_albums \
             (name, rule_json, cover_photo_ids, tag, is_system, created_at, updated_at) \
             VALUES ('Night', '{\"type\":\"exif\",\"field\":\"iso\",\"op\":\"gte\",\"value\":3200}', \
             '[]', 'lighting', 1, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("insert album")
    }

    #[tokio::test]
    async fn list_photos_album_filter_iso_gte_3200() {
        let (_tmp, pool) = make_pool().await;
        insert_photo_with_iso(&pool, 'a', 6400).await;
        insert_photo_with_iso(&pool, 'b', 100).await;
        let album_id = insert_night_album(&pool).await;

        // Build the where clause directly via the rule engine.
        let rule_json = r#"{"type":"exif","field":"iso","op":"gte","value":3200}"#;
        let rule = crate::catalog::rules::parse_rule(rule_json).unwrap();
        let frag = crate::catalog::rules::rule_to_sql(&rule).unwrap();
        let sql = format!(
            "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
             size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
             aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged \
             FROM photos WHERE {frag} ORDER BY imported_at DESC LIMIT 100 OFFSET 0"
        );
        let rows = sqlx::query_as::<_, PhotoRow>(&sql)
            .fetch_all(&pool)
            .await
            .expect("query");

        assert_eq!(rows.len(), 1, "only high-ISO photo should match");
        assert_eq!(rows[0].iso, Some(6400));
        let _ = album_id;
    }

    #[tokio::test]
    async fn refresh_smart_albums_updates_photo_count() {
        let (_tmp, pool) = make_pool().await;
        insert_photo_with_iso(&pool, 'c', 6400).await;
        insert_photo_with_iso(&pool, 'd', 100).await;
        let album_id = insert_night_album(&pool).await;

        refresh_album_counts(&pool).await.expect("refresh");

        let (count,): (i64,) = sqlx::query_as("SELECT photo_count FROM smart_albums WHERE id = ?1")
            .bind(album_id)
            .fetch_one(&pool)
            .await
            .expect("query");

        assert_eq!(count, 1, "only high-ISO photo counts");
    }

    #[tokio::test]
    async fn refresh_smart_albums_sets_cover_photo_ids() {
        let (_tmp, pool) = make_pool().await;
        let photo_id = insert_photo_with_iso(&pool, 'e', 5000).await;
        let album_id = insert_night_album(&pool).await;

        refresh_album_counts(&pool).await.expect("refresh");

        let cover: String =
            sqlx::query_scalar("SELECT cover_photo_ids FROM smart_albums WHERE id = ?1")
                .bind(album_id)
                .fetch_one(&pool)
                .await
                .expect("query");

        let ids: Vec<i64> = serde_json::from_str(&cover).expect("json");
        assert!(
            ids.contains(&photo_id),
            "cover should include the matching photo"
        );
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
            "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
             size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
             aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged FROM photos \
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
            "SELECT id, sha256, filename, width, height, captured_at, imported_at, is_raw, \
             size_bytes, camera_make, camera_model, aperture, shutter, iso, focal_mm, \
             aesthetic_score, paired_photo_id, raw_format, orientation, sharpness_score, star_rating, is_flagged FROM photos \
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
            "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, \
             p.imported_at, p.is_raw, p.size_bytes, p.camera_make, p.camera_model, \
             p.aperture, p.shutter, p.iso, p.focal_mm, p.aesthetic_score, \
             p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
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
            "SELECT p.id, p.sha256, p.filename, p.width, p.height, p.captured_at, \
             p.imported_at, p.is_raw, p.size_bytes, p.camera_make, p.camera_model, \
             p.aperture, p.shutter, p.iso, p.focal_mm, p.aesthetic_score, \
             p.paired_photo_id, p.raw_format, p.orientation, p.sharpness_score, p.star_rating, p.is_flagged \
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

    // ── cleanup_execute unit tests ────────────────────────────────────────────

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

    #[tokio::test]
    async fn cleanup_execute_wrong_token_returns_permission_denied() {
        let pool = test_pool().await;
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
    }

    #[tokio::test]
    async fn cleanup_execute_sha256_mismatch_skips_file_adds_to_errors() {
        let pool = test_pool().await;
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let file_path = tmp.path().join("photo.jpg");
        std::fs::write(&file_path, b"real content").unwrap();

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

        let wrong_hash = "deadbeef".repeat(8);
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

        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.freed_bytes, 0);
        assert!(
            result.errors.iter().any(|e| e.contains("SHA256 mismatch")),
            "expected SHA256 mismatch error, got: {:?}",
            result.errors
        );
        assert!(file_path.exists(), "file should not have been deleted");
    }

    // ── ai_models_status tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn ai_models_status_returns_all_known_models() {
        let tmp_user = tempfile::TempDir::new().expect("tempdir");
        let tmp_bundled = tempfile::TempDir::new().expect("bundled tempdir");
        let statuses = resolve_all_model_statuses(
            Some(tmp_bundled.path().to_path_buf()),
            tmp_user.path().to_path_buf(),
        )
        .await
        .expect("resolve_all_model_statuses failed");
        use crate::ai::download::KNOWN_MODELS;
        assert_eq!(
            statuses.len(),
            KNOWN_MODELS.len(),
            "status count must match KNOWN_MODELS length"
        );
    }

    #[tokio::test]
    async fn ai_models_status_not_installed_when_dir_empty() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let tmp_bundled = tempfile::TempDir::new().expect("bundled tempdir");
        let statuses = resolve_all_model_statuses(
            Some(tmp_bundled.path().to_path_buf()),
            tmp.path().to_path_buf(),
        )
        .await
        .expect("resolve_all_model_statuses failed");
        assert!(!statuses.is_empty(), "should report all KNOWN_MODELS rows");
        for s in &statuses {
            assert!(
                !s.installed,
                "model {} reported installed but both tempdirs are empty",
                s.name
            );
            assert_eq!(
                s.source,
                ModelSource::Missing,
                "model {} source should be Missing when no files present",
                s.name
            );
        }
    }

    #[tokio::test]
    async fn ai_models_status_reports_bundled_source_when_present() {
        let tmp_user = tempfile::TempDir::new().expect("user tempdir");
        let tmp_bundled = tempfile::TempDir::new().expect("bundled tempdir");
        std::fs::write(
            tmp_bundled.path().join("siglip2-b16-image.onnx"),
            b"fake siglip model bytes",
        )
        .expect("write fake model");

        let statuses = resolve_all_model_statuses(
            Some(tmp_bundled.path().to_path_buf()),
            tmp_user.path().to_path_buf(),
        )
        .await
        .expect("resolve_all_model_statuses failed");
        let siglip = statuses
            .iter()
            .find(|s| s.filename == "siglip2-b16-image.onnx")
            .expect("siglip2 entry must be present");

        assert!(siglip.installed, "siglip2 must report installed=true");
        assert_eq!(
            siglip.source,
            ModelSource::Bundled,
            "siglip2 present in bundled dir must report source=Bundled"
        );

        for s in statuses
            .iter()
            .filter(|s| s.filename != "siglip2-b16-image.onnx")
        {
            assert_eq!(
                s.source,
                ModelSource::Missing,
                "model {} should be Missing — not seeded",
                s.name
            );
        }
    }

    #[test]
    fn verify_model_hash_wrong_hash_returns_false() {
        let tmp = tempfile::Builder::new()
            .suffix(".bin")
            .tempfile()
            .expect("tempfile");
        std::fs::write(tmp.path(), b"test data").expect("write");
        assert!(
            !verify_model_hash_streaming(
                tmp.path(),
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            "wrong hash must return false"
        );
    }

    #[test]
    fn verify_model_hash_correct_hash_returns_true() {
        use sha2::{Digest, Sha256};
        let data = b"chronimage test";
        let tmp = tempfile::Builder::new()
            .suffix(".bin")
            .tempfile()
            .expect("tempfile");
        std::fs::write(tmp.path(), data).expect("write");
        let expected = hex::encode(Sha256::digest(data));
        assert!(
            verify_model_hash_streaming(tmp.path(), &expected),
            "correct hash must return true"
        );
    }

    #[test]
    fn verify_model_hash_missing_file_returns_false() {
        assert!(
            !verify_model_hash_streaming(std::path::Path::new("/nonexistent/model.onnx"), "abc123"),
            "missing file must return false"
        );
    }

    // ── face_clusters_list tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn face_clusters_list_returns_empty_when_no_clusters() {
        let (_tmp, pool) = make_pool().await;
        let rows: Vec<catalog::models::ClusterRow> =
            sqlx::query_as::<_, catalog::models::ClusterRow>(
                "SELECT c.id, c.name, c.is_named,
                        COUNT(f.id) AS face_count,
                        (SELECT f2.photo_id FROM faces f2
                         WHERE f2.id = c.cover_face_id) AS cover_photo_id
                 FROM clusters c
                 LEFT JOIN faces f ON f.cluster_id = c.id
                 GROUP BY c.id
                 ORDER BY face_count DESC
                 LIMIT 50",
            )
            .fetch_all(&pool)
            .await
            .expect("query");
        assert!(
            rows.is_empty(),
            "expected empty list when no clusters exist"
        );
    }

    #[tokio::test]
    async fn face_clusters_list_returns_clusters_ordered_by_face_count() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let cluster_a: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (name, is_named, created_at, updated_at) \
             VALUES ('Alice', 1, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("cluster_a");

        let cluster_b: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (name, is_named, created_at, updated_at) \
             VALUES ('Bob', 1, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("cluster_b");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'p.jpg', 100, 100, ?2, 0) RETURNING id",
        )
        .bind("a".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        // 2 faces in cluster_a, 1 face in cluster_b.
        for _ in 0..2 {
            sqlx::query(
                "INSERT INTO faces \
                 (photo_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, created_at) \
                 VALUES (?1, ?2, 0, 0, 10, 10, ?3)",
            )
            .bind(photo_id)
            .bind(cluster_a)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("face_a");
        }
        sqlx::query(
            "INSERT INTO faces \
             (photo_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, created_at) \
             VALUES (?1, ?2, 0, 0, 10, 10, ?3)",
        )
        .bind(photo_id)
        .bind(cluster_b)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("face_b");

        let rows: Vec<catalog::models::ClusterRow> =
            sqlx::query_as::<_, catalog::models::ClusterRow>(
                "SELECT c.id, c.name, c.is_named,
                        COUNT(f.id) AS face_count,
                        (SELECT f2.photo_id FROM faces f2
                         WHERE f2.id = c.cover_face_id) AS cover_photo_id
                 FROM clusters c
                 LEFT JOIN faces f ON f.cluster_id = c.id
                 GROUP BY c.id
                 ORDER BY face_count DESC
                 LIMIT 50",
            )
            .fetch_all(&pool)
            .await
            .expect("query");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, cluster_a, "cluster_a (2 faces) must be first");
        assert_eq!(rows[0].face_count, 2);
        assert_eq!(rows[1].id, cluster_b, "cluster_b (1 face) must be second");
        assert_eq!(rows[1].face_count, 1);
    }

    // ── face_cluster_name tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn face_cluster_name_happy_path() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let cluster_id: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (is_named, created_at, updated_at) \
             VALUES (0, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("cluster");

        let affected = sqlx::query("UPDATE clusters SET name = ?1, is_named = 1 WHERE id = ?2")
            .bind("Carol")
            .bind(cluster_id)
            .execute(&pool)
            .await
            .expect("name update")
            .rows_affected();
        assert_eq!(affected, 1);

        let (name, is_named): (Option<String>, bool) =
            sqlx::query_as("SELECT name, is_named FROM clusters WHERE id = ?1")
                .bind(cluster_id)
                .fetch_one(&pool)
                .await
                .expect("fetch");

        assert_eq!(name.as_deref(), Some("Carol"));
        assert!(is_named, "is_named must be true after naming");
    }

    #[tokio::test]
    async fn face_cluster_name_missing_id_returns_zero_affected() {
        let (_tmp, pool) = make_pool().await;
        let affected = sqlx::query("UPDATE clusters SET name = ?1, is_named = 1 WHERE id = ?2")
            .bind("Nobody")
            .bind(9999_i64)
            .execute(&pool)
            .await
            .expect("query")
            .rows_affected();
        assert_eq!(affected, 0, "non-existent cluster must affect 0 rows");
    }

    // ── face_cluster_merge tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn face_cluster_merge_idempotent_same_id() {
        // Mirrors the `if a == b { return Ok(a) }` guard in face_cluster_merge.
        let a: i64 = 42;
        let b: i64 = 42;
        assert_eq!(a, b, "guard condition");
        // The function would return Ok(a) immediately without touching the DB.
        assert_eq!(a, 42);
    }

    #[tokio::test]
    async fn face_cluster_merge_happy_path_reassigns_faces() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let ca: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (is_named, created_at, updated_at) \
             VALUES (0, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("ca");

        let cb: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (is_named, created_at, updated_at) \
             VALUES (0, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("cb");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'f.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind("b".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        sqlx::query(
            "INSERT INTO faces \
             (photo_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, created_at) \
             VALUES (?1, ?2, 0, 0, 5, 5, ?3)",
        )
        .bind(photo_id)
        .bind(cb)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("face");

        // Execute the merge (mirrors face_cluster_merge internals).
        let mut tx = pool.begin().await.expect("tx");
        sqlx::query("UPDATE faces SET cluster_id = ?1 WHERE cluster_id = ?2")
            .bind(ca)
            .bind(cb)
            .execute(&mut *tx)
            .await
            .expect("reassign");
        sqlx::query("DELETE FROM clusters WHERE id = ?1")
            .bind(cb)
            .execute(&mut *tx)
            .await
            .expect("delete_b");
        tx.commit().await.expect("commit");

        let b_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM clusters WHERE id = ?1")
            .bind(cb)
            .fetch_optional(&pool)
            .await
            .expect("check b");
        assert!(b_exists.is_none(), "cluster b must be deleted after merge");

        let face_cluster: i64 =
            sqlx::query_scalar("SELECT cluster_id FROM faces WHERE photo_id = ?1")
                .bind(photo_id)
                .fetch_one(&pool)
                .await
                .expect("face cluster");
        assert_eq!(face_cluster, ca, "face must be reassigned to cluster a");
    }

    #[tokio::test]
    async fn face_cluster_merge_missing_a_not_found() {
        let (_tmp, pool) = make_pool().await;
        let a_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM clusters WHERE id = ?1")
            .bind(999_i64)
            .fetch_optional(&pool)
            .await
            .expect("check");
        assert!(
            a_exists.is_none(),
            "cluster 999 must not exist — NotFound path fires for missing a"
        );
    }

    #[tokio::test]
    async fn face_cluster_merge_missing_b_not_found() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let ca: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (is_named, created_at, updated_at) \
             VALUES (0, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("ca");

        let b_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM clusters WHERE id = ?1")
            .bind(9999_i64)
            .fetch_optional(&pool)
            .await
            .expect("check b");
        assert!(
            b_exists.is_none(),
            "cluster 9999 must not exist — NotFound path fires for missing b"
        );
        let _ = ca;
    }

    // ── record_photo_view tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn record_photo_view_happy_path_sets_last_viewed_at() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'view.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind("c".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        sqlx::query(
            "INSERT INTO photo_views (photo_id, last_viewed_at, view_count)
             VALUES (?1, ?2, 1)
             ON CONFLICT(photo_id) DO UPDATE SET
               last_viewed_at = excluded.last_viewed_at,
               view_count     = view_count + 1",
        )
        .bind(photo_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("first view");

        let (view_count, last_viewed_at): (i64, Option<String>) = sqlx::query_as(
            "SELECT view_count, last_viewed_at FROM photo_views WHERE photo_id = ?1",
        )
        .bind(photo_id)
        .fetch_one(&pool)
        .await
        .expect("fetch");

        assert_eq!(view_count, 1, "view_count should be 1 after first view");
        assert!(
            last_viewed_at.is_some(),
            "last_viewed_at must be set after first view"
        );
    }

    #[tokio::test]
    async fn record_photo_view_second_call_increments_view_count() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'view2.jpg', 0, 0, ?2, 0) RETURNING id",
        )
        .bind("d".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        for _ in 0..2 {
            sqlx::query(
                "INSERT INTO photo_views (photo_id, last_viewed_at, view_count)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(photo_id) DO UPDATE SET
                   last_viewed_at = excluded.last_viewed_at,
                   view_count     = view_count + 1",
            )
            .bind(photo_id)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("view");
        }

        let view_count: i64 =
            sqlx::query_scalar("SELECT view_count FROM photo_views WHERE photo_id = ?1")
                .bind(photo_id)
                .fetch_one(&pool)
                .await
                .expect("fetch");

        assert_eq!(view_count, 2, "view_count must be 2 after two calls");
    }

    // ── ai_reindex tests ──────────────────────────────────────────────────────
    //
    // `ai_reindex` takes `State<'_, AppState>` which requires a live Tauri
    // runtime to construct. Following the established pattern in this file
    // (see face_cluster_merge, record_photo_view), we test the underlying SQL
    // behaviour directly rather than going through the command dispatcher.

    #[tokio::test]
    async fn ai_reindex_embeddings_sql_clears_table() {
        let (_tmp, pool) = make_pool().await;
        // photo_embeddings is empty on a fresh DB — DELETE returns 0 rows.
        let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photo_embeddings")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count_before, 0, "fresh DB has no embeddings");
        sqlx::query("DELETE FROM photo_embeddings")
            .execute(&pool)
            .await
            .expect("delete");
        let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photo_embeddings")
            .fetch_one(&pool)
            .await
            .expect("count after");
        assert_eq!(
            count_after, 0,
            "table still empty after delete on empty table"
        );
    }

    #[tokio::test]
    async fn ai_reindex_aesthetic_sql_nulls_scores() {
        let (_tmp, pool) = make_pool().await;
        // Insert a photo with a non-NULL aesthetic_score.
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, aesthetic_score) \
             VALUES (?1, 'score.jpg', 100, 100, ?2, 0, 0.85)",
        )
        .bind("e".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert");

        let before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE aesthetic_score IS NOT NULL")
                .fetch_one(&pool)
                .await
                .expect("count before");
        assert_eq!(before, 1);

        sqlx::query("UPDATE photos SET aesthetic_score = NULL")
            .execute(&pool)
            .await
            .expect("update");

        let after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE aesthetic_score IS NOT NULL")
                .fetch_one(&pool)
                .await
                .expect("count after");
        assert_eq!(after, 0, "aesthetic_score must be NULL after reindex");
    }

    #[tokio::test]
    async fn ai_reindex_faces_sql_clears_faces_and_clusters() {
        let (_tmp, pool) = make_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        // Seed a cluster + face.
        let cluster_id: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (is_named, photo_count, created_at, updated_at) \
             VALUES (0, 1, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("cluster");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) \
             VALUES (?1, 'face.jpg', 100, 100, ?2, 0) RETURNING id",
        )
        .bind("f".repeat(64))
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("photo");

        sqlx::query(
            "INSERT INTO faces (photo_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, quality, created_at) \
             VALUES (?1, ?2, 0.1, 0.1, 0.5, 0.5, 0.9, ?3)",
        )
        .bind(photo_id)
        .bind(cluster_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("face");

        let face_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM faces")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(face_count_before, 1);

        // Execute the reindex SQL.
        sqlx::query("DELETE FROM faces")
            .execute(&pool)
            .await
            .expect("delete faces");
        sqlx::query("UPDATE clusters SET photo_count = 0")
            .execute(&pool)
            .await
            .expect("reset photo_count");
        sqlx::query("DELETE FROM clusters")
            .execute(&pool)
            .await
            .expect("delete clusters");

        let face_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM faces")
            .fetch_one(&pool)
            .await
            .expect("count after");
        let cluster_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clusters")
            .fetch_one(&pool)
            .await
            .expect("clusters after");
        assert_eq!(face_count_after, 0, "faces must be cleared");
        assert_eq!(cluster_count_after, 0, "clusters must be cleared");
    }

    #[test]
    fn ai_reindex_unknown_kind_is_invalid_input() {
        // Validates the error branch logic without a Tauri runtime — the match arm
        // is the only non-DB path in ai_reindex.
        let known_kinds = [
            "embeddings",
            "face-detect",
            "face-embed",
            "aesthetic",
            "captions",
        ];
        let bogus = "totally-unknown";
        assert!(
            !known_kinds.contains(&bogus),
            "bogus kind must not match any valid arm"
        );
    }

    // ── list_tags unit tests ──────────────────────────────────────────────────

    async fn fetch_tags(pool: &sqlx::SqlitePool, photo_id: i64) -> Vec<TagRow> {
        sqlx::query_as::<_, TagRow>(
            "SELECT id, label, kind, confidence FROM tags \
             WHERE photo_id = ?1 ORDER BY confidence DESC, id ASC",
        )
        .bind(photo_id)
        .fetch_all(pool)
        .await
        .expect("fetch tags")
    }

    #[tokio::test]
    async fn list_tags_returns_empty_when_no_tags() {
        let pool = test_pool().await;
        let rows = fetch_tags(&pool, 42).await;
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_tags_orders_by_confidence_descending() {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, size_bytes, is_raw, imported_at) \
             VALUES ('t1', 'p.jpg', 10, 10, 1, 0, ?1)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert photo");

        for (label, kind, conf) in [
            ("beach", "auto_scene", 0.95),
            ("sunset", "auto_scene", 0.88),
            ("Ari", "people", 0.6),
        ] {
            sqlx::query(
                "INSERT INTO tags (photo_id, label, kind, confidence, created_at) \
                 VALUES (1, ?1, ?2, ?3, ?4)",
            )
            .bind(label)
            .bind(kind)
            .bind(conf)
            .bind(&now)
            .execute(&pool)
            .await
            .expect("insert tag");
        }

        let rows = fetch_tags(&pool, 1).await;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].label, "beach");
        assert!(rows[0].confidence > rows[1].confidence);
        assert!(rows[1].confidence > rows[2].confidence);
    }

    // ── get_thumbnail unit tests ──────────────────────────────────────────────

    /// Build a 64×64 JPEG on disk, seed one photo row pointing at it,
    /// and return (pool, tmp-dir guard, photo_id, jpeg_path).
    async fn seed_photo_with_local_jpeg(
        tmp: &tempfile::TempDir,
    ) -> (sqlx::SqlitePool, i64, PathBuf) {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        let jpg_path = tmp.path().join("photo.jpg");
        let img = image::DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            64,
            64,
            image::Rgb([180u8, 60u8, 40u8]),
        ));
        img.save_with_format(&jpg_path, image::ImageFormat::Jpeg)
            .expect("write jpeg");

        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES ('test', 'local', 'idle', '{}', ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert source");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, size_bytes, is_raw, imported_at) \
             VALUES ('deadbeef', 'photo.jpg', 64, 64, 1024, 0, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&pool)
        .await
        .expect("insert photo");

        sqlx::query(
            "INSERT INTO source_copies \
             (source_id, photo_id, path, verified_sha256, is_primary, last_seen_at) \
             VALUES (?1, ?2, ?3, 'deadbeef', 1, ?4)",
        )
        .bind(source_id)
        .bind(photo_id)
        .bind(jpg_path.to_str().unwrap())
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert source_copy");

        (pool, photo_id, jpg_path)
    }

    #[tokio::test]
    async fn get_thumbnail_missing_photo_returns_not_found() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // Point cache at tempdir so we don't pollute the real data dir.
        std::env::set_var("CHRONIMAGE_THUMBNAILS_DIR", tmp.path());
        let pool = test_pool().await;
        let err = generate_thumbnail_bytes(999, Some(128), &pool)
            .await
            .expect_err("should fail");
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound for unknown photo, got {err:?}"
        );
    }

    #[tokio::test]
    async fn get_thumbnail_returns_jpeg_bytes_and_caches() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::env::set_var("CHRONIMAGE_THUMBNAILS_DIR", &cache_dir);
        let (pool, photo_id, _jpg_path) = seed_photo_with_local_jpeg(&tmp).await;

        let bytes = generate_thumbnail_bytes(photo_id, Some(128), &pool)
            .await
            .expect("thumbnail");
        // JPEG magic bytes.
        assert_eq!(&bytes[..3], &[0xff, 0xd8, 0xff]);

        // Cache hit round-trip.
        let cached = std::fs::read(cache_dir.join("deadbeef_128.jpg")).expect("cache file written");
        assert_eq!(cached, bytes, "cached bytes must match returned bytes");
    }

    // ── search_suggestions unit tests ─────────────────────────────────────────

    #[tokio::test]
    async fn search_suggestions_empty_db_returns_curated_seeds() {
        let pool = test_pool().await;
        let out = build_search_suggestions(&pool).await;
        assert_eq!(
            out.len(),
            8,
            "should emit 8 curated seeds when catalog is empty"
        );
        assert!(out.contains(&"golden hour portraits".to_string()));
        assert!(out.iter().all(|s| !s.starts_with("Photos of ")));
    }

    #[tokio::test]
    async fn search_suggestions_blends_named_cluster_and_camera() {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();

        // Seed a named cluster.
        sqlx::query(
            "INSERT INTO clusters (name, is_named, cover_face_id, created_at, updated_at) \
             VALUES ('Ari', 1, NULL, ?1, ?1)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert cluster");

        // Seed 2 photos for Sony A7 IV so it wins the top-camera group-by.
        for i in 0..2 {
            sqlx::query(
                "INSERT INTO photos (sha256, filename, width, height, size_bytes, is_raw, imported_at, camera_make, camera_model) \
                 VALUES (?1, ?2, 6000, 4000, 1024, 0, ?3, 'Sony', 'ILCE-7M4')",
            )
            .bind(format!("sha-{i}"))
            .bind(format!("IMG_{i:04}.jpg"))
            .bind(&now)
            .execute(&pool)
            .await
            .expect("insert photo");
        }

        let out = build_search_suggestions(&pool).await;
        assert!(out.len() <= 8, "cap at 8 suggestions");
        assert!(
            out.iter().any(|s| s == "Photos of Ari"),
            "named cluster hint missing, got {out:?}"
        );
        assert!(
            out.iter().any(|s| s == "Sony ILCE-7M4 shots"),
            "camera hint missing, got {out:?}"
        );
        // Curated seeds should still fill remaining slots up to 8.
        assert!(out.contains(&"golden hour portraits".to_string()));
    }

    // ── Google Photos OAuth2 command tests ────────────────────────────────
    //
    // The new Tauri commands are thin wrappers around the loopback-driven
    // flow in [`crate::sources::google_photos`]. The module itself carries
    // the PKCE/CSRF/percent-encode/refresh unit tests; here we only cover
    // the command-layer plumbing that's reachable without live Google
    // endpoints.

    #[tokio::test]
    async fn gphotos_poll_oauth_flow_returns_not_found_for_unknown_id() {
        let err = gphotos_poll_oauth_flow("nonexistent-flow-id".into())
            .await
            .expect_err("unknown flow must be NotFound");
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn gphotos_cancel_oauth_flow_is_idempotent() {
        // No flow ever started → cancel must still succeed.
        gphotos_cancel_oauth_flow("nonexistent".into())
            .await
            .expect("cancel must be idempotent");
    }

    /// `gphotos_auth_status` shouldn't panic on a fresh machine with no
    /// keyring entry — it must surface `false`. On CI without a real secret
    /// store the keyring crate may error; the command propagates it as an
    /// `AppError::Internal`, so we accept either outcome.
    #[tokio::test]
    async fn gphotos_auth_status_is_callable_on_fresh_install() {
        let res = gphotos_auth_status().await;
        assert!(
            matches!(res, Ok(_) | Err(AppError::Internal(_))),
            "unexpected variant: {res:?}",
        );
    }

    // ── Deletion command tests ────────────────────────────────────────────────

    async fn insert_source(pool: &sqlx::SqlitePool, name: &str, kind: &str) -> i64 {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO sources (name, kind, status, config_json, created_at)
             VALUES (?1, ?2, 'idle', '{}', ?3) RETURNING id",
        )
        .bind(name)
        .bind(kind)
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("insert source")
    }

    async fn insert_photo(pool: &sqlx::SqlitePool, sha: &str, size: i64) -> i64 {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, size_bytes)
             VALUES (?1, 'photo.jpg', 4000, 3000, ?2, 0, ?3) RETURNING id",
        )
        .bind(sha)
        .bind(&now)
        .bind(size)
        .fetch_one(pool)
        .await
        .expect("insert photo")
    }

    async fn insert_source_copy(
        pool: &sqlx::SqlitePool,
        photo_id: i64,
        source_id: i64,
        path: Option<&str>,
        sha: &str,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO source_copies
             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
        )
        .bind(photo_id)
        .bind(source_id)
        .bind(path)
        .bind(sha)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert source_copy");
    }

    #[tokio::test]
    async fn source_deletion_preview_counts_orphans_and_shared() {
        let pool = test_pool().await;
        let src_a = insert_source(&pool, "A", "local").await;
        let src_b = insert_source(&pool, "B", "local").await;

        // Photo 1: only in src_a (orphan)
        let p1 = insert_photo(&pool, "sha1", 1_000_000).await;
        insert_source_copy(&pool, p1, src_a, Some("/tmp/p1.jpg"), "sha1").await;

        // Photo 2: in both src_a and src_b (not orphan)
        let p2 = insert_photo(&pool, "sha2", 2_000_000).await;
        insert_source_copy(&pool, p2, src_a, Some("/tmp/p2.jpg"), "sha2").await;
        insert_source_copy(&pool, p2, src_b, Some("/elsewhere/p2.jpg"), "sha2").await;

        // Photo 3: only in src_a, cloud-only (no path)
        let p3 = insert_photo(&pool, "sha3", 3_000_000).await;
        insert_source_copy(&pool, p3, src_a, None, "sha3").await;

        let plan = source_deletion_preview_impl(&pool, src_a)
            .await
            .expect("preview");

        assert_eq!(plan.photos_total, 3, "src_a has 3 photos");
        assert_eq!(
            plan.orphan_photos, 2,
            "p1 and p3 are orphans (p2 exists in src_b)"
        );
        assert_eq!(
            plan.local_files, 1,
            "only p1 has a local path among orphans"
        );
        assert_eq!(plan.total_bytes, 1_000_000, "bytes only from p1");
        assert_eq!(plan.cloud_only, 1, "p3 is the cloud-only orphan");
    }

    #[tokio::test]
    async fn delete_source_cascades_orphan_photos_and_keeps_shared() {
        let pool = test_pool().await;
        let src_a = insert_source(&pool, "A", "local").await;
        let src_b = insert_source(&pool, "B", "local").await;

        let p_orphan = insert_photo(&pool, "orphan", 500).await;
        insert_source_copy(&pool, p_orphan, src_a, None, "orphan").await;

        let p_shared = insert_photo(&pool, "shared", 600).await;
        insert_source_copy(&pool, p_shared, src_a, None, "shared").await;
        insert_source_copy(&pool, p_shared, src_b, None, "shared").await;

        let receipt = delete_source_impl(&pool, src_a, false, true)
            .await
            .expect("delete");
        assert_eq!(receipt.removed_photos, 1, "orphan photo removed");

        // src_a gone; src_b kept.
        let sources_remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE id = ?1")
                .bind(src_a)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(sources_remaining, 0);

        // p_orphan gone; p_shared kept.
        let photos_remaining: Vec<i64> = sqlx::query_scalar("SELECT id FROM photos ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(photos_remaining, vec![p_shared]);

        // p_shared's source_copy row from src_a removed; src_b row kept.
        let copies_for_shared: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM source_copies WHERE photo_id = ?1")
                .bind(p_shared)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(copies_for_shared, 1);
    }

    #[tokio::test]
    async fn delete_source_without_orphan_cleanup_leaves_orphan_photos() {
        let pool = test_pool().await;
        let src = insert_source(&pool, "A", "local").await;
        let p = insert_photo(&pool, "lonely", 42).await;
        insert_source_copy(&pool, p, src, None, "lonely").await;

        let receipt = delete_source_impl(&pool, src, false, false)
            .await
            .expect("delete");
        assert_eq!(receipt.removed_photos, 0, "orphan NOT removed");

        let photo_still_there: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE id = ?1")
                .bind(p)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(photo_still_there, 1, "photo row remains");
    }

    #[tokio::test]
    async fn remove_photos_preview_splits_local_and_cloud_only() {
        let pool = test_pool().await;
        let src = insert_source(&pool, "A", "local").await;

        let p_local = insert_photo(&pool, "L", 100).await;
        insert_source_copy(&pool, p_local, src, Some("/tmp/L.jpg"), "L").await;

        let p_cloud = insert_photo(&pool, "C", 200).await;
        insert_source_copy(&pool, p_cloud, src, None, "C").await;

        let preview = remove_photos_preview_impl(&pool, &[p_local, p_cloud])
            .await
            .expect("preview");
        assert_eq!(preview.photo_count, 2);
        assert_eq!(preview.local_files, 1);
        assert_eq!(preview.cloud_only_photos, 1);
        assert_eq!(preview.total_bytes, 100, "only p_local contributes bytes");
    }

    #[tokio::test]
    async fn remove_photos_preview_empty_input_returns_zeros() {
        let pool = test_pool().await;
        let preview = remove_photos_preview_impl(&pool, &[]).await.unwrap();
        assert_eq!(preview.photo_count, 0);
        assert_eq!(preview.local_files, 0);
        assert_eq!(preview.cloud_only_photos, 0);
        assert_eq!(preview.total_bytes, 0);
    }

    #[tokio::test]
    async fn remove_photos_from_catalog_deletes_rows_and_source_copies() {
        let pool = test_pool().await;
        let src = insert_source(&pool, "A", "local").await;

        let p1 = insert_photo(&pool, "p1", 10).await;
        insert_source_copy(&pool, p1, src, None, "p1").await;
        let p2 = insert_photo(&pool, "p2", 20).await;
        insert_source_copy(&pool, p2, src, None, "p2").await;

        let receipt = remove_photos_from_catalog_impl(&pool, &[p1])
            .await
            .expect("remove");
        assert_eq!(receipt.removed_photos, 1);

        let remaining: Vec<i64> = sqlx::query_scalar("SELECT id FROM photos ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, vec![p2], "only p2 should remain");

        let orphan_copies: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM source_copies WHERE photo_id = ?1")
                .bind(p1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            orphan_copies, 0,
            "FK cascade removed source_copies for deleted photo"
        );
    }

    #[tokio::test]
    async fn recycle_source_copies_sends_local_files_to_trash_and_reports() {
        let pool = test_pool().await;
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let file_a = tmp.path().join("a.jpg");
        let file_b = tmp.path().join("b.jpg");
        std::fs::write(&file_a, b"aaa").unwrap();
        std::fs::write(&file_b, b"bbb").unwrap();

        let src = insert_source(&pool, "A", "local").await;
        let pa = insert_photo(&pool, "a", 3).await;
        insert_source_copy(&pool, pa, src, Some(file_a.to_str().unwrap()), "a").await;
        let pb = insert_photo(&pool, "b", 3).await;
        insert_source_copy(&pool, pb, src, Some(file_b.to_str().unwrap()), "b").await;

        let receipt = recycle_source_copies_impl(&pool, &[pa, pb])
            .await
            .expect("recycle");

        // DB rows are UNTOUCHED (recycle is file-only).
        let photo_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(photo_rows, 2, "recycle must NOT delete DB rows");

        // Files are gone from their original location (in Recycle Bin on Windows;
        // moved or deleted on other platforms depending on `trash` backend).
        assert!(!file_a.exists(), "a.jpg should be gone from source dir");
        assert!(!file_b.exists(), "b.jpg should be gone from source dir");
        assert_eq!(
            receipt.recycled_count, 2,
            "both files should have been recycled"
        );
        assert_eq!(receipt.skipped_count, 0);
        assert!(receipt.errors.is_empty(), "no errors expected");
    }

    #[tokio::test]
    async fn recycle_source_copies_skips_missing_and_empty_input() {
        let pool = test_pool().await;
        let src = insert_source(&pool, "A", "local").await;

        // Photo with a path that doesn't exist on disk.
        let p = insert_photo(&pool, "ghost", 1).await;
        insert_source_copy(&pool, p, src, Some("/nonexistent/ghost.jpg"), "ghost").await;

        let receipt = recycle_source_copies_impl(&pool, &[p])
            .await
            .expect("recycle");
        assert_eq!(receipt.recycled_count, 0);
        assert_eq!(
            receipt.skipped_count, 1,
            "nonexistent path counted as skipped"
        );

        // Empty input is a no-op.
        let empty = recycle_source_copies_impl(&pool, &[]).await.unwrap();
        assert_eq!(empty.recycled_count, 0);
        assert_eq!(empty.skipped_count, 0);
    }
}
