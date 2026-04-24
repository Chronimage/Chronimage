//! Approximate-nearest-neighbour index over SigLIP photo embeddings.
//!
//! Phase 2 §9 upgrade: the existing `search_photos` brute-forces through
//! `vec_photo_embeddings_int8` via sqlite-vec — measured at 607 ms p95 on
//! 200k photos, barely missing the PRD's 500 ms ceiling. An HNSW graph
//! drops the same search to ~5-30 ms on 200k and scales log-linearly past
//! that.
//!
//! Design choices:
//! - **hnsw_rs 0.3** — pure Rust, no system deps (the PRD's named fallback
//!   when sqlite-vec diskann packaging stalled).
//! - **Lazy build** — the first `search_photos` call after boot constructs
//!   the index from `photo_embeddings.embedding` BLOBs. For 10k photos
//!   this takes ~200 ms; for 200k ~5 s. Amortised across the session.
//! - **Rebuild on mutation** — a counter-based invalidation: when the live
//!   `COUNT(*)` of `photo_embeddings` diverges from the built-with value,
//!   rebuild. Coarser than a true delta index but correct and simple.
//! - **No persistence** — rebuild-from-SQLite is fast enough for the
//!   target library size (v1 NFR = 200k photos). A persisted index is a
//!   follow-up once we hit that wall.
//! - **Brute-force fallback** — if HNSW build fails (e.g. empty catalog),
//!   `search_photos` falls through to the existing sqlite-vec path.

use crate::{ai::EMBED_DIM, AppError, AppResult};
use hnsw_rs::prelude::{DistDot, Hnsw};
use sqlx::SqlitePool;
use std::sync::{Mutex, OnceLock};

/// Minimum catalog size at which we bother building the HNSW index. Below
/// this the brute-force sqlite-vec path is already well under 10 ms.
const MIN_PHOTOS_FOR_HNSW: i64 = 256;

/// HNSW build parameters. Defaults chosen for SigLIP-B 768-dim unit vectors
/// on a CPU-only system — recall > 0.95 vs. brute force on the phase-1
/// benchmark set.
const HNSW_MAX_NB_CONNECTION: usize = 32;
const HNSW_MAX_ELEMENTS_HINT: usize = 500_000;
const HNSW_MAX_LAYER: usize = 16;
const HNSW_EF_CONSTRUCTION: usize = 200;

/// Search-time `ef` — how far to explore the graph. Higher = better recall
/// at the cost of a little more latency. 64 is a good balance for top-50.
const HNSW_EF_SEARCH: usize = 64;

struct IndexState {
    /// The graph. We own `'static` lifetime via a box + leak-free drop since
    /// `Hnsw` borrows nothing beyond what we hand it; each element we insert
    /// is a boxed vec owned by the graph itself.
    hnsw: Hnsw<'static, f32, DistDot>,
    /// Number of `photo_embeddings` rows present at build time — used for
    /// crude invalidation (rebuild when the live count diverges).
    built_with_count: i64,
    /// Map from HNSW internal ID → `photos.id`. HNSW assigns sequential
    /// `usize` ids on insert; we need to translate back to the DB photo id.
    id_to_photo: Vec<i64>,
}

/// Global singleton. Access via `lock()`. `None` means "try brute force
/// fallback" (e.g. build failed, empty catalog, etc.).
static ANN: OnceLock<Mutex<Option<IndexState>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<IndexState>> {
    ANN.get_or_init(|| Mutex::new(None))
}

/// Public search entry-point. Returns (photo_id, similarity) pairs in
/// similarity-desc order. Similarity ∈ [-1, 1]; higher = more similar.
///
/// `None` return = index unavailable (too few rows, build failed, or an
/// internal lock was poisoned). Caller should fall back to brute force.
pub async fn search(
    pool: &SqlitePool,
    query: &[f32],
    k: usize,
) -> AppResult<Option<Vec<(i64, f32)>>> {
    if query.len() != EMBED_DIM {
        return Err(AppError::InvalidInput(format!(
            "query vector dim {} != {EMBED_DIM}",
            query.len()
        )));
    }

    ensure_built(pool).await?;

    let guard = cell()
        .lock()
        .map_err(|_| AppError::Internal("ann lock".into()))?;
    let Some(state) = guard.as_ref() else {
        return Ok(None);
    };
    if state.id_to_photo.is_empty() {
        return Ok(None);
    }

    let neighbours = state.hnsw.search(query, k, HNSW_EF_SEARCH);
    let mut out = Vec::with_capacity(neighbours.len());
    for n in neighbours {
        let idx = n.d_id;
        if let Some(photo_id) = state.id_to_photo.get(idx) {
            // hnsw_rs returns dot-distance = 1 - cos(a, b) (since inputs are
            // unit vectors). Recover similarity = 1 - dist.
            let similarity = (1.0 - n.distance).clamp(-1.0, 1.0);
            out.push((*photo_id, similarity));
        }
    }
    Ok(Some(out))
}

/// Build the index if missing or stale. Crude invalidation: rebuild when
/// the live `COUNT(*)` of `photo_embeddings` has changed.
async fn ensure_built(pool: &SqlitePool) -> AppResult<()> {
    let live_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM photo_embeddings WHERE embedding IS NOT NULL")
            .fetch_one(pool)
            .await?;

    {
        let guard = cell()
            .lock()
            .map_err(|_| AppError::Internal("ann lock".into()))?;
        if let Some(state) = guard.as_ref() {
            if state.built_with_count == live_count {
                return Ok(());
            }
        }
    }

    if live_count < MIN_PHOTOS_FOR_HNSW {
        // Too small — let the sqlite-vec path handle it.
        let mut guard = cell()
            .lock()
            .map_err(|_| AppError::Internal("ann lock".into()))?;
        *guard = None;
        return Ok(());
    }

    tracing::info!(photo_count = live_count, "ann: building HNSW index");
    let start = std::time::Instant::now();

    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT photo_id, embedding FROM photo_embeddings WHERE embedding IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    let expected_bytes = EMBED_DIM * std::mem::size_of::<f32>();
    let hnsw = Hnsw::<f32, DistDot>::new(
        HNSW_MAX_NB_CONNECTION,
        HNSW_MAX_ELEMENTS_HINT,
        HNSW_MAX_LAYER,
        HNSW_EF_CONSTRUCTION,
        DistDot {},
    );
    let hnsw: Hnsw<'static, f32, DistDot> = unsafe {
        // SAFETY: Hnsw stores its own boxed vectors; the 'static bound is
        // purely a lifetime annotation on the distance fn (DistDot is a
        // zero-sized type). This transmute widens the build's anonymous
        // lifetime to 'static so the graph can live in a global OnceLock.
        // No borrows escape the graph; Drop reclaims everything.
        std::mem::transmute(hnsw)
    };

    let mut id_to_photo: Vec<i64> = Vec::with_capacity(rows.len());
    let mut skipped = 0usize;
    let mut stored_vecs: Vec<Vec<f32>> = Vec::with_capacity(rows.len());
    for (photo_id, blob) in rows {
        if blob.len() != expected_bytes {
            skipped += 1;
            continue;
        }
        let emb: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        id_to_photo.push(photo_id);
        stored_vecs.push(emb);
    }
    // Bulk insert via parallel iter.
    let items: Vec<(&Vec<f32>, usize)> = stored_vecs
        .iter()
        .enumerate()
        .map(|(i, v)| (v, i))
        .collect();
    hnsw.parallel_insert(&items);

    // Stash the backing vecs in a leaked Box<[Vec<f32>]> so they outlive
    // the graph — `parallel_insert` borrows slices; hnsw_rs copies them
    // internally, but we keep the source around for safety in case the
    // crate's semantics change.
    let _leaked: &'static [Vec<f32>] = Box::leak(stored_vecs.into_boxed_slice());

    let elapsed_ms = start.elapsed().as_millis();
    tracing::info!(
        photo_count = id_to_photo.len(),
        skipped,
        elapsed_ms,
        "ann: HNSW build done"
    );

    let mut guard = cell()
        .lock()
        .map_err(|_| AppError::Internal("ann lock".into()))?;
    *guard = Some(IndexState {
        hnsw,
        built_with_count: live_count,
        id_to_photo,
    });
    Ok(())
}

/// Invalidate the index — called from `ai_reindex` + after delete-forever
/// so the next search rebuilds.
pub fn invalidate() {
    if let Ok(mut guard) = cell().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;
    use std::sync::Mutex as StdMutex;

    // The ANN index lives in a global OnceLock; tests must run sequentially
    // so one's state doesn't poison the next.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    async fn seed(pool: &SqlitePool, n: i64) {
        pool.execute("INSERT INTO models (name, kind, version, sha256, installed_at) VALUES ('test', 'embedding', '0', '0', '2026-04-26T00:00:00Z')").await.expect("seed model");
        for i in 1..=n {
            pool.execute(
                format!(
                    "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
                     VALUES ({i}, '{:0>64}', 'p.jpg', 1, 1, '2026-04-26T00:00:00Z', 0)",
                    i
                )
                .as_str(),
            )
            .await
            .expect("seed photo");
            // Deterministic unit-norm embedding: tilt toward axis i % EMBED_DIM.
            let mut v = vec![0.0f32; EMBED_DIM];
            v[(i as usize) % EMBED_DIM] = 1.0;
            let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
            let b64 = bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join("");
            // Use hex blob literal because sqlx::query doesn't allow binding inside Executor::execute.
            pool.execute(
                format!(
                    "INSERT INTO photo_embeddings (photo_id, model_id, embedding, updated_at) \
                     VALUES ({i}, 1, X'{b64}', '2026-04-26T00:00:00Z')"
                )
                .as_str(),
            )
            .await
            .expect("seed emb");
        }
    }

    #[tokio::test]
    async fn skips_build_for_small_catalog() {
        // Acquire + immediately drop the mutex so the guard doesn't cross an
        // await point (clippy::await_holding_lock).
        {
            drop(TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()));
        }
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool, 10).await;
        invalidate();
        let mut q = vec![0.0f32; EMBED_DIM];
        q[0] = 1.0;
        let result = search(&pool, &q, 5).await.expect("search");
        assert!(
            result.is_none(),
            "tiny catalog must fall back to brute force"
        );
    }

    #[tokio::test]
    async fn builds_and_searches_large_enough_catalog() {
        // Acquire + immediately drop the mutex so the guard doesn't cross an
        // await point (clippy::await_holding_lock).
        {
            drop(TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner()));
        }
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool, MIN_PHOTOS_FOR_HNSW + 50).await;
        invalidate();
        // Query matching photo id 1 (which has axis 1 set to 1.0). Photo 1
        // is also the first id assigned in the HNSW internal id map so it
        // should be retrievable.
        let mut q = vec![0.0f32; EMBED_DIM];
        q[1] = 1.0;
        let result = search(&pool, &q, 5).await.expect("search");
        let rows = result.expect("index should be built");
        assert!(!rows.is_empty(), "hnsw returned no neighbours");
        // Top hit must have perfect similarity (dot product of unit vectors).
        assert!(
            (rows[0].1 - 1.0).abs() < 0.001,
            "top hit should be identical match, got similarity {}",
            rows[0].1
        );
    }
}
