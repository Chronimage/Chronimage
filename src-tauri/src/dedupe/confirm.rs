//! SigLIP cosine-similarity duplicate confirmation layer.
//!
//! This is the second dedupe layer. `phash` gives a cheap bit-distance
//! pre-filter; `confirm` groups remaining candidates by semantic similarity
//! using L2-normalised SigLIP embeddings.
//!
//! # Performance
//!
//! The current implementation is O(n²) over all embeddings in the catalog.
//! This is acceptable for n < 10,000 (a typical hobbyist library). For
//! larger catalogs the pairwise loop should be replaced with an ANN query
//! against the `vec_photo_embeddings` sqlite-vec virtual table (which stores
//! the same vectors in an HNSW index).

use crate::{AppError, AppResult};
use serde::Serialize;
use sqlx::SqlitePool;

/// The semantic similarity bucket for a duplicate group.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum DupeKind {
    /// Cosine similarity ≥ 0.95 — essentially the same scene/content.
    Exact,
    /// Cosine similarity in [0.90, 0.95) — very similar but not identical.
    Near,
}

/// A set of photos that are semantically similar to each other.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// IDs of the photos in the group (always ≥ 2).
    pub photo_ids: Vec<i64>,
    /// Highest pairwise cosine similarity found within the group.
    pub max_similarity: f64,
    /// Whether the group is exact or near-duplicate.
    pub kind: DupeKind,
}

// ── Internal types ────────────────────────────────────────────────────────────

#[derive(Debug)]
struct PhotoEmbedding {
    photo_id: i64,
    /// L2-normalised 768-dimensional SigLIP vector.
    vector: Vec<f32>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load all SigLIP embeddings from the pool, compute pairwise cosine
/// similarity, and return groups of photos whose similarity meets or exceeds
/// `min_similarity`.
///
/// RAW+JPG pairs (`photos.paired_photo_id IS NOT NULL`) are excluded because
/// they are intentional stacks, not duplicates.
///
/// # Errors
///
/// Returns [`AppError::Db`] on database failure, or [`AppError::InvalidInput`]
/// if an embedding BLOB is the wrong length.
pub async fn find_duplicate_groups(
    pool: &SqlitePool,
    min_similarity: f64,
) -> AppResult<Vec<DuplicateGroup>> {
    let rows = fetch_embeddings(pool).await?;

    if rows.len() < 2 {
        return Ok(Vec::new());
    }

    let embeddings = decode_and_normalise(rows)?;
    let groups = group_by_similarity(&embeddings, min_similarity);
    Ok(groups)
}

// ── Database ──────────────────────────────────────────────────────────────────

struct RawRow {
    photo_id: i64,
    embedding: Vec<u8>,
}

async fn fetch_embeddings(pool: &SqlitePool) -> AppResult<Vec<RawRow>> {
    // Join photos so we can filter out RAW+JPG stacks. We take any embedding
    // row per photo (no model_id filter) so the function works even when
    // multiple embedding models are loaded.
    //
    // Using the dynamic query builder (not sqlx::query!) so no DATABASE_URL
    // is required at compile time.
    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT pe.photo_id, pe.embedding
         FROM   photo_embeddings pe
         JOIN   photos p ON p.id = pe.photo_id
         WHERE  pe.embedding IS NOT NULL
           AND  p.paired_photo_id IS NULL",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(photo_id, embedding)| RawRow {
            photo_id,
            embedding,
        })
        .collect())
}

// ── Embedding decoding + normalisation ───────────────────────────────────────

const EMBEDDING_DIM: usize = 768;
const EMBEDDING_BYTES: usize = EMBEDDING_DIM * 4; // f32 is 4 bytes

fn decode_and_normalise(rows: Vec<RawRow>) -> AppResult<Vec<PhotoEmbedding>> {
    rows.into_iter()
        .map(|row| {
            if row.embedding.len() != EMBEDDING_BYTES {
                return Err(AppError::InvalidInput(format!(
                    "photo_id {} has embedding of {} bytes, expected {}",
                    row.photo_id,
                    row.embedding.len(),
                    EMBEDDING_BYTES,
                )));
            }

            let mut vector: Vec<f32> = row
                .embedding
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            l2_normalise(&mut vector);

            Ok(PhotoEmbedding {
                photo_id: row.photo_id,
                vector,
            })
        })
        .collect()
}

/// Normalise `v` to unit length in-place. If the vector is zero-length (all
/// components zero) it is left unchanged — it will produce zero cosine
/// similarity against everything, so it will not join any group.
fn l2_normalise(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ── Grouping ──────────────────────────────────────────────────────────────────

/// Greedy union-find grouping over pairwise cosine similarities.
///
/// For two photos i and j, cosine(i, j) = dot(normalised_i, normalised_j).
///
/// Time complexity: O(n²) dot products, O(n·α(n)) union-find operations.
fn group_by_similarity(embeddings: &[PhotoEmbedding], min_similarity: f64) -> Vec<DuplicateGroup> {
    let n = embeddings.len();
    let mut parent: Vec<usize> = (0..n).collect();
    // Track the maximum similarity for the root of each component.
    let mut max_sim: Vec<f64> = vec![0.0_f64; n];

    for i in 0..n {
        for j in (i + 1)..n {
            let sim = cosine_similarity(&embeddings[i].vector, &embeddings[j].vector);
            if sim >= min_similarity {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    // Union: attach the smaller-index root under the larger to
                    // keep the tree shallow. We just need any stable root.
                    parent[rj] = ri;
                    let merged_sim = max_sim[ri].max(max_sim[rj]).max(sim);
                    max_sim[ri] = merged_sim;
                } else {
                    // Already same component; still update max_sim.
                    max_sim[ri] = max_sim[ri].max(sim);
                }
            }
        }
    }

    // Collect components.
    let mut components: std::collections::HashMap<usize, (Vec<i64>, f64)> =
        std::collections::HashMap::new();
    for (idx, emb) in embeddings.iter().enumerate() {
        let root = find(&mut parent, idx);
        let entry = components.entry(root).or_insert((Vec::new(), 0.0_f64));
        entry.0.push(emb.photo_id);
        entry.1 = entry.1.max(max_sim[root]);
    }

    let mut groups: Vec<DuplicateGroup> = components
        .into_values()
        .filter(|(ids, _)| ids.len() >= 2)
        .map(|(mut photo_ids, sim)| {
            photo_ids.sort_unstable();
            let kind = if sim >= 0.95 {
                DupeKind::Exact
            } else {
                DupeKind::Near
            };
            DuplicateGroup {
                photo_ids,
                max_similarity: sim,
                kind,
            }
        })
        .collect();

    // Sort highest similarity first.
    groups.sort_by(|a, b| {
        b.max_similarity
            .partial_cmp(&a.max_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    groups
}

/// Path-compressing find for union-find.
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]]; // path halving
        x = parent[x];
    }
    x
}

/// Dot product of two unit-length vectors == cosine similarity.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(ai, bi)| (*ai as f64) * (*bi as f64))
        .sum()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Unit helpers ─────────────────────────────────────────────────────────

    fn make_embedding(photo_id: i64, raw: &[f32]) -> PhotoEmbedding {
        let mut vector = raw.to_vec();
        l2_normalise(&mut vector);
        PhotoEmbedding { photo_id, vector }
    }

    fn unit_vec(dim: usize, hot: usize) -> Vec<f32> {
        // One-hot vector — already unit length.
        let mut v = vec![0.0_f32; dim];
        v[hot] = 1.0;
        v
    }

    // ── l2_normalise ─────────────────────────────────────────────────────────

    #[test]
    fn normalise_produces_unit_vector() {
        let mut v = vec![3.0_f32, 4.0]; // magnitude = 5
        l2_normalise(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "norm should be 1.0, got {norm}");
    }

    #[test]
    fn normalise_zero_vector_is_noop() {
        let mut v = vec![0.0_f32; 4];
        l2_normalise(&mut v);
        assert!(v.iter().all(|&x| x == 0.0));
    }

    // ── cosine_similarity ────────────────────────────────────────────────────

    #[test]
    fn cosine_identical_unit_vectors_is_one() {
        let a = unit_vec(8, 0);
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-9, "expected 1.0, got {sim}");
    }

    #[test]
    fn cosine_orthogonal_vectors_is_zero() {
        let a = unit_vec(8, 0);
        let b = unit_vec(8, 1);
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-9, "expected 0.0, got {sim}");
    }

    // ── group_by_similarity ──────────────────────────────────────────────────

    #[test]
    fn empty_embeddings_returns_empty_groups() {
        let groups = group_by_similarity(&[], 0.90);
        assert!(groups.is_empty());
    }

    #[test]
    fn single_embedding_returns_empty_groups() {
        let embs = vec![make_embedding(1, &unit_vec(EMBEDDING_DIM, 0))];
        let groups = group_by_similarity(&embs, 0.90);
        assert!(groups.is_empty());
    }

    #[test]
    fn two_identical_embeddings_form_one_exact_group() {
        let v = unit_vec(EMBEDDING_DIM, 0);
        let embs = vec![make_embedding(1, &v), make_embedding(2, &v)];
        let groups = group_by_similarity(&embs, 0.90);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.photo_ids.len(), 2);
        assert!(
            (g.max_similarity - 1.0).abs() < 1e-9,
            "expected max_similarity ≈ 1.0, got {}",
            g.max_similarity
        );
        assert_eq!(g.kind, DupeKind::Exact);
    }

    #[test]
    fn orthogonal_embeddings_produce_no_groups() {
        // Each photo has a distinct one-hot vector — all pairwise sims = 0.
        let embs: Vec<PhotoEmbedding> = (0..4)
            .map(|i| make_embedding(i as i64 + 1, &unit_vec(EMBEDDING_DIM, i)))
            .collect();
        let groups = group_by_similarity(&embs, 0.90);
        assert!(groups.is_empty());
    }

    #[test]
    fn near_duplicate_classified_as_near() {
        // Build a vector that is almost identical to another but not quite 0.95.
        // We'll construct two vectors with cosine ≈ 0.92.
        let mut a = vec![0.0_f32; EMBEDDING_DIM];
        a[0] = 1.0;

        // Slightly tilted: cos(θ) = a·b = cos(~23°) ≈ 0.92
        let angle: f32 = 0.392_f32; // ≈ 22.5 degrees in radians
        let mut b = vec![0.0_f32; EMBEDDING_DIM];
        b[0] = angle.cos();
        b[1] = angle.sin();

        let embs = vec![make_embedding(1, &a), make_embedding(2, &b)];

        let groups = group_by_similarity(&embs, 0.90);
        assert_eq!(groups.len(), 1, "expected one group");
        assert_eq!(groups[0].kind, DupeKind::Near);
    }

    #[test]
    fn below_threshold_not_grouped() {
        // a and b have cosine ≈ 0.707 (45°) — below 0.90.
        let mut a = vec![0.0_f32; EMBEDDING_DIM];
        a[0] = 1.0;
        let mut b = vec![0.0_f32; EMBEDDING_DIM];
        b[0] = 1.0_f32 / std::f32::consts::SQRT_2;
        b[1] = 1.0_f32 / std::f32::consts::SQRT_2;

        let embs = vec![make_embedding(1, &a), make_embedding(2, &b)];

        let groups = group_by_similarity(&embs, 0.90);
        assert!(groups.is_empty());
    }

    #[test]
    fn groups_sorted_by_max_similarity_descending() {
        // Group A: photo 1 & 2, identical (sim = 1.0).
        // Group B: photo 3 & 4, near-dupe (sim ≈ 0.92).
        let v_exact = unit_vec(EMBEDDING_DIM, 0);

        let angle: f32 = 0.392_f32;
        let mut b_near = vec![0.0_f32; EMBEDDING_DIM];
        b_near[2] = angle.cos(); // different dimensions than group A
        b_near[3] = angle.sin();
        let mut a_near = vec![0.0_f32; EMBEDDING_DIM];
        a_near[2] = 1.0;

        let embs = vec![
            make_embedding(1, &v_exact),
            make_embedding(2, &v_exact),
            make_embedding(3, &a_near),
            make_embedding(4, &b_near),
        ];

        let groups = group_by_similarity(&embs, 0.90);
        assert_eq!(groups.len(), 2);
        assert!(
            groups[0].max_similarity >= groups[1].max_similarity,
            "groups not sorted descending"
        );
        assert_eq!(groups[0].kind, DupeKind::Exact);
    }

    // ── decode_and_normalise ─────────────────────────────────────────────────

    #[test]
    fn decode_rejects_wrong_blob_length() {
        let row = RawRow {
            photo_id: 99,
            embedding: vec![0u8; 100], // wrong length
        };
        let err = decode_and_normalise(vec![row]).unwrap_err();
        assert!(
            matches!(err, AppError::InvalidInput(_)),
            "expected InvalidInput, got {err:?}"
        );
    }

    #[test]
    fn decode_roundtrips_known_vector() {
        // Encode a known f32 value and check it decodes correctly.
        let val: f32 = 1.0;
        let mut blob = val.to_le_bytes().to_vec();
        // Pad to EMBEDDING_BYTES with zeros.
        blob.extend(vec![0u8; EMBEDDING_BYTES - 4]);

        let rows = vec![RawRow {
            photo_id: 7,
            embedding: blob,
        }];
        let decoded = decode_and_normalise(rows).expect("decode");
        assert_eq!(decoded.len(), 1);
        // After normalisation the first component should be 1.0 (already unit).
        assert!(
            (decoded[0].vector[0] - 1.0).abs() < 1e-6,
            "first component should be 1.0 after normalisation, got {}",
            decoded[0].vector[0]
        );
    }

    // ── Integration-style tests against an in-memory DB ──────────────────────

    async fn make_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory pool");

        // Minimal schema — just enough for find_duplicate_groups.
        sqlx::query(
            "CREATE TABLE photos (
               id               INTEGER PRIMARY KEY,
               paired_photo_id  INTEGER
             )",
        )
        .execute(&pool)
        .await
        .expect("create photos");

        sqlx::query(
            "CREATE TABLE photo_embeddings (
               photo_id   INTEGER NOT NULL,
               model_id   INTEGER NOT NULL DEFAULT 1,
               embedding  BLOB,
               updated_at TEXT NOT NULL DEFAULT '',
               PRIMARY KEY (photo_id, model_id)
             )",
        )
        .execute(&pool)
        .await
        .expect("create photo_embeddings");

        pool
    }

    fn encode_vec(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn padded_unit_vec(hot: usize) -> Vec<u8> {
        let mut v = vec![0.0_f32; EMBEDDING_DIM];
        v[hot] = 1.0;
        encode_vec(&v)
    }

    #[tokio::test]
    async fn empty_pool_returns_empty_vec() {
        let pool = make_test_pool().await;
        let groups = find_duplicate_groups(&pool, 0.90)
            .await
            .expect("find_duplicate_groups");
        assert!(groups.is_empty(), "expected no groups from empty catalog");
    }

    #[tokio::test]
    async fn two_identical_embeddings_are_grouped() {
        let pool = make_test_pool().await;

        let blob = padded_unit_vec(0);

        for id in [1_i64, 2_i64] {
            sqlx::query("INSERT INTO photos (id, paired_photo_id) VALUES (?1, NULL)")
                .bind(id)
                .execute(&pool)
                .await
                .expect("insert photo");

            sqlx::query("INSERT INTO photo_embeddings (photo_id, embedding) VALUES (?1, ?2)")
                .bind(id)
                .bind(&blob)
                .execute(&pool)
                .await
                .expect("insert embedding");
        }

        let groups = find_duplicate_groups(&pool, 0.90)
            .await
            .expect("find_duplicate_groups");

        assert_eq!(groups.len(), 1, "expected exactly one group");
        assert_eq!(groups[0].photo_ids.len(), 2);
        assert_eq!(groups[0].kind, DupeKind::Exact);
    }

    #[tokio::test]
    async fn paired_photos_are_excluded() {
        let pool = make_test_pool().await;

        let blob = padded_unit_vec(0);

        // Photo 1: unpaired; photo 2: paired (RAW+JPG stack — should be skipped).
        sqlx::query("INSERT INTO photos (id, paired_photo_id) VALUES (1, NULL)")
            .execute(&pool)
            .await
            .expect("insert photo 1");
        sqlx::query("INSERT INTO photos (id, paired_photo_id) VALUES (2, 1)")
            .execute(&pool)
            .await
            .expect("insert photo 2");

        for id in [1_i64, 2_i64] {
            sqlx::query("INSERT INTO photo_embeddings (photo_id, embedding) VALUES (?1, ?2)")
                .bind(id)
                .bind(&blob)
                .execute(&pool)
                .await
                .expect("insert embedding");
        }

        // Only photo 1 should be loaded; not enough to form a group.
        let groups = find_duplicate_groups(&pool, 0.90)
            .await
            .expect("find_duplicate_groups");

        assert!(groups.is_empty(), "paired photos must not form groups");
    }

    #[tokio::test]
    async fn orthogonal_embeddings_produce_no_groups_in_db() {
        let pool = make_test_pool().await;

        for id in 0_i64..4 {
            sqlx::query("INSERT INTO photos (id, paired_photo_id) VALUES (?1, NULL)")
                .bind(id + 1)
                .execute(&pool)
                .await
                .expect("insert photo");

            let blob = padded_unit_vec(id as usize);
            sqlx::query("INSERT INTO photo_embeddings (photo_id, embedding) VALUES (?1, ?2)")
                .bind(id + 1)
                .bind(&blob)
                .execute(&pool)
                .await
                .expect("insert embedding");
        }

        let groups = find_duplicate_groups(&pool, 0.90)
            .await
            .expect("find_duplicate_groups");
        assert!(
            groups.is_empty(),
            "orthogonal embeddings must not be grouped"
        );
    }
}
