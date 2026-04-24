//! Phase 2 acceptance test: reject → restore → photo is present again in
//! the catalog with its original source_copies still attached (because the
//! Cull Bin only stages an audit row; source_copies aren't touched until
//! the permanent delete).

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::cull::bin::{list as list_bin, restore, CullFilter};
use chronimage::cull::verdict::{apply_verdict, CullReason, Verdict};
use sqlx::Executor;

#[tokio::test]
async fn reject_then_restore_preserves_source_copies() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");

    pool.execute(
        "INSERT INTO sources (id, name, kind, status, config_json, created_at) \
         VALUES (1, 'local', 'local', 'idle', '{}', '2026-04-26T00:00:00Z')",
    )
    .await
    .expect("seed source");
    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
         VALUES (42, '4242424242424242424242424242424242424242424242424242424242424242', 'round.jpg', 100, 100, '2026-04-26T00:00:00Z', 0)",
    )
    .await
    .expect("seed photo");
    pool.execute(
        "INSERT INTO source_copies (photo_id, source_id, path, verified_sha256, last_seen_at) \
         VALUES (42, 1, 'D:/photos/round.jpg', '4242424242424242424242424242424242424242424242424242424242424242', '2026-04-26T00:00:00Z')",
    )
    .await
    .expect("seed copy");

    // Reject → lands in bin.
    apply_verdict(&pool, 42, Verdict::RejectA, CullReason::User, 30)
        .await
        .expect("reject");
    assert_eq!(list_bin(&pool, CullFilter::All).await.unwrap().len(), 1);

    // Source_copies row untouched even while in the bin (rejection doesn't
    // unlink files on disk or clear metadata — that's the whole point of
    // the recoverable bin).
    let live_copies: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM source_copies WHERE photo_id = 42")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(live_copies, 1);

    // Restore → row is out of cull_bin, photo itself never left the main
    // table, source_copies still intact.
    let r = restore(&pool, &[42]).await.expect("restore");
    assert_eq!(r.restored_count, 1);
    assert!(r.skipped.is_empty());
    assert_eq!(list_bin(&pool, CullFilter::All).await.unwrap().len(), 0);

    let post_photo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE id = 42")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(post_photo, 1, "photo row never left");

    let post_copies: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM source_copies WHERE photo_id = 42")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(post_copies, 1, "source_copies still linked after restore");
}
