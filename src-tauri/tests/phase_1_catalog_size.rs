//! Phase 1 exit criterion: catalog DB size ≤ 2% of library bytes on a 10k-photo fixture.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria.
//!
//! Generates a synthetic fixture programmatically (10k 100×100 JPEGs) to avoid
//! committing large binaries to the repo. Marked `#[ignore]` because fixture
//! generation + full pipeline import takes ~90 s even on fast hardware; the
//! nightly CI workflow runs this with `--ignored`.
//!
//! Run locally with:
//! `cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_catalog_size -- --ignored --nocapture`

use chronimage::{
    catalog::db::{open_pool, PoolOptions},
    import::run_pipeline_headless,
};
use image::{ImageBuffer, Rgb};
use std::{path::Path, sync::Arc};
use tempfile::TempDir;

/// Number of synthetic photos to generate. 10 000 is the PRD's fixture size.
const PHOTO_COUNT: usize = 10_000;

/// Upper bound on catalog.db / library-bytes ratio.
const MAX_RATIO: f64 = 0.02;

/// Build an `Rgb8` image that varies per-index so every photo is unique
/// *and* compresses to a realistic JPEG size. The earlier gradient-only
/// version produced 1.4 KB files, which let per-row catalog overhead
/// dominate the ratio — not representative of the PRD's real-library NFR.
/// 512×512 with per-pixel noise encodes to ~40-80 KB, matching the
/// lower end of real phone / compact-camera output.
fn synthesize_jpeg(index: usize) -> Vec<u8> {
    const SIZE: u32 = 512;
    let mut buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(SIZE, SIZE);
    // 64-bit LCG seeded from the index — deterministic + incompressible noise.
    let mut state = (index as u64)
        .wrapping_mul(0x5851_F42D_4C95_7F2D)
        .wrapping_add(1);
    for pixel in buf.pixels_mut() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let r = (state >> 40) as u8;
        let g = (state >> 32) as u8;
        let b = (state >> 24) as u8;
        *pixel = Rgb([r, g, b]);
    }
    let mut out = Vec::with_capacity(80_000);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&buf)
        .expect("encode jpeg");
    out
}

fn dir_size_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    for entry in std::fs::read_dir(root).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let meta = entry.metadata().expect("metadata");
        if meta.is_file() {
            total += meta.len();
        } else if meta.is_dir() {
            total += dir_size_bytes(&entry.path());
        }
    }
    total
}

#[tokio::test]
#[ignore = "generates 10k photos + runs full import — nightly only (~90s)"]
async fn catalog_db_size_le_2_percent_of_library_bytes() {
    // Keep stage-4 AI enrichment from loading the developer's real models
    // (nima.onnx etc.) against our 10k synthetic fixtures.
    // SAFETY: set before spawning any tokio tasks.
    unsafe {
        std::env::set_var(
            "CHRONIMAGE_MODELS_DIR",
            "\\nonexistent\\chronimage-test-models",
        );
    }
    let fixture_dir = TempDir::new().expect("tempdir");
    let library_root = fixture_dir.path().join("library");
    std::fs::create_dir_all(&library_root).expect("mkdir library");

    // 1. Generate the fixture.
    for i in 0..PHOTO_COUNT {
        let path = library_root.join(format!("photo_{i:05}.jpg"));
        std::fs::write(&path, synthesize_jpeg(i)).expect("write jpeg");
    }
    let library_bytes = dir_size_bytes(&library_root);
    println!("fixture: {PHOTO_COUNT} photos, {library_bytes} bytes on disk");

    // 2. Open a fresh catalog DB (tempdir, not the user's real catalog).
    let db_path = fixture_dir.path().join("catalog.db");
    let pool = open_pool(PoolOptions::new(db_path.clone()))
        .await
        .expect("open_pool");

    // 3. Create a source row for the import.
    let now_ts = chrono::Utc::now().to_rfc3339();
    let source_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, created_at, config_json) \
         VALUES (?1, 'local', 'idle', ?2, '{}') RETURNING id",
    )
    .bind("phase-1-catalog-size-fixture")
    .bind(&now_ts)
    .fetch_one(&pool)
    .await
    .expect("insert source");

    // 4. Run the headless pipeline over the synthetic library.
    let noop: Arc<dyn Fn(chronimage::import::ImportProgress) + Send + Sync> = Arc::new(|_| {});
    let result = run_pipeline_headless(source_id, library_root.clone(), pool.clone(), noop)
        .await
        .expect("run_pipeline_headless");
    assert_eq!(
        result.imported_count as usize, PHOTO_COUNT,
        "expected all photos imported"
    );

    // 5. Checkpoint WAL so the -wal file flushes into the main db before
    //    measuring.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE);")
        .execute(&pool)
        .await
        .expect("wal checkpoint");
    pool.close().await;

    // 6. Measure catalog footprint (main + -wal + -shm).
    let mut catalog_bytes = 0u64;
    for suffix in ["", "-wal", "-shm"] {
        let p = fixture_dir.path().join(format!("catalog.db{suffix}"));
        if let Ok(m) = std::fs::metadata(&p) {
            catalog_bytes += m.len();
        }
    }

    let ratio = catalog_bytes as f64 / library_bytes as f64;
    println!(
        "catalog: {catalog_bytes} bytes, ratio: {:.4} (threshold: {MAX_RATIO})",
        ratio
    );
    assert!(
        ratio <= MAX_RATIO,
        "catalog footprint {catalog_bytes} B exceeded {}% of library {library_bytes} B (ratio {:.4})",
        MAX_RATIO * 100.0,
        ratio
    );
}
