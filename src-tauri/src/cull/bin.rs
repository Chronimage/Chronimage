//! Cull Bin lifecycle — list / restore / permanent-delete + daily sweep.
//!
//! Covers Phase 2 PRD §3 + §4. The UI is purely a view on the `cull_bin`
//! table; every mutation goes through one of the functions here so the tests
//! + background sweep share a single code path.

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Filter chip in the sidepanel. `All` = no filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CullFilter {
    All,
    NearDup,
    Blur,
    EyesClosed,
    Exposure,
    User,
    Flag,
    Duplicate,
    Other,
}

impl CullFilter {
    fn sql_clause(self) -> Option<&'static str> {
        match self {
            CullFilter::All => None,
            CullFilter::NearDup => Some("near_dup"),
            CullFilter::Blur => Some("blur"),
            CullFilter::EyesClosed => Some("eyes_closed"),
            CullFilter::Exposure => Some("exposure"),
            CullFilter::User => Some("user"),
            CullFilter::Flag => Some("flag"),
            CullFilter::Duplicate => Some("duplicate"),
            CullFilter::Other => Some("other"),
        }
    }
}

/// One row in the Cull Bin list — enough for the UI to render a row + decide
/// how many bytes will be reclaimed when permanently deleted.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CullBinRow {
    pub photo_id: i64,
    pub filename: String,
    pub rejected_at: String,
    pub reason: String,
    pub retention_days: i64,
    pub permanent_delete_after: String,
    pub size_bytes: Option<i64>,
    pub sha256: String,
}

/// Summary of what's in the bin right now. Feeds the sidepanel's "Reclaimable"
/// block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullBinSummary {
    pub total_count: i64,
    pub total_bytes: i64,
    pub by_reason: Vec<(String, i64)>,
}

/// List Cull Bin entries, newest first. The `filter` chip narrows by reason.
pub async fn list(pool: &SqlitePool, filter: CullFilter) -> AppResult<Vec<CullBinRow>> {
    match filter.sql_clause() {
        Some(reason) => sqlx::query_as::<_, CullBinRow>(
            "SELECT cb.photo_id, p.filename, cb.rejected_at, cb.reason, \
                    cb.retention_days, cb.permanent_delete_after, \
                    p.size_bytes, p.sha256 \
             FROM cull_bin cb JOIN photos p ON p.id = cb.photo_id \
             WHERE cb.reason = ?1 \
             ORDER BY cb.rejected_at DESC",
        )
        .bind(reason)
        .fetch_all(pool)
        .await
        .map_err(AppError::from),
        None => sqlx::query_as::<_, CullBinRow>(
            "SELECT cb.photo_id, p.filename, cb.rejected_at, cb.reason, \
                    cb.retention_days, cb.permanent_delete_after, \
                    p.size_bytes, p.sha256 \
             FROM cull_bin cb JOIN photos p ON p.id = cb.photo_id \
             ORDER BY cb.rejected_at DESC",
        )
        .fetch_all(pool)
        .await
        .map_err(AppError::from),
    }
}

/// Aggregate summary for the sidepanel.
pub async fn summary(pool: &SqlitePool) -> AppResult<CullBinSummary> {
    let total_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cull_bin")
        .fetch_one(pool)
        .await?;
    let total_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.size_bytes), 0) \
         FROM cull_bin cb JOIN photos p ON p.id = cb.photo_id",
    )
    .fetch_one(pool)
    .await?;
    let by_reason = sqlx::query_as::<_, (String, i64)>(
        "SELECT reason, COUNT(*) FROM cull_bin GROUP BY reason ORDER BY COUNT(*) DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(CullBinSummary {
        total_count,
        total_bytes,
        by_reason,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReceipt {
    pub restored_count: usize,
    pub skipped: Vec<i64>,
}

/// Remove rows from `cull_bin` → photos are "restored to catalog". The frozen
/// `source_copies_frozen_json` is no longer needed — the live rows in
/// `source_copies` were never touched at rejection time, so the photo pops
/// back into the catalog with full source metadata intact.
pub async fn restore(pool: &SqlitePool, photo_ids: &[i64]) -> AppResult<RestoreReceipt> {
    if photo_ids.is_empty() {
        return Ok(RestoreReceipt {
            restored_count: 0,
            skipped: Vec::new(),
        });
    }
    let mut restored = 0usize;
    let mut skipped: Vec<i64> = Vec::new();
    let mut tx = pool.begin().await?;
    for id in photo_ids {
        let res = sqlx::query("DELETE FROM cull_bin WHERE photo_id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 1 {
            restored += 1;
        } else {
            skipped.push(*id);
        }
    }
    tx.commit().await?;
    Ok(RestoreReceipt {
        restored_count: restored,
        skipped,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyReceipt {
    pub deleted_photo_count: usize,
    pub freed_bytes: i64,
    pub errors: Vec<String>,
}

/// Permanently delete the listed photos — removes the `photos` row (FK
/// cascades handle faces/tags/embeddings/source_copies), best-effort nukes
/// the cached `{sha256}_320.jpg` thumbnail, and appends to `source_deletions`
/// for audit.
///
/// Does NOT unlink original files on disk — callers that also want recycle-
/// bin routing should drive `recycle_source_copies` first (Phase-1 ADR 0006
/// path) then call this.
pub async fn delete_forever(pool: &SqlitePool, photo_ids: &[i64]) -> AppResult<EmptyReceipt> {
    if photo_ids.is_empty() {
        return Ok(EmptyReceipt {
            deleted_photo_count: 0,
            freed_bytes: 0,
            errors: Vec::new(),
        });
    }

    let mut errors: Vec<String> = Vec::new();

    // Sum sizes + collect sha256s BEFORE delete (once rows are gone we can't
    // report what was freed).
    struct DelMeta {
        id: i64,
        sha256: String,
        size_bytes: i64,
    }
    let mut metas: Vec<DelMeta> = Vec::new();
    for id in photo_ids {
        if let Some((sha, sz)) = sqlx::query_as::<_, (String, Option<i64>)>(
            "SELECT sha256, size_bytes FROM photos WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        {
            metas.push(DelMeta {
                id: *id,
                sha256: sha,
                size_bytes: sz.unwrap_or(0),
            });
        }
    }

    let mut tx = pool.begin().await?;
    let mut deleted = 0usize;
    let mut freed = 0i64;
    for meta in &metas {
        // sqlite-vec cascade isn't automatic — explicit deletes keep the
        // virtual tables in sync (matches remove_photos_from_catalog).
        let _ = sqlx::query("DELETE FROM vec_photo_embeddings WHERE rowid = ?1")
            .bind(meta.id)
            .execute(&mut *tx)
            .await;
        let _ = sqlx::query("DELETE FROM vec_photo_embeddings_int8 WHERE rowid = ?1")
            .bind(meta.id)
            .execute(&mut *tx)
            .await;
        // Photo row goes last — FK cascades handle tags/faces/embeddings/
        // source_copies/photo_embeddings/cull_bin.
        let res = sqlx::query("DELETE FROM photos WHERE id = ?1")
            .bind(meta.id)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 1 {
            deleted += 1;
            freed += meta.size_bytes;
        }
    }
    tx.commit().await?;

    // Best-effort thumbnail cleanup, off-transaction.
    if let Ok(thumbs) = crate::util::paths::thumbnails_dir() {
        for meta in &metas {
            for size in [320, 640, 1280] {
                let p = thumbs.join(format!("{}_{size}.jpg", meta.sha256));
                if p.exists() {
                    if let Err(e) = std::fs::remove_file(&p) {
                        errors.push(format!("thumb remove {}: {e}", p.display()));
                    }
                }
            }
        }
    }

    Ok(EmptyReceipt {
        deleted_photo_count: deleted,
        freed_bytes: freed,
        errors,
    })
}

/// Daily sweep: permanently delete every `cull_bin` row whose
/// `permanent_delete_after` has passed. Called by the background task at
/// app start.
pub async fn sweep_expired(pool: &SqlitePool) -> AppResult<EmptyReceipt> {
    let now = chrono::Utc::now().to_rfc3339();
    let expired: Vec<i64> =
        sqlx::query_scalar("SELECT photo_id FROM cull_bin WHERE permanent_delete_after <= ?1")
            .bind(&now)
            .fetch_all(pool)
            .await?;
    if expired.is_empty() {
        return Ok(EmptyReceipt {
            deleted_photo_count: 0,
            freed_bytes: 0,
            errors: Vec::new(),
        });
    }
    tracing::info!(expired_count = expired.len(), "cull_bin sweep firing");
    delete_forever(pool, &expired).await
}

#[cfg(test)]
mod tests {
    use super::super::verdict::{apply_verdict, CullReason, Verdict};
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn setup() -> SqlitePool {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw, size_bytes) \
             VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 1000), \
                    (2, '2222222222222222222222222222222222222222222222222222222222222222', 'b.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 2000), \
                    (3, '3333333333333333333333333333333333333333333333333333333333333333', 'c.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 3000)",
        )
        .await
        .expect("seed");
        pool
    }

    #[tokio::test]
    async fn list_respects_filter() {
        let pool = setup().await;
        apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
            .await
            .unwrap();
        apply_verdict(&pool, 2, Verdict::RejectA, CullReason::NearDup, 30)
            .await
            .unwrap();
        let all = list(&pool, CullFilter::All).await.unwrap();
        assert_eq!(all.len(), 2);
        let blurs = list(&pool, CullFilter::Blur).await.unwrap();
        assert_eq!(blurs.len(), 1);
        assert_eq!(blurs[0].photo_id, 1);
    }

    #[tokio::test]
    async fn summary_aggregates() {
        let pool = setup().await;
        apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
            .await
            .unwrap();
        apply_verdict(&pool, 2, Verdict::RejectA, CullReason::Blur, 30)
            .await
            .unwrap();
        let s = summary(&pool).await.unwrap();
        assert_eq!(s.total_count, 2);
        assert_eq!(s.total_bytes, 3000);
        assert_eq!(s.by_reason.first().map(|(r, _)| r.as_str()), Some("blur"));
    }

    #[tokio::test]
    async fn restore_removes_row() {
        let pool = setup().await;
        apply_verdict(&pool, 1, Verdict::RejectA, CullReason::User, 30)
            .await
            .unwrap();
        let r = restore(&pool, &[1]).await.unwrap();
        assert_eq!(r.restored_count, 1);
        assert!(r.skipped.is_empty());
        assert_eq!(list(&pool, CullFilter::All).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn delete_forever_frees_bytes_and_photo_row() {
        let pool = setup().await;
        apply_verdict(&pool, 1, Verdict::RejectA, CullReason::User, 30)
            .await
            .unwrap();
        let r = delete_forever(&pool, &[1]).await.unwrap();
        assert_eq!(r.deleted_photo_count, 1);
        assert_eq!(r.freed_bytes, 1000);
        let p: Option<(i64,)> = sqlx::query_as("SELECT id FROM photos WHERE id = 1")
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(p.is_none());
    }

    #[tokio::test]
    async fn sweep_deletes_only_past_rows() {
        let pool = setup().await;
        apply_verdict(&pool, 1, Verdict::RejectA, CullReason::User, 30)
            .await
            .unwrap();
        // Force a past expiry for photo 1.
        sqlx::query("UPDATE cull_bin SET permanent_delete_after = '2000-01-01T00:00:00Z' WHERE photo_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        apply_verdict(&pool, 2, Verdict::RejectA, CullReason::User, 30)
            .await
            .unwrap();
        let r = sweep_expired(&pool).await.unwrap();
        assert_eq!(r.deleted_photo_count, 1);
        let remaining: Vec<i64> = sqlx::query_scalar("SELECT photo_id FROM cull_bin")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, vec![2]);
    }
}
