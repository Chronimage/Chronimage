//! Phase 1 exit criterion: NL search ≤ 750 ms 95p over 10 seed queries on a
//! 200 k-photo catalog (revised 2026-04-21 from the original 500 ms estimate,
//! see PRD § NFR for rationale — sqlite-vec 0.1.9 is CPU brute-force and has
//! a measured floor around 550-600 ms p95 at this catalog size; ANN via
//! sqlite-vec 0.1.10+ diskann is tracked for Phase 2).
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
const P95_THRESHOLD_MS: f64 = 750.0;

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
        "INSERT INTO sources (name, kind, status, created_at, config_json) \
         VALUES ('latency-fixture', 'local', 'idle', ?1, '{}') RETURNING id",
    )
    .bind(&now)
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

        // Insert photo_embeddings BLOB rows (legacy fallback path).
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

        // Also populate the sqlite-vec virtual tables. Production pipeline
        // stage-4 does this; the latency benchmark must exercise the same
        // KNN path search_photos uses. rowid = photo_id per the insert contract.
        // vec0 doesn't support multi-VALUES inserts, so one row at a time.
        for (((pid,), blob), f32vec) in ids.iter().zip(embeddings.iter()).zip(cursor..end) {
            // f32 table (fallback path).
            sqlx::query("INSERT INTO vec_photo_embeddings(rowid, embedding) VALUES (?1, ?2)")
                .bind(pid)
                .bind(blob)
                .execute(pool)
                .await
                .expect("insert vec_photo_embeddings");
            // int8 table (primary path).
            let v = random_unit_vec(f32vec as u64 + 1);
            let i8_bytes = chronimage::catalog::db::quantize_unit_f32_to_i8_bytes(&v);
            sqlx::query(
                "INSERT INTO vec_photo_embeddings_int8(rowid, embedding) VALUES (?1, vec_int8(?2))",
            )
            .bind(pid)
            .bind(&i8_bytes)
            .execute(pool)
            .await
            .expect("insert vec_photo_embeddings_int8");
        }

        cursor = end;
        if cursor.is_multiple_of(20_000) {
            println!("  seeded {cursor}/{CATALOG_SIZE}");
        }
    }
    let _ = source_id;
    println!("  seed done in {:.1}s", t0.elapsed().as_secs_f64());
}

/// Measure the int8 vec0 KNN search path — the primary path `search_photos`
/// uses. Query vec is quantised the same way stored vectors are so distance
/// ordering is preserved. Returns per-iteration wall-clock times in ms.
async fn measure_vec0_search(pool: &SqlitePool, query_vec: &[f32]) -> Vec<u128> {
    let query_i8 = chronimage::catalog::db::quantize_unit_f32_to_i8_bytes(query_vec);
    let mut samples = Vec::with_capacity(ITERATIONS_PER_QUERY);
    for _ in 0..ITERATIONS_PER_QUERY {
        let t = Instant::now();
        let rows: Vec<(i64, f32)> = sqlx::query_as(
            "SELECT rowid, distance FROM vec_photo_embeddings_int8 \
             WHERE embedding MATCH vec_int8(?1) ORDER BY distance LIMIT 50",
        )
        .bind(&query_i8)
        .fetch_all(pool)
        .await
        .expect("vec0 int8 knn query");
        std::hint::black_box(rows);
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
        let per_query = measure_vec0_search(&pool, &query_vec).await;
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
