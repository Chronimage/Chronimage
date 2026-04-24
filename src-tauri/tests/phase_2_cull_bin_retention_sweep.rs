//! Phase 2 acceptance test: the daily sweep permanently deletes Cull Bin
//! rows whose `permanent_delete_after` is in the past, while preserving
//! rows whose retention hasn't expired — even if their `retention_days`
//! is higher than the default 30.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::cull::bin::{list as list_bin, sweep_expired, CullFilter};
use chronimage::cull::verdict::{apply_verdict, CullReason, Verdict};
use sqlx::Executor;

#[tokio::test]
async fn sweep_deletes_expired_preserves_fresh() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");

    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw, size_bytes) \
         VALUES \
           (1, '1111111111111111111111111111111111111111111111111111111111111111', 'old.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 1000), \
           (2, '2222222222222222222222222222222222222222222222222222222222222222', 'fresh.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 2000), \
           (3, '3333333333333333333333333333333333333333333333333333333333333333', 'extra.jpg', 100, 100, '2026-04-26T00:00:00Z', 0, 3000)",
    )
    .await
    .expect("seed photos");

    // Photo 1: rejected with default 30 days, then back-date the
    // permanent_delete_after so the sweep finds it expired.
    apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
        .await
        .expect("reject old");
    pool.execute(
        "UPDATE cull_bin SET permanent_delete_after = '2000-01-01T00:00:00Z' WHERE photo_id = 1",
    )
    .await
    .expect("back-date");

    // Photo 2: rejected with a 60-day retention. Sweep should NOT touch it
    // even though the default is 30 — retention is per-row, not global.
    apply_verdict(&pool, 2, Verdict::RejectA, CullReason::NearDup, 60)
        .await
        .expect("reject fresh");

    // Photo 3: rejected just now. Sweep leaves alone.
    apply_verdict(&pool, 3, Verdict::RejectA, CullReason::User, 7)
        .await
        .expect("reject extra");

    let receipt = sweep_expired(&pool).await.expect("sweep");
    assert_eq!(receipt.deleted_photo_count, 1, "one expired row swept");
    assert_eq!(receipt.freed_bytes, 1000);

    // Bin now has photos 2 + 3; photo 1 is gone from photos AND cull_bin.
    let remaining: Vec<i64> = list_bin(&pool, CullFilter::All)
        .await
        .expect("list")
        .into_iter()
        .map(|r| r.photo_id)
        .collect();
    assert_eq!(remaining, vec![3, 2], "2 and 3 preserved, newest first");

    // Photo 1's row is gone from `photos` (FK cascade via the sweep's
    // delete_forever path).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE id = 1")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 0);
}
