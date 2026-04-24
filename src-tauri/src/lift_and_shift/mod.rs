//! Lift & Shift — rsync-style copy of photos from scattered sources into a
//! unified `D:/Chronimage/` library with SHA256 pre/post verification and a
//! persisted manifest.
//!
//! # Two-step flow
//! 1. Call `plan_lift(pool, target_root)` → `LiftPlan` (dry-run; no files moved).
//! 2. Call `execute_lift(plan_id, confirm_token, pool)` → `LiftReceipt`.
//!
//! Plans are held in [`PENDING_LIFT_PLANS`] (in-process; same pattern as
//! `PENDING_PLANS` in `commands.rs` for source-side cleanup).
//!
//! # Safety invariants
//! 1. Two-step: `plan_lift` issues a signed `confirm_token`; `execute_lift`
//!    rejects anything that doesn't match.
//! 2. SHA256 pre-verify: source file is re-hashed before copy. Mismatch →
//!    skip + append to `LiftReceipt.errors`; never panics.
//! 3. SHA256 post-verify: destination file is hashed after copy. Mismatch →
//!    error + destination file removed.
//! 4. ≥ 1.5× free-space gate: target drive must have ≥ 1.5× `total_bytes`
//!    free. Failure sets `LiftPlan.free_space_ok = false`; caller decides
//!    whether to abort.
//! 5. Lift does NOT delete source copies — that is source-side cleanup's job.
//!    The original `source_copies` row is left intact.
//!
//! Frontend wiring lives in `src/tauri/invoke.ts` (`liftShiftDryRun` /
//! `liftShiftExecute`) with React Query hooks in `src/state/queries.ts`.

use crate::{import, AppError, AppResult};
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

// ── Public types ─────────────────────────────────────────────────────────────

/// A single file that a lift plan proposes to copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftItem {
    /// `source_copies.id`
    pub copy_id: i64,
    /// `photos.id`
    pub photo_id: i64,
    /// `sources.id`
    pub source_id: i64,
    /// Absolute source path on disk.
    pub src_path: String,
    /// Relative destination path within `target_root`, e.g. `"2026/04/IMG_1234.jpg"`.
    pub dest_rel_path: String,
    /// SHA256 recorded at import time (used for pre-copy verification).
    pub sha256: String,
    /// File size in bytes recorded at import time.
    pub size_bytes: i64,
}

/// The result of `plan_lift`: a plan that can be executed by passing its
/// `plan_id` + `confirm_token` to `execute_lift`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftPlan {
    pub plan_id: String,
    /// Single-use token that must be echoed back in `execute_lift`.
    pub confirm_token: String,
    pub target_root: PathBuf,
    /// Total bytes to copy.
    pub total_bytes: u64,
    /// Number of files to copy.
    pub total_file_count: usize,
    pub items: Vec<LiftItem>,
    /// `true` when target drive has ≥ 1.5× `total_bytes` free at plan time,
    /// **or** when we couldn't probe the drive (in which case
    /// `free_space_probe_error` is set so the caller can warn the user).
    pub free_space_ok: bool,
    /// OS-level error text from the free-space probe. `None` on a clean
    /// probe (success or failure); `Some` when the probe itself errored
    /// (unmounted volume, permissions, etc.). The UI should surface this
    /// as a non-blocking warning so the user knows the 1.5× gate was
    /// effectively skipped.
    pub free_space_probe_error: Option<String>,
}

/// Result of a successful (or partially-successful) `execute_lift`.
#[derive(Debug, Serialize)]
pub struct LiftReceipt {
    pub copied_count: usize,
    pub bytes_copied: u64,
    /// Absolute path to the manifest JSON file written under `target_root/_manifest/`.
    pub manifest_path: PathBuf,
    /// Non-fatal per-file errors (SHA256 mismatch, I/O error, etc.).
    pub errors: Vec<String>,
}

// ── In-process plan store ────────────────────────────────────────────────────

static PENDING_LIFT_PLANS: Lazy<Mutex<HashMap<String, LiftPlan>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// ── Free-space helper (Windows-native; safe sentinel on other platforms) ────

fn free_bytes_for_path(path: &Path) -> std::io::Result<u64> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

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
        // Non-Windows CI runners: return sentinel so the gate always passes.
        let _ = path;
        Ok(u64::MAX)
    }
}

// ── Planning ─────────────────────────────────────────────────────────────────

/// Compute the lift plan: find all source_copies whose local path does NOT
/// already live under `target_root`, then build the copy list.
///
/// Filename collisions (same `dest_rel_path`, different SHA256) are resolved
/// by appending `_<first8ofsha256>` to the stem. Same-SHA collisions (idempotent
/// re-run) are silently skipped.
///
/// The resulting plan is stored in [`PENDING_LIFT_PLANS`] and returned.
pub async fn plan_lift(pool: &sqlx::SqlitePool, target_root: PathBuf) -> AppResult<LiftPlan> {
    // Normalise target_root to a forward-slash string for prefix comparison.
    // On Windows, PathBuf::display() uses backslashes, so we convert to a
    // consistent form by canonicalising the separator.
    let target_prefix = path_to_prefix_string(&target_root);

    #[derive(sqlx::FromRow)]
    struct CopyRow {
        copy_id: i64,
        photo_id: i64,
        source_id: i64,
        path: String,
        verified_sha256: String,
        size_bytes: Option<i64>,
        captured_at: Option<String>,
        imported_at: String,
        filename: String,
    }

    let rows: Vec<CopyRow> = sqlx::query_as::<_, CopyRow>(
        "SELECT sc.id AS copy_id, sc.photo_id, sc.source_id,
                sc.path, sc.verified_sha256,
                p.size_bytes, p.captured_at, p.imported_at, p.filename
         FROM source_copies sc
         JOIN photos p ON p.id = sc.photo_id
         WHERE sc.path IS NOT NULL
           AND sc.verified_sha256 IS NOT NULL
           AND sc.last_seen_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    // dest_rel_path → sha256: track seen destinations to detect collisions.
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut items: Vec<LiftItem> = Vec::new();

    for row in rows {
        // Skip files that already live under target_root.
        let normalised_src = normalise_sep(&row.path);
        if normalised_src.starts_with(&target_prefix) {
            continue;
        }

        // Build YYYY/MM/<filename> from captured_at; fall back to imported_at.
        let date_str = row.captured_at.as_deref().unwrap_or(&row.imported_at);
        let (year, month) = parse_year_month(date_str);
        let base_rel = format!("{}/{}/{}", year, month, row.filename);

        // Collision resolution.
        let dest_rel_path = if let Some(existing_sha) = seen.get(&base_rel) {
            if *existing_sha == row.verified_sha256 {
                // Same file already queued (idempotent re-run scenario).
                continue;
            }
            // Different SHA → append hash suffix to stem.
            let stem_with_suffix = stem_with_sha_suffix(&row.filename, &row.verified_sha256);
            format!("{}/{}/{}", year, month, stem_with_suffix)
        } else {
            base_rel.clone()
        };

        seen.insert(dest_rel_path.clone(), row.verified_sha256.clone());

        items.push(LiftItem {
            copy_id: row.copy_id,
            photo_id: row.photo_id,
            source_id: row.source_id,
            src_path: row.path,
            dest_rel_path,
            sha256: row.verified_sha256,
            size_bytes: row.size_bytes.unwrap_or(0),
        });
    }

    let total_bytes: u64 = items.iter().map(|i| i.size_bytes.max(0) as u64).sum();

    // ≥ 1.5× free-space gate.
    // If target_root does not yet exist, check its nearest existing ancestor.
    let probe_path = nearest_existing_ancestor(&target_root);
    let (free_space_ok, free_space_probe_error) = match free_bytes_for_path(&probe_path) {
        Ok(free) => {
            let required = total_bytes.saturating_add(total_bytes / 2); // 1.5×
            (free >= required, None)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "lift-and-shift: could not check free space on target volume; \
                 proceeding with free_space_ok = true + warning surfaced to UI"
            );
            (true, Some(e.to_string()))
        }
    };

    let plan_id = Uuid::new_v4().to_string();
    let confirm_token = Uuid::new_v4().to_string();

    let plan = LiftPlan {
        plan_id: plan_id.clone(),
        confirm_token,
        target_root,
        total_bytes,
        total_file_count: items.len(),
        items,
        free_space_ok,
        free_space_probe_error,
    };

    let mut guard = PENDING_LIFT_PLANS
        .lock()
        .map_err(|_| AppError::Internal("lift plan store lock poisoned".into()))?;
    guard.insert(plan_id, plan.clone());

    Ok(plan)
}

// ── Execution ─────────────────────────────────────────────────────────────────

/// Execute a previously issued lift plan.
///
/// For each item:
/// 1. SHA256 pre-verify source file (mismatch → skip + error).
/// 2. `std::fs::copy` source → `target_root / dest_rel_path`.
/// 3. SHA256 post-verify destination (mismatch → remove dest + error).
/// 4. Insert a new `source_copies` row for the Chronimage Local source.
///
/// At the end, write a manifest JSON to `target_root/_manifest/lift_<ts>.json`.
///
/// Plans are single-use: removed from [`PENDING_LIFT_PLANS`] on the first
/// call regardless of outcome (successful or partial).
pub async fn execute_lift(
    pool: &sqlx::SqlitePool,
    plan_id: &str,
    confirm_token: &str,
) -> AppResult<LiftReceipt> {
    // ── Gate 1: retrieve and validate plan ──────────────────────────────────
    let plan = {
        let mut guard = PENDING_LIFT_PLANS
            .lock()
            .map_err(|_| AppError::Internal("lift plan store lock poisoned".into()))?;
        guard
            .remove(plan_id)
            .ok_or_else(|| AppError::NotFound(format!("lift plan not found: {plan_id}")))?
    };

    if plan.confirm_token != confirm_token {
        // Wrong token: re-insert the plan so the user can retry with correct token.
        // NOTE: per spec, leave plan in place on token mismatch; only remove on success.
        let mut guard = PENDING_LIFT_PLANS
            .lock()
            .map_err(|_| AppError::Internal("lift plan store lock poisoned".into()))?;
        guard.insert(plan_id.to_string(), plan);
        return Err(AppError::PermissionDenied(
            "confirm_token does not match the issued lift plan".into(),
        ));
    }

    let target_root = &plan.target_root;

    // Ensure the Chronimage Local source row exists (lazy upsert).
    let local_source_id = ensure_chronimage_local_source(pool, target_root).await?;

    let mut copied_count: usize = 0;
    let mut bytes_copied: u64 = 0;
    let mut errors: Vec<String> = Vec::new();

    // Manifest accumulator.
    #[derive(Serialize)]
    struct ManifestEntry {
        src_path: String,
        dest_path: String,
        sha256: String,
        size_bytes: i64,
        copied_at: String,
    }
    let mut manifest_entries: Vec<ManifestEntry> = Vec::new();

    for item in &plan.items {
        let src = PathBuf::from(&item.src_path);

        // ── SHA256 pre-verify ─────────────────────────────────────────────
        let pre_sha = match import::sha256_file(&src) {
            Ok(h) => h,
            Err(e) => {
                errors.push(format!(
                    "photo {}: could not hash source {}: {}",
                    item.photo_id,
                    src.display(),
                    e
                ));
                continue;
            }
        };

        if pre_sha != item.sha256 {
            errors.push(format!(
                "photo {}: source SHA256 mismatch on {} — expected {} got {} — skipping",
                item.photo_id,
                src.display(),
                item.sha256,
                pre_sha
            ));
            continue;
        }

        // ── Copy ──────────────────────────────────────────────────────────
        let dest = target_root.join(&item.dest_rel_path);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!(
                    "photo {}: could not create destination dir {}: {}",
                    item.photo_id,
                    parent.display(),
                    e
                ));
                continue;
            }
        }

        match std::fs::copy(&src, &dest) {
            Ok(n) => {
                // ── SHA256 post-verify ────────────────────────────────────
                match import::sha256_file(&dest) {
                    Ok(post_sha) if post_sha == item.sha256 => {
                        let copied_at = Utc::now().to_rfc3339();

                        // Insert new source_copies row for the local target.
                        let now = copied_at.clone();
                        if let Err(e) = sqlx::query(
                            "INSERT OR IGNORE INTO source_copies \
                             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
                             VALUES (?1, ?2, ?3, 0, ?4, ?5)",
                        )
                        .bind(item.photo_id)
                        .bind(local_source_id)
                        .bind(dest.to_string_lossy().as_ref())
                        .bind(&item.sha256)
                        .bind(&now)
                        .execute(pool)
                        .await
                        {
                            tracing::warn!(
                                error = %e,
                                photo_id = item.photo_id,
                                "lift: failed to insert source_copies row (non-fatal)"
                            );
                            errors.push(format!(
                                "photo {}: file copied but source_copies insert failed: {}",
                                item.photo_id, e
                            ));
                        }

                        manifest_entries.push(ManifestEntry {
                            src_path: item.src_path.clone(),
                            dest_path: dest.to_string_lossy().into_owned(),
                            sha256: item.sha256.clone(),
                            size_bytes: item.size_bytes,
                            copied_at,
                        });

                        copied_count += 1;
                        bytes_copied += n;
                    }
                    Ok(bad_sha) => {
                        // Post-verify failed: remove corrupt destination.
                        let _ = std::fs::remove_file(&dest);
                        errors.push(format!(
                            "photo {}: destination SHA256 mismatch after copy to {} — \
                             expected {} got {} — destination removed",
                            item.photo_id,
                            dest.display(),
                            item.sha256,
                            bad_sha
                        ));
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&dest);
                        errors.push(format!(
                            "photo {}: could not hash destination {}: {} — destination removed",
                            item.photo_id,
                            dest.display(),
                            e
                        ));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "photo {}: failed to copy {} → {}: {}",
                    item.photo_id,
                    src.display(),
                    dest.display(),
                    e
                ));
            }
        }
    }

    // ── Write manifest ────────────────────────────────────────────────────────
    let manifest_dir = target_root.join("_manifest");
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let manifest_filename = format!("lift_{ts}.json");
    let manifest_path = manifest_dir.join(&manifest_filename);

    let manifest_doc = serde_json::json!({
        "plan_id": plan_id,
        "target_root": target_root.to_string_lossy(),
        "copies": manifest_entries,
    });

    // Non-fatal: if we can't write the manifest, log + report in errors.
    let manifest_path = match (|| -> AppResult<PathBuf> {
        std::fs::create_dir_all(&manifest_dir)?;
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest_doc)?)?;
        Ok(manifest_path)
    })() {
        Ok(p) => p,
        Err(e) => {
            errors.push(format!("manifest write failed: {e}"));
            manifest_dir.join(manifest_filename) // return the path even if write failed
        }
    };

    Ok(LiftReceipt {
        copied_count,
        bytes_copied,
        manifest_path,
        errors,
    })
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Ensure a `sources` row of kind `"local"` named `"Chronimage Local"` exists
/// at the given root. Creates it if absent. Returns the `source_id`.
async fn ensure_chronimage_local_source(
    pool: &sqlx::SqlitePool,
    target_root: &Path,
) -> AppResult<i64> {
    let root_str = target_root.to_string_lossy().into_owned();
    let config = serde_json::json!({ "root": root_str, "managed": true }).to_string();

    // Try to find an existing source with the same root.
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM sources WHERE kind = 'local' AND json_extract(config_json, '$.managed') = 1 \
         AND json_extract(config_json, '$.root') = ?1 LIMIT 1",
    )
    .bind(&root_str)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let now = Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, config_json, created_at) \
         VALUES ('Chronimage Local', 'local', 'ready', ?1, ?2) RETURNING id",
    )
    .bind(&config)
    .bind(&now)
    .fetch_one(pool)
    .await?;

    tracing::info!(source_id = id, root = %root_str, "lift-and-shift: created Chronimage Local source");
    Ok(id)
}

/// Walk up the path tree to find the nearest ancestor that actually exists on
/// disk. Used to probe free space when `target_root` itself hasn't been created
/// yet.
fn nearest_existing_ancestor(p: &Path) -> PathBuf {
    let mut candidate = p.to_path_buf();
    loop {
        if candidate.exists() {
            return candidate;
        }
        match candidate.parent() {
            Some(parent) => candidate = parent.to_path_buf(),
            None => return candidate, // reached filesystem root
        }
    }
}

/// Normalise path separators to `/` for consistent prefix matching.
fn normalise_sep(s: &str) -> String {
    s.replace('\\', "/")
}

/// Convert a PathBuf to a normalised prefix string (with trailing `/`).
fn path_to_prefix_string(p: &Path) -> String {
    let mut s = normalise_sep(&p.to_string_lossy());
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// Parse `YYYY` and `MM` from an ISO-8601 string. Falls back to `"0000"/"00"`.
fn parse_year_month(dt: &str) -> (String, String) {
    // ISO-8601: "YYYY-MM-DD..." or "YYYY/MM/DD..."
    let clean = dt.replace('/', "-");
    let parts: Vec<&str> = clean.splitn(3, '-').collect();
    let year = parts.first().copied().unwrap_or("0000").to_string();
    let month = parts.get(1).copied().unwrap_or("00").to_string();
    (year, month)
}

/// Given `"IMG_1234.jpg"` and a sha256, return `"IMG_1234_a1b2c3d4.jpg"`.
fn stem_with_sha_suffix(filename: &str, sha256: &str) -> String {
    let suffix = &sha256[..8.min(sha256.len())];
    if let Some(dot) = filename.rfind('.') {
        let (stem, ext) = filename.split_at(dot);
        format!("{}_{}{}", stem, suffix, ext)
    } else {
        format!("{}_{}", filename, suffix)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use std::fs;
    use tempfile::TempDir;

    // ── Unit tests ────────────────────────────────────────────────────────────

    #[test]
    fn dest_rel_path_uses_year_month_from_captured_at() {
        let (year, month) = parse_year_month("2024-07-15T10:30:00Z");
        assert_eq!(year, "2024");
        assert_eq!(month, "07");
    }

    #[test]
    fn parse_year_month_falls_back_on_missing_month() {
        // A string with only a year produces an empty month part.
        let (year, month) = parse_year_month("2024");
        assert_eq!(year, "2024");
        assert_eq!(month, "00");
    }

    #[test]
    fn colliding_different_sha_appends_hash_suffix() {
        let result = stem_with_sha_suffix("IMG_1234.jpg", "abcdef1234567890");
        assert_eq!(result, "IMG_1234_abcdef12.jpg");
    }

    #[test]
    fn colliding_no_extension_appends_hash_suffix() {
        let result = stem_with_sha_suffix("noext", "abcdef1234567890");
        assert_eq!(result, "noext_abcdef12");
    }

    #[test]
    fn path_prefix_normalises_backslashes() {
        let p = PathBuf::from(r"D:\Chronimage");
        let prefix = path_to_prefix_string(&p);
        assert!(prefix.contains('/'));
        assert!(prefix.ends_with('/'));
    }

    // ── Integration tests (require a real SQLite pool) ────────────────────────

    async fn make_test_pool(dir: &Path) -> sqlx::SqlitePool {
        let db_path = dir.join("test_catalog.db");
        open_pool(PoolOptions {
            db_path,
            max_connections: 1,
            create_if_missing: true,
            run_migrations: true,
        })
        .await
        .expect("open pool")
    }

    #[tokio::test]
    async fn plan_empty_db_returns_empty_plan() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_test_pool(tmp.path()).await;
        let target = tmp.path().join("target");

        let plan = plan_lift(&pool, target.clone())
            .await
            .expect("plan_lift should succeed on empty db");

        assert_eq!(plan.total_file_count, 0);
        assert_eq!(plan.total_bytes, 0);
        assert!(plan.items.is_empty());
        assert!(!plan.plan_id.is_empty());
        assert!(!plan.confirm_token.is_empty());
    }

    #[tokio::test]
    async fn execute_with_wrong_confirm_token_errors() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_test_pool(tmp.path()).await;
        let target = tmp.path().join("target");

        let plan = plan_lift(&pool, target.clone()).await.expect("plan_lift");

        let result = execute_lift(&pool, &plan.plan_id, "wrong-token-entirely").await;
        assert!(result.is_err(), "should error on wrong confirm_token");
        assert!(
            matches!(result.unwrap_err(), AppError::PermissionDenied(_)),
            "expected PermissionDenied"
        );
    }

    #[tokio::test]
    async fn execute_with_missing_plan_id_errors() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_test_pool(tmp.path()).await;

        let result = execute_lift(&pool, "no-such-plan-id", "any-token").await;
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), AppError::NotFound(_)),
            "expected NotFound"
        );
    }

    #[tokio::test]
    async fn end_to_end_copies_file_and_writes_manifest() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = make_test_pool(tmp.path()).await;

        // Create a real source file.
        let src_dir = tmp.path().join("source");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src_file = src_dir.join("IMG_001.jpg");
        fs::write(&src_file, b"fake jpeg bytes for test").expect("write src");

        // Hash the file so we can insert a verified source_copy.
        let sha = import::sha256_file(&src_file).expect("hash");

        // Insert source, photo, and source_copy rows.
        let now = "2024-07-15T10:00:00Z";
        let source_id: i64 = sqlx::query_scalar(
            "INSERT INTO sources (name, kind, status, config_json, created_at) \
             VALUES ('Test Source', 'local', 'ready', '{}', ?1) RETURNING id",
        )
        .bind(now)
        .fetch_one(&pool)
        .await
        .expect("insert source");

        let photo_id: i64 = sqlx::query_scalar(
            "INSERT INTO photos (sha256, filename, width, height, captured_at, imported_at) \
             VALUES (?1, 'IMG_001.jpg', 100, 100, '2024-07-15T10:00:00Z', ?2) RETURNING id",
        )
        .bind(&sha)
        .bind(now)
        .fetch_one(&pool)
        .await
        .expect("insert photo");

        sqlx::query(
            "INSERT INTO source_copies \
             (photo_id, source_id, path, is_primary, verified_sha256, last_seen_at) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
        )
        .bind(photo_id)
        .bind(source_id)
        .bind(src_file.to_string_lossy().as_ref())
        .bind(&sha)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert source_copy");

        // Plan.
        let target = tmp.path().join("target");
        let plan = plan_lift(&pool, target.clone()).await.expect("plan_lift");

        assert_eq!(plan.total_file_count, 1, "one file should be planned");

        // Execute.
        let receipt = execute_lift(&pool, &plan.plan_id, &plan.confirm_token)
            .await
            .expect("execute_lift");

        assert_eq!(receipt.copied_count, 1, "one file copied");
        assert!(receipt.errors.is_empty(), "no errors: {:?}", receipt.errors);

        // Destination file should exist.
        let dest = target.join(&plan.items[0].dest_rel_path);
        assert!(
            dest.exists(),
            "destination file should exist at {}",
            dest.display()
        );

        // Verify post-copy SHA.
        let dest_sha = import::sha256_file(&dest).expect("hash dest");
        assert_eq!(dest_sha, sha);

        // Manifest should have been written.
        assert!(
            receipt.manifest_path.exists(),
            "manifest file should exist at {}",
            receipt.manifest_path.display()
        );

        // New source_copies row for the target.
        let copy_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM source_copies WHERE photo_id = ?1")
                .bind(photo_id)
                .fetch_one(&pool)
                .await
                .expect("count copies");
        assert_eq!(copy_count, 2, "original + new local copy");
    }
}
