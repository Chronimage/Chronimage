//! Phase 3 acceptance test: `paste_edits(ops, photo_ids)` creates one
//! `edits` row per photo with identical `operations_json`, and each
//! photo's `current_edit_id` advances to its new row.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::develop::history::{paste_edits, save};
use chronimage::develop::ops::Operations;
use sqlx::Executor;

#[tokio::test]
async fn paste_onto_n_photos_creates_n_edits_rows_with_identical_json() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");

    // 5 target photos + 1 source photo.
    let mut ids = String::new();
    for i in 1..=6 {
        ids.push_str(&format!(
            "({i}, '{:0>64}', 'p.jpg', 100, 100, '2026-08-01T00:00:00Z', 0)",
            i
        ));
        if i < 6 {
            ids.push_str(", ");
        }
    }
    pool.execute(
        format!(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES {ids}"
        )
        .as_str(),
    )
    .await
    .expect("seed");

    // Source (photo 6) has an edit we'll copy.
    let source_ops = Operations {
        exposure: 0.5,
        contrast: 20.0,
        shadows: 35.0,
        vibrance: 15.0,
        ..Operations::identity()
    };
    save(&pool, 6, &source_ops, Some("source".into()))
        .await
        .expect("save source");

    // Paste onto 1..=5.
    let targets: Vec<i64> = (1..=5).collect();
    let receipt = paste_edits(&pool, &targets, &source_ops)
        .await
        .expect("paste");
    assert_eq!(receipt.pasted_photo_count, 5);
    assert!(receipt.skipped.is_empty());

    // Every target has exactly one edits row with the source's json.
    let source_json = serde_json::to_string(&source_ops).unwrap();
    for target_id in &targets {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, operations_json FROM edits WHERE photo_id = ?1 ORDER BY saved_at",
        )
        .bind(target_id)
        .fetch_all(&pool)
        .await
        .expect("fetch");
        assert_eq!(rows.len(), 1, "photo {target_id} should have 1 edit row");
        assert_eq!(rows[0].1, source_json, "photo {target_id} json mismatch");

        // Pointer must point at the pasted row.
        let cur: Option<i64> =
            sqlx::query_scalar("SELECT current_edit_id FROM photos WHERE id = ?1")
                .bind(target_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cur, Some(rows[0].0));
    }

    // Source (photo 6) still has exactly its own 1 edit (not mutated).
    let source_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edits WHERE photo_id = 6")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(source_rows, 1);
}

#[tokio::test]
async fn paste_skips_missing_photo_ids() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
         VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'p.jpg', 100, 100, '2026-08-01T00:00:00Z', 0)",
    )
    .await
    .expect("seed");

    let ops = Operations {
        exposure: 1.0,
        ..Operations::identity()
    };
    let receipt = paste_edits(&pool, &[1, 42, 99], &ops).await.expect("paste");
    assert_eq!(receipt.pasted_photo_count, 1);
    assert_eq!(receipt.skipped, vec![42, 99]);
}
