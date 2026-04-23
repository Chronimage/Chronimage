//! Bridge between the pure HDBSCAN routine in [`super::cluster`] and the
//! `faces` + `clusters` SQLite tables.
//!
//! Responsibilities:
//! 1. Load every face embedding from the DB.
//! 2. Run HDBSCAN ([`cluster_faces`]).
//! 3. Match new cluster groups to existing `clusters` rows by centroid cosine
//!    similarity (threshold 0.60) so user-assigned names survive re-clusters.
//! 4. Insert new `clusters` rows for un-matched groups.
//! 5. Update `faces.cluster_id` + `clusters.cover_face_id` + `clusters.photo_count`.
//! 6. Prune empty unnamed clusters (keep empty named ones as "held" placeholders).
//!
//! The import pipeline calls [`reeval_clusters`] once at the end of every
//! `execute_pipeline` run. It's also exposed as a Tauri command
//! (`recluster_faces`) for manual re-triggers from the People screen.

use super::cluster::{cluster_faces, ClusterParams, FaceInput, FACE_EMBED_DIM};
use crate::{AppError, AppResult};
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Cosine-similarity floor for "this new cluster is the same person as an
/// existing cluster". 0.60 is conservative — ArcFace same-identity pairs
/// typically score > 0.70. Below this we create a new cluster row.
const NAME_INHERIT_COSINE_THRESHOLD: f32 = 0.60;

/// Snapshot of an existing `clusters` row + derived centroid, keyed by
/// cluster id in `reeval_clusters`. `name` is carried for completeness so
/// future work (e.g. surfacing preserved names in the receipt) doesn't need
/// a second DB round-trip; currently unread by the matching logic.
#[allow(dead_code)]
struct ExistingClusterInfo {
    name: Option<String>,
    is_named: bool,
    centroid: Option<Vec<f32>>,
}

/// Receipt returned to the frontend after a recluster run.
#[derive(Debug, Serialize)]
pub struct ReclusterReceipt {
    pub total_faces: i64,
    pub clustered_faces: i64,
    pub cluster_count: i64,
    pub named_preserved: i64,
    pub new_clusters: i64,
    pub pruned_empty: i64,
    pub elapsed_ms: u64,
}

/// Full rebuild of cluster assignments.
///
/// Safe to call while the DB is in use — writes happen in a single transaction
/// at the end. HDBSCAN is CPU-bound and runs outside the transaction window.
pub async fn reeval_clusters(pool: &SqlitePool) -> AppResult<ReclusterReceipt> {
    let t0 = std::time::Instant::now();

    // ── Step 1: load all faces with embeddings ────────────────────────────
    let rows: Vec<(i64, i64, Vec<u8>, f64)> = sqlx::query_as(
        "SELECT id, photo_id, embedding, quality FROM faces WHERE embedding IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    let total_faces = rows.len() as i64;

    // Note: we don't early-return when total_faces == 0 — we still want to
    // prune orphan unnamed clusters from the `clusters` table below.

    // Decode byte BLOBs into f32 vectors. 512 * 4 = 2048 bytes per embedding.
    let mut inputs: Vec<FaceInput> = Vec::with_capacity(rows.len());
    let mut quality_by_face: HashMap<i64, f64> = HashMap::with_capacity(rows.len());
    for (face_id, photo_id, blob, quality) in &rows {
        let embedding = decode_f32_blob(blob).ok_or_else(|| {
            AppError::Internal(format!(
                "face_id {face_id}: embedding blob is not a multiple of 4 bytes ({} bytes)",
                blob.len()
            ))
        })?;
        if embedding.len() != FACE_EMBED_DIM {
            // Not fatal — skip faces with unexpected dim. This can happen if
            // the embedding model ever changes. Caller sees reduced count.
            tracing::warn!(
                face_id,
                dim = embedding.len(),
                expected = FACE_EMBED_DIM,
                "face embedding dim mismatch, skipping"
            );
            continue;
        }
        inputs.push(FaceInput {
            face_id: *face_id,
            photo_id: *photo_id,
            embedding,
        });
        quality_by_face.insert(*face_id, *quality);
    }

    // ── Step 2: run HDBSCAN ───────────────────────────────────────────────
    let assignments = cluster_faces(&inputs, &ClusterParams::default())?;

    // Group faces by assigned cluster_id (skip noise points).
    let mut groups: HashMap<i64, Vec<i64>> = HashMap::new();
    for a in &assignments {
        if a.cluster_id == -1 {
            continue; // noise — leave cluster_id NULL
        }
        groups.entry(a.cluster_id).or_default().push(a.face_id);
    }

    let clustered_faces = groups.values().map(|v| v.len() as i64).sum::<i64>();
    let new_group_count = groups.len() as i64;

    // ── Step 3: compute centroid per new group ───────────────────────────
    // Keyed by the HDBSCAN stable cluster_id (= smallest face_id in the group).
    let input_by_face: HashMap<i64, &FaceInput> = inputs.iter().map(|i| (i.face_id, i)).collect();
    let new_centroids: HashMap<i64, Vec<f32>> = groups
        .iter()
        .map(|(cid, face_ids)| (*cid, compute_centroid(face_ids, &input_by_face)))
        .collect();

    // ── Step 4: load existing clusters + face embeddings so we can compute
    //    their current centroids and try to inherit names ──────────────────
    let existing_clusters: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as("SELECT id, name, is_named FROM clusters")
            .fetch_all(pool)
            .await?;

    // Map existing_cluster_id -> (name, is_named, centroid). Centroids come
    // from whatever `faces.cluster_id = X` rows currently exist — which may
    // be empty for a freshly-seeded DB. Empty existing clusters carry a name
    // but no centroid; we can't inherit-by-centroid, but if an existing named
    // cluster has no surviving faces we'll keep the placeholder row anyway.
    let existing_face_rows: Vec<(i64, i64, Vec<u8>)> =
        sqlx::query_as("SELECT cluster_id, id, embedding FROM faces WHERE cluster_id IS NOT NULL AND embedding IS NOT NULL")
            .fetch_all(pool)
            .await?;

    let mut existing_face_embeds: HashMap<i64, Vec<Vec<f32>>> = HashMap::new();
    for (cid, _fid, blob) in &existing_face_rows {
        if let Some(v) = decode_f32_blob(blob) {
            if v.len() == FACE_EMBED_DIM {
                existing_face_embeds.entry(*cid).or_default().push(v);
            }
        }
    }

    let mut existing_info: HashMap<i64, ExistingClusterInfo> = HashMap::new();
    for (id, name, is_named) in &existing_clusters {
        let centroid = existing_face_embeds.get(id).map(|vecs| mean_vector(vecs));
        existing_info.insert(
            *id,
            ExistingClusterInfo {
                name: name.clone(),
                is_named: *is_named != 0,
                centroid,
            },
        );
    }

    // ── Step 5: assign each new group to an existing cluster or create one.
    //    Use Hungarian-ish greedy matching — best pair first, then exclude.
    let mut named_preserved = 0_i64;
    let mut new_clusters_created = 0_i64;

    // For each new group: hdbscan_id -> existing_cluster_id (may be brand-new).
    let mut hdbscan_to_db_cluster: HashMap<i64, i64> = HashMap::new();

    // Build candidate pairs (new_hdbscan_id, existing_cluster_id, cosine_sim).
    let mut candidate_pairs: Vec<(i64, i64, f32)> = Vec::new();
    for (hdbscan_id, new_centroid) in &new_centroids {
        for (existing_id, info) in &existing_info {
            if let Some(existing_centroid) = &info.centroid {
                let sim = cosine_similarity(new_centroid, existing_centroid);
                if sim >= NAME_INHERIT_COSINE_THRESHOLD {
                    candidate_pairs.push((*hdbscan_id, *existing_id, sim));
                }
            }
        }
    }
    // Greedy: highest similarity first, each existing cluster only claimed once.
    candidate_pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut claimed_existing: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut claimed_new: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (hdbscan_id, existing_id, _sim) in &candidate_pairs {
        if claimed_new.contains(hdbscan_id) || claimed_existing.contains(existing_id) {
            continue;
        }
        hdbscan_to_db_cluster.insert(*hdbscan_id, *existing_id);
        claimed_existing.insert(*existing_id);
        claimed_new.insert(*hdbscan_id);
        if existing_info
            .get(existing_id)
            .is_some_and(|info| info.is_named)
        {
            named_preserved += 1;
        }
    }

    // ── Step 6: single transaction for all writes ───────────────────────
    let mut tx = pool.begin().await?;
    let now = Utc::now().to_rfc3339();

    // Reset cluster_id on every face first — this avoids leaving stale
    // assignments when a face becomes noise or its cluster disappears.
    sqlx::query("UPDATE faces SET cluster_id = NULL")
        .execute(&mut *tx)
        .await?;

    // Insert rows for un-matched new groups and record their DB id.
    for hdbscan_id in groups.keys() {
        if hdbscan_to_db_cluster.contains_key(hdbscan_id) {
            continue;
        }
        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO clusters (name, is_named, photo_count, created_at, updated_at)
             VALUES (NULL, 0, 0, ?1, ?1) RETURNING id",
        )
        .bind(&now)
        .fetch_one(&mut *tx)
        .await?;
        hdbscan_to_db_cluster.insert(*hdbscan_id, new_id);
        new_clusters_created += 1;
    }

    // Assign each face to its matched cluster and track cover candidates.
    let mut cover_by_cluster: HashMap<i64, (i64, f64)> = HashMap::new();
    let mut count_by_cluster: HashMap<i64, i64> = HashMap::new();
    for (hdbscan_id, face_ids) in &groups {
        let db_cluster_id = hdbscan_to_db_cluster[hdbscan_id];
        for face_id in face_ids {
            sqlx::query("UPDATE faces SET cluster_id = ?1 WHERE id = ?2")
                .bind(db_cluster_id)
                .bind(face_id)
                .execute(&mut *tx)
                .await?;
            *count_by_cluster.entry(db_cluster_id).or_insert(0) += 1;
            let quality = quality_by_face.get(face_id).copied().unwrap_or(0.0);
            let best = cover_by_cluster.get(&db_cluster_id).copied();
            if best.is_none_or(|(_, q)| quality > q) {
                cover_by_cluster.insert(db_cluster_id, (*face_id, quality));
            }
        }
    }

    // Update clusters table: cover + photo_count + updated_at.
    for (db_cluster_id, (cover_face_id, _)) in &cover_by_cluster {
        let count = count_by_cluster.get(db_cluster_id).copied().unwrap_or(0);
        // photo_count = DISTINCT photo_id count for that cluster.
        let distinct_photos: i64 =
            sqlx::query_scalar("SELECT COUNT(DISTINCT photo_id) FROM faces WHERE cluster_id = ?1")
                .bind(db_cluster_id)
                .fetch_one(&mut *tx)
                .await?;
        let _ = count; // face_count column computed by SELECT elsewhere
        sqlx::query(
            "UPDATE clusters SET cover_face_id = ?1, photo_count = ?2, updated_at = ?3 WHERE id = ?4",
        )
        .bind(cover_face_id)
        .bind(distinct_photos)
        .bind(&now)
        .bind(db_cluster_id)
        .execute(&mut *tx)
        .await?;
    }

    // Prune clusters that ended up with zero faces AND were not user-named.
    // Named-but-empty clusters are preserved as "held" placeholders — the
    // user's label shouldn't vanish just because we recomputed clusters.
    let pruned = sqlx::query(
        "DELETE FROM clusters
         WHERE is_named = 0
           AND NOT EXISTS (SELECT 1 FROM faces WHERE faces.cluster_id = clusters.id)",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    tx.commit().await?;

    Ok(ReclusterReceipt {
        total_faces,
        clustered_faces,
        cluster_count: new_group_count,
        named_preserved,
        new_clusters: new_clusters_created,
        pruned_empty: pruned,
        elapsed_ms: t0.elapsed().as_millis() as u64,
    })
}

// ── small helpers ────────────────────────────────────────────────────────

fn decode_f32_blob(blob: &[u8]) -> Option<Vec<f32>> {
    if !blob.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(out)
}

fn compute_centroid(face_ids: &[i64], inputs: &HashMap<i64, &FaceInput>) -> Vec<f32> {
    let mut acc = vec![0.0_f32; FACE_EMBED_DIM];
    let mut n = 0_usize;
    for fid in face_ids {
        if let Some(fi) = inputs.get(fid) {
            for (a, b) in acc.iter_mut().zip(fi.embedding.iter()) {
                *a += *b;
            }
            n += 1;
        }
    }
    if n == 0 {
        return acc;
    }
    let inv = 1.0 / n as f32;
    for v in &mut acc {
        *v *= inv;
    }
    // Re-normalise (sum of unit vectors isn't necessarily unit length).
    l2_normalise(&mut acc);
    acc
}

fn mean_vector(vecs: &[Vec<f32>]) -> Vec<f32> {
    if vecs.is_empty() {
        return vec![];
    }
    let dim = vecs[0].len();
    let mut acc = vec![0.0_f32; dim];
    for v in vecs {
        for (a, b) in acc.iter_mut().zip(v.iter()) {
            *a += *b;
        }
    }
    let inv = 1.0 / vecs.len() as f32;
    for v in &mut acc {
        *v *= inv;
    }
    l2_normalise(&mut acc);
    acc
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_normalise(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use tempfile::TempDir;

    async fn mk_pool() -> (TempDir, SqlitePool) {
        let tmp = TempDir::new().expect("tempdir");
        let pool = open_pool(PoolOptions::new(tmp.path().join("c.db")))
            .await
            .expect("pool");
        (tmp, pool)
    }

    /// Build a unit-length 512-dim vector with a single dominant direction.
    /// `seed` controls which axis carries most of the weight.
    ///
    /// `jitter` spreads the vector across a small set of neighbouring axes
    /// so within-cluster points aren't identical (HDBSCAN needs some
    /// spatial variance to establish density — identical points collapse
    /// to noise under the mutual-reachability metric).
    fn synthesize_embedding(seed: usize, jitter: f32) -> Vec<f32> {
        let mut v = vec![0.0_f32; FACE_EMBED_DIM];
        v[seed % FACE_EMBED_DIM] = 1.0;
        // Spread across 5 adjacent axes with decaying weights scaled by
        // jitter. Different seeds → vastly different directions; same-seed
        // vectors differ by a small amount proportional to jitter.
        for k in 1..=5 {
            v[(seed + k) % FACE_EMBED_DIM] = jitter * (6.0 - k as f32);
        }
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut v {
            *x /= n;
        }
        v
    }

    async fn insert_photo(pool: &SqlitePool, sha: &str) -> i64 {
        let now = Utc::now().to_rfc3339();
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, size_bytes)
             VALUES (?1, 'x.jpg', 100, 100, ?2, 0, 1) RETURNING id",
        )
        .bind(sha)
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("photo")
    }

    async fn insert_face(pool: &SqlitePool, photo_id: i64, embedding: &[f32], quality: f64) -> i64 {
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let now = Utc::now().to_rfc3339();
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO faces
             (photo_id, bbox_x, bbox_y, bbox_w, bbox_h, quality, embedding, created_at)
             VALUES (?1, 0, 0, 10, 10, ?2, ?3, ?4) RETURNING id",
        )
        .bind(photo_id)
        .bind(quality)
        .bind(&blob)
        .bind(&now)
        .fetch_one(pool)
        .await
        .expect("face")
    }

    #[tokio::test]
    async fn reeval_clusters_empty_db_returns_zeros() {
        let (_tmp, pool) = mk_pool().await;
        let receipt = reeval_clusters(&pool).await.expect("reeval");
        assert_eq!(receipt.total_faces, 0);
        assert_eq!(receipt.cluster_count, 0);
    }

    #[tokio::test]
    async fn reeval_clusters_groups_similar_faces() {
        let (_tmp, pool) = mk_pool().await;
        // 6 faces along axis 10 (one cluster), 6 along axis 300 (another),
        // 1 stray along axis 100 (noise). min_cluster_size default = 5.
        for i in 0..6 {
            let p = insert_photo(&pool, &format!("a{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(10, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        for i in 0..6 {
            let p = insert_photo(&pool, &format!("b{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(300, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        let p = insert_photo(&pool, "c0").await;
        insert_face(&pool, p, &synthesize_embedding(100, 0.05), 0.5).await;

        let receipt = reeval_clusters(&pool).await.expect("reeval");
        assert_eq!(receipt.total_faces, 13);
        assert_eq!(receipt.cluster_count, 2, "expected 2 clusters");
        assert_eq!(receipt.clustered_faces, 12, "stray is noise");
    }

    #[tokio::test]
    async fn reeval_clusters_preserves_named_cluster() {
        let (_tmp, pool) = mk_pool().await;
        // Round 1: seed 6 "Ari" faces + 5 unrelated faces so HDBSCAN has
        // density contrast. (With fewer than min_cluster_size=5 unrelated
        // points the library collapses the lone "Ari" blob to noise.)
        for i in 0..6 {
            let p = insert_photo(&pool, &format!("a{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(10, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        for i in 0..5 {
            let p = insert_photo(&pool, &format!("noise-r1-{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(300, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        let r1 = reeval_clusters(&pool).await.expect("reeval-1");
        assert_eq!(r1.cluster_count, 2, "expected Ari + noise-r1 clusters");

        // Name the Ari cluster (the one holding the photos with 'a' prefix).
        sqlx::query(
            "UPDATE clusters SET name = 'Ari', is_named = 1
             WHERE id = (
               SELECT f.cluster_id FROM faces f
               JOIN photos p ON p.id = f.photo_id
               WHERE p.sha256 LIKE 'a%'
               LIMIT 1
             )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Round 2: add 6 more faces of the same person + 6 of a different one.
        for i in 6..12 {
            let p = insert_photo(&pool, &format!("a{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(10, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        for i in 0..6 {
            let p = insert_photo(&pool, &format!("b{i}")).await;
            insert_face(
                &pool,
                p,
                &synthesize_embedding(300, 0.02 + 0.01 * i as f32),
                0.5,
            )
            .await;
        }
        let r2 = reeval_clusters(&pool).await.expect("reeval-2");
        assert_eq!(r2.cluster_count, 2);
        assert!(r2.named_preserved >= 1, "Ari should be preserved");

        // Verify the name survived.
        let ari_faces: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM faces f JOIN clusters c ON c.id = f.cluster_id
             WHERE c.name = 'Ari'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ari_faces, 12, "all 12 Ari-like faces inherited the name");
    }

    #[tokio::test]
    async fn reeval_clusters_prunes_empty_unnamed() {
        let (_tmp, pool) = mk_pool().await;
        // Seed a stray unnamed cluster with no faces pointing at it.
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO clusters (name, is_named, photo_count, created_at, updated_at)
             VALUES (NULL, 0, 0, ?1, ?1)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clusters")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 1);

        let r = reeval_clusters(&pool).await.expect("reeval");
        assert!(r.pruned_empty >= 1);

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clusters")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, 0);
    }

    #[tokio::test]
    async fn reeval_clusters_keeps_empty_named_cluster() {
        let (_tmp, pool) = mk_pool().await;
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO clusters (name, is_named, photo_count, created_at, updated_at)
             VALUES ('Ghost', 1, 0, ?1, ?1)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let _r = reeval_clusters(&pool).await.expect("reeval");
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM clusters WHERE name = 'Ghost'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1, "named cluster must survive prune");
    }
}
