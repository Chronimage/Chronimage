//! `apply_verdict(photo_id, verdict)` — moves rejected photos into `cull_bin`
//! with a frozen snapshot of their `source_copies` rows so they can be
//! restored for 30 days before any bytes are permanently deleted.
//!
//! Verdict semantics (Phase 2 PRD §2):
//! - `Keep`        — no-op; caller may still want the event for UX.
//! - `RejectA`     — reject `photo_id` itself.
//! - `RejectB`     — reject `photo_id`'s paired photo (RAW+JPG pair) if any.
//! - `RejectBoth`  — reject both members of the pair.
//! - `Skip`        — no-op; present so the UI can record "seen but undecided".
//!
//! Idempotent: re-applying a verdict on a photo already in `cull_bin` is a
//! no-op. No error, no duplicate rows (the `cull_bin.photo_id` PK enforces it).

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Frozen snapshot of one `source_copies` row captured at rejection time so
/// the photo can be restored even if the live row is later mutated.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FrozenCopy {
    pub id: i64,
    pub photo_id: i64,
    pub source_id: i64,
    pub path: Option<String>,
    pub verified_sha256: Option<String>,
    pub last_seen_at: Option<String>,
}

/// Possible verdict outcomes. Keep/Skip are no-ops; the three Reject variants
/// write to `cull_bin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Keep,
    RejectA,
    RejectB,
    RejectBoth,
    Skip,
}

/// Why a photo was rejected. Reported back to the UI + persisted in
/// `cull_bin.reason` for the Cull Bin filter chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CullReason {
    NearDup,
    Blur,
    EyesClosed,
    Exposure,
    User,
    Flag,
    Duplicate,
    Other,
}

impl CullReason {
    fn as_str(self) -> &'static str {
        match self {
            CullReason::NearDup => "near_dup",
            CullReason::Blur => "blur",
            CullReason::EyesClosed => "eyes_closed",
            CullReason::Exposure => "exposure",
            CullReason::User => "user",
            CullReason::Flag => "flag",
            CullReason::Duplicate => "duplicate",
            CullReason::Other => "other",
        }
    }
}

/// Receipt returned by [`apply_verdict`]. Lists the photo ids actually moved
/// into `cull_bin` (may be empty for Keep/Skip or if the photo was already
/// there).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictReceipt {
    pub photo_id: i64,
    pub verdict: Verdict,
    pub rejected_ids: Vec<i64>,
    /// Retention in days copied from the Settings tweak (default 30).
    pub retention_days: i64,
}

/// Core entry point. Verdict + reason + retention decide what happens.
///
/// `retention_days` should come from the Settings panel (default 30). Clamped
/// to `>= 1` internally.
pub async fn apply_verdict(
    pool: &SqlitePool,
    photo_id: i64,
    verdict: Verdict,
    reason: CullReason,
    retention_days: i64,
) -> AppResult<VerdictReceipt> {
    let retention_days = retention_days.max(1);

    // Look up the paired photo early so RejectB / RejectBoth can fan out.
    let paired: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT paired_photo_id FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    let mut targets: Vec<i64> = Vec::new();
    match verdict {
        Verdict::Keep | Verdict::Skip => {}
        Verdict::RejectA => targets.push(photo_id),
        Verdict::RejectB => {
            if let Some(p) = paired {
                targets.push(p);
            } else {
                // No pair — user pressed B on a singleton. Treat as RejectA
                // so the verdict isn't silently dropped.
                targets.push(photo_id);
            }
        }
        Verdict::RejectBoth => {
            targets.push(photo_id);
            if let Some(p) = paired {
                targets.push(p);
            }
        }
    }

    let mut rejected: Vec<i64> = Vec::new();
    for target in &targets {
        if reject_photo(pool, *target, reason, retention_days).await? {
            rejected.push(*target);
        }
    }

    Ok(VerdictReceipt {
        photo_id,
        verdict,
        rejected_ids: rejected,
        retention_days,
    })
}

/// Move a single photo into `cull_bin`. Returns `true` if this call created
/// the row (false if the photo was already there).
async fn reject_photo(
    pool: &SqlitePool,
    photo_id: i64,
    reason: CullReason,
    retention_days: i64,
) -> AppResult<bool> {
    // Snapshot the source_copies for restore; if the photo is later deleted
    // forever, the frozen JSON is enough to recover the original path list.
    let frozen = sqlx::query_as::<_, FrozenCopy>(
        "SELECT id, photo_id, source_id, path, verified_sha256, last_seen_at \
         FROM source_copies WHERE photo_id = ?1",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await?;
    let frozen_json = serde_json::to_string(&frozen)?;

    let now = chrono::Utc::now();
    let permanent = now + chrono::Duration::days(retention_days);

    let result = sqlx::query(
        "INSERT OR IGNORE INTO cull_bin \
         (photo_id, rejected_at, reason, source_copies_frozen_json, retention_days, permanent_delete_after) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(photo_id)
    .bind(now.to_rfc3339())
    .bind(reason.as_str())
    .bind(&frozen_json)
    .bind(retention_days)
    .bind(permanent.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Set `photos.rating`. Validates 0..=5 (0 clears).
pub async fn set_rating(pool: &SqlitePool, photo_id: i64, rating: i64) -> AppResult<()> {
    if !(0..=5).contains(&rating) {
        return Err(AppError::InvalidInput(format!(
            "rating must be 0..5, got {rating}"
        )));
    }
    let affected = sqlx::query("UPDATE photos SET rating = ?1 WHERE id = ?2")
        .bind(rating)
        .bind(photo_id)
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!("photo {photo_id}")));
    }
    Ok(())
}

/// Toggle `photos.is_flagged`. Returns the new flagged state. Separate from
/// the verdict engine: a flag is a soft signal the user can act on later via
/// the Cull screen, not an immediate Cull-Bin insert.
pub async fn toggle_flag(pool: &SqlitePool, photo_id: i64) -> AppResult<bool> {
    let current: Option<(i64,)> = sqlx::query_as("SELECT is_flagged FROM photos WHERE id = ?1")
        .bind(photo_id)
        .fetch_optional(pool)
        .await?;
    let current = current.ok_or_else(|| AppError::NotFound(format!("photo {photo_id}")))?;
    let next = if current.0 == 1 { 0 } else { 1 };
    let now = chrono::Utc::now().to_rfc3339();
    let flagged_at = if next == 1 { Some(&now) } else { None };
    sqlx::query("UPDATE photos SET is_flagged = ?1, flagged_at = ?2 WHERE id = ?3")
        .bind(next)
        .bind(flagged_at)
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(next == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn setup() -> SqlitePool {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, '0101010101010101010101010101010101010101010101010101010101010101', 'a.jpg', 100, 100, '2026-04-26T00:00:00Z', 0), \
                    (2, '0202020202020202020202020202020202020202020202020202020202020202', 'b.arw', 100, 100, '2026-04-26T00:00:00Z', 1), \
                    (3, '0303030303030303030303030303030303030303030303030303030303030303', 'c.jpg', 100, 100, '2026-04-26T00:00:00Z', 0)",
        )
        .await
        .expect("seed photos");
        pool.execute("UPDATE photos SET paired_photo_id = 2 WHERE id = 1; UPDATE photos SET paired_photo_id = 1 WHERE id = 2;")
            .await
            .expect("pair");
        pool
    }

    #[tokio::test]
    async fn keep_is_noop() {
        let pool = setup().await;
        let r = apply_verdict(&pool, 1, Verdict::Keep, CullReason::User, 30)
            .await
            .expect("verdict");
        assert!(r.rejected_ids.is_empty());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cull_bin")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn reject_a_inserts_once() {
        let pool = setup().await;
        let r = apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
            .await
            .expect("verdict");
        assert_eq!(r.rejected_ids, vec![1]);
        // Second apply — idempotent, no new row.
        let r2 = apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
            .await
            .expect("verdict 2");
        assert!(r2.rejected_ids.is_empty());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cull_bin")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn reject_both_targets_pair_members() {
        let pool = setup().await;
        let r = apply_verdict(&pool, 1, Verdict::RejectBoth, CullReason::NearDup, 30)
            .await
            .expect("verdict");
        assert_eq!(r.rejected_ids.len(), 2);
        let rejected: Vec<i64> =
            sqlx::query_scalar("SELECT photo_id FROM cull_bin ORDER BY photo_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rejected, vec![1, 2]);
    }

    #[tokio::test]
    async fn reject_b_on_singleton_falls_back_to_a() {
        let pool = setup().await;
        // Photo 3 has no pair — RejectB must still reject something, not swallow.
        let r = apply_verdict(&pool, 3, Verdict::RejectB, CullReason::User, 30)
            .await
            .expect("verdict");
        assert_eq!(r.rejected_ids, vec![3]);
    }

    #[tokio::test]
    async fn set_rating_clamps_and_persists() {
        let pool = setup().await;
        set_rating(&pool, 1, 4).await.expect("set 4");
        let s: i64 = sqlx::query_scalar("SELECT rating FROM photos WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s, 4);
        // Out-of-range rejected.
        assert!(matches!(
            set_rating(&pool, 1, 7).await.unwrap_err(),
            AppError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn toggle_flag_flips_state() {
        let pool = setup().await;
        assert!(toggle_flag(&pool, 1).await.unwrap());
        assert!(!toggle_flag(&pool, 1).await.unwrap());
    }
}
