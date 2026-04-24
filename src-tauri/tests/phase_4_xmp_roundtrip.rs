//! Phase 4 §7 acceptance test: importing a photo with a matching
//! `<stem>.xmp` sidecar containing `xmp:Rating=4` + `dc:subject=[portrait,
//! golden-hour]` results in DB rows with rating=4 + two `tags` rows with
//! kind='user'. Also covers the normalise-label path (`Red` → `red`).
//!
//! Write-out (the other half of the PRD §7 round-trip test) is deferred.

use chronimage::catalog::db::{open_pool, PoolOptions};
use chronimage::xmp::{apply_to_photo, parse_str, read_sidecar, sidecar_for};
use sqlx::Executor;
use tempfile::tempdir;

const SAMPLE_XMP: &str = r#"<?xml version='1.0' encoding='UTF-8'?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
           xmlns:dc="http://purl.org/dc/elements/1.1/"
           xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description rdf:about=""
                     xmp:Rating="4"
                     xmp:Label="Red">
      <dc:subject>
        <rdf:Bag>
          <rdf:li>portrait</rdf:li>
          <rdf:li>golden-hour</rdf:li>
        </rdf:Bag>
      </dc:subject>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#;

#[tokio::test]
async fn full_import_side_car_roundtrip() {
    let tmp = tempdir().expect("tmp");
    let photo_path = tmp.path().join("IMG_0042.jpg");
    std::fs::write(&photo_path, b"fake jpg bytes").expect("write photo");
    let sidecar_path = tmp.path().join("IMG_0042.xmp");
    std::fs::write(&sidecar_path, SAMPLE_XMP).expect("write xmp");

    // 1. sidecar_for finds the matching sidecar.
    let found = sidecar_for(&photo_path).expect("sidecar_for returned Some");
    assert_eq!(found, sidecar_path);

    // 2. read_sidecar parses rating + label + subjects.
    let data = read_sidecar(&sidecar_path)
        .expect("read_sidecar ok")
        .expect("Some");
    assert_eq!(data.rating, Some(4));
    assert_eq!(data.color_label.as_deref(), Some("red"));
    assert_eq!(
        data.subjects,
        vec!["portrait".to_string(), "golden-hour".into()]
    );

    // 3. apply_to_photo writes everything into the right columns + tags.
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
         VALUES (42, '4242424242424242424242424242424242424242424242424242424242424242', 'IMG_0042.jpg', 100, 100, '2026-10-01T00:00:00Z', 0)",
    )
    .await
    .expect("seed photo");

    apply_to_photo(&pool, 42, &data).await.expect("apply");

    let (rating, label): (i64, Option<String>) =
        sqlx::query_as("SELECT rating, color_label FROM photos WHERE id = 42")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rating, 4);
    assert_eq!(label.as_deref(), Some("red"));

    let tags: Vec<String> = sqlx::query_scalar(
        "SELECT label FROM tags WHERE photo_id = 42 AND kind = 'user' ORDER BY label",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(tags, vec!["golden-hour".to_string(), "portrait".into()]);
}

#[tokio::test]
async fn reimport_does_not_duplicate_tags() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    pool.execute(
        "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
         VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'p.jpg', 100, 100, '2026-10-01T00:00:00Z', 0)",
    )
    .await
    .expect("seed");

    let data = parse_str(SAMPLE_XMP);
    apply_to_photo(&pool, 1, &data).await.unwrap();
    apply_to_photo(&pool, 1, &data).await.unwrap();
    apply_to_photo(&pool, 1, &data).await.unwrap();

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE photo_id = 1 AND kind = 'user'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2, "INSERT OR IGNORE keeps the set unique");
}
