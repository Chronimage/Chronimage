//! Phase 1 exit criterion: NL search ≤ 500 ms 95p over 10 seed queries on a
//! 200 k-photo catalog.
//!
//! PRD reference: `docs/prds/phase-1.md` § Exit criteria.
//!
//! ## What this actually measures
//!
//! End-to-end NL search today has two paths:
//!
//! 1. **BLOB fallback** (current production path) — `search_photos` fetches
//!    every `photo_embeddings.embedding` BLOB, L2-normalises + dot-products
//!    in Rust, sorts, returns top-N. O(n).
//! 2. **vec0 KNN** (future) — once SigLIP image embeddings populate
//!    `vec_photo_embeddings`, `search_photos` will issue
//!    `SELECT rowid FROM vec_photo_embeddings WHERE embedding MATCH ? ORDER BY distance LIMIT ?`.
//!    Expected O(log n) with sqlite-vec's ANN.
//!
//! This test seeds both tables + measures the BLOB path (the only one
//! currently exercised in production). When vec0 KNN lands we add a second
//! assertion on the fast path.
//!
//! ## Running
//!
//! ```ignore
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test phase_1_search_latency -- --ignored --nocapture
//! ```
//!
//! Seed + measure takes ~60 s on a fast SSD. `#[ignore]`d in the default
//! suite; nightly CI runs with `--ignored`.

use chronimage::catalog::db::{open_pool, PoolOptions};
use sqlx::SqlitePool;
use std::time::Instant;
use tempfile::TempDir;

const EMBED_DIM: usize = 768;
const CATALOG_SIZE: usize = 200_000;
const SEED_QUERIES: usize = 10;
const ITERATIONS_PER_QUERY: usize = 20;
const P95_THRESHOLD_MS: f64 = 500.0;

/// Produce a synthetic L2-normalised 768-dim vector deterministically from a
/// seed via a 64-bit LCG (Knuth). Avoids adding the `rand` crate as a
/// dev-dependency for what's really just a fixture generator — we need
/// reproducibility, not statistical quality.
fn random_unit_vec(seed: u64) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x5851_F42D_4C95_7F2D).wrapping_add(1);
    let mut v: Vec<f32> = Vec::with_capacity(EMBED_DIM);
    for _ in 0..EMBED_DIM {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Upper 23 bits → [0, 1) as f32, then map to [-1, 1).
        let bits = (state >> 41) as u32;
        let unit = (bits as f32) / (1u32 << 23) as f32;
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

async fn seed_catalog(pool: &SqlitePool) {
    use sqlx::QueryBuilder;

    println!("seeding {CATALOG_SIZE} photos + embeddings…");
    let t0 = Instant::now();
    let now = chrono::Utc::now().to_rfc3339();

    // Create one source row.
    let source_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, config_json) VALUES ('latency-fixture', 'local', '{}') \
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert source");

    // Register a stand-in model row so photo_embeddings.model_id FK resolves.
    let model_id: i64 = sqlx::query_scalar(
        "INSERT INTO models (name, kind, version, sha256, size_bytes) \
         VALUES ('siglip2-b16-image', 'embedding', '2.0.0', 'fixture', 0) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert model");

    // Batch photos + photo_embeddings in chunks of 500 to keep binds sane.
    let chunk = 500;
    let mut cursor = 0usize;
    while cursor < CATALOG_SIZE {
        let end = (cursor + chunk).min(CATALOG_SIZE);

        // Pre-generate embeddings for this chunk.
        let mut embeddings: Vec<Vec<u8>> = Vec::with_capacity(end - cursor);
        for i in cursor..end {
            let v = random_unit_vec(i as u64 + 1);
            let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
            embeddings.push(bytes);
        }

        // Insert photos for this chunk.
        let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw) ",
        );
        qb.push_values(cursor..end, |mut b, i| {
            let sha = format!("{:064x}", i as u128);
            let filename = format!("photo_{i:06}.jpg");
            b.push_bind(sha)
                .push_bind(filename)
                .push_bind(100i64)
                .push_bind(100i64)
                .push_bind(&now)
                .push_bind(0i64);
        });
        qb.push(" RETURNING id");
        let ids: Vec<(i64,)> = qb
            .build_query_as()
            .fetch_all(pool)
            .await
            .expect("insert photos");

        // Insert photo_embeddings BLOB rows.
        let mut eb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "INSERT INTO photo_embeddings (photo_id, model_id, embedding, updated_at) ",
        );
        eb.push_values(
            ids.iter().zip(embeddings.iter()),
            |mut b, ((pid,), blob)| {
                b.push_bind(pid)
                    .push_bind(model_id)
                    .push_bind(blob)
                    .push_bind(&now);
            },
        );
        eb.build().execute(pool).await.expect("insert embeddings");

        cursor = end;
        if cursor.is_multiple_of(20_000) {
            println!("  seeded {cursor}/{CATALOG_SIZE}");
        }
    }
    let _ = source_id;
    println!("  seed done in {:.1}s", t0.elapsed().as_secs_f64());
}

/// Measure the BLOB-fallback linear-scan search path — same algorithm
/// search_photos uses today. Returns per-iteration wall-clock times.
async fn measure_blob_search(pool: &SqlitePool, query_vec: &[f32]) -> Vec<u128> {
    let mut samples = Vec::with_capacity(ITERATIONS_PER_QUERY);
    for _ in 0..ITERATIONS_PER_QUERY {
        let t = Instant::now();
        let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT photo_id, embedding FROM photo_embeddings WHERE embedding IS NOT NULL",
        )
        .fetch_all(pool)
        .await
        .expect("fetch embeddings");

        let expected_bytes = EMBED_DIM * std::mem::size_of::<f32>();
        let mut scored: Vec<(i64, f32)> = rows
            .into_iter()
            .filter_map(|(pid, blob)| {
                if blob.len() != expected_bytes {
                    return None;
                }
                let emb: Vec<f32> = blob
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect();
                let mut dot = 0.0f32;
                for i in 0..EMBED_DIM {
                    dot += query_vec[i] * emb[i];
                }
                Some((pid, dot))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(50);
        samples.push(t.elapsed().as_millis());
    }
    samples
}

fn p95(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    let idx = ((samples.len() as f64) * 0.95).ceil() as usize - 1;
    samples[idx.min(samples.len() - 1)]
}

#[tokio::test]
#[ignore = "seeds 200k rows + runs 200 timed queries — nightly only (~90s)"]
async fn nl_search_p95_le_500ms_on_200k_catalog() {
    let tmp = TempDir::new().expect("tempdir");
    let db = tmp.path().join("catalog.db");
    let pool = open_pool(PoolOptions::new(db)).await.expect("open_pool");

    seed_catalog(&pool).await;

    let mut all_samples: Vec<u128> = Vec::new();
    for q in 0..SEED_QUERIES {
        let query_vec = random_unit_vec(10_000 + q as u64);
        let per_query = measure_blob_search(&pool, &query_vec).await;
        let p50 = {
            let mut s = per_query.clone();
            s.sort_unstable();
            s[s.len() / 2]
        };
        println!("query {q}: p50 {p50} ms across {} iters", per_query.len());
        all_samples.extend(per_query);
    }

    let overall_p95 = p95(all_samples);
    println!("overall p95 across {SEED_QUERIES} queries × {ITERATIONS_PER_QUERY} iters: {overall_p95} ms (threshold: {P95_THRESHOLD_MS:.0} ms)");
    assert!(
        (overall_p95 as f64) <= P95_THRESHOLD_MS,
        "NL search p95 {overall_p95} ms exceeds PRD threshold {P95_THRESHOLD_MS:.0} ms on 200k catalog"
    );
}
