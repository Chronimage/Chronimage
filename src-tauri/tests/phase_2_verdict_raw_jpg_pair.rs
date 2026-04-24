//! Phase 2 acceptance test: RejectA on a RAW+JPG pair drops only the RAW row
//! (the target of `apply_verdict`); the paired JPG row is untouched.
//! `cull_bin.source_copies_frozen_json` captures the RAW's source_copies so
//! a later restore would re-link them.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::cull::verdict::{apply_verdict, CullReason, FrozenCopy, Verdict};
use sqlx::Executor;

#[tokio::test]
async fn reject_a_on_raw_jpg_pair_drops_only_raw() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");

    // Two rows: id=1 (RAW, .arw), id=2 (JPG, .jpg), paired via paired_photo_id.
    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
         VALUES \
           (1, 'a11111111111111111111111111111111111111111111111111111111111aaaa', 'x.arw', 6000, 4000, '2026-04-26T00:00:00Z', 1), \
           (2, 'b22222222222222222222222222222222222222222222222222222222222bbbb', 'x.jpg', 6000, 4000, '2026-04-26T00:00:00Z', 0)",
    )
    .await
    .expect("seed photos");
    pool.execute(
        "UPDATE photos SET paired_photo_id = 2 WHERE id = 1; \
         UPDATE photos SET paired_photo_id = 1 WHERE id = 2;",
    )
    .await
    .expect("link pair");

    // Seed a source + a live source_copies row for the RAW only — that's
    // what the frozen snapshot must capture.
    pool.execute(
        "INSERT INTO sources (id, name, kind, status, config_json, created_at) \
         VALUES (1, 'local', 'local', 'idle', '{}', '2026-04-26T00:00:00Z')",
    )
    .await
    .expect("seed source");
    pool.execute(
        "INSERT INTO source_copies (photo_id, source_id, path, verified_sha256, last_seen_at) \
         VALUES (1, 1, 'D:/photos/x.arw', 'a11111111111111111111111111111111111111111111111111111111111aaaa', '2026-04-26T00:00:00Z')",
    )
    .await
    .expect("seed copy");

    // RejectA targets photo 1 (the RAW).
    let receipt = apply_verdict(&pool, 1, Verdict::RejectA, CullReason::Blur, 30)
        .await
        .expect("verdict");
    assert_eq!(receipt.rejected_ids, vec![1]);

    // Photo 1 is still present in `photos` (rejection stages to cull_bin,
    // doesn't wipe the row) AND lives in cull_bin now. Photo 2 remains
    // untouched and NOT in the bin.
    let in_bin: Vec<i64> = sqlx::query_scalar("SELECT photo_id FROM cull_bin ORDER BY photo_id")
        .fetch_all(&pool)
        .await
        .expect("fetch cull_bin");
    assert_eq!(in_bin, vec![1], "only the RAW should be in the bin");

    // Frozen JSON must contain the raw's source_copies row.
    let frozen_json: String =
        sqlx::query_scalar("SELECT source_copies_frozen_json FROM cull_bin WHERE photo_id = 1")
            .fetch_one(&pool)
            .await
            .expect("fetch frozen");
    let frozen: Vec<FrozenCopy> = serde_json::from_str(&frozen_json).expect("parse frozen");
    assert_eq!(frozen.len(), 1);
    assert_eq!(frozen[0].photo_id, 1);
    assert_eq!(frozen[0].path.as_deref(), Some("D:/photos/x.arw"));
}
