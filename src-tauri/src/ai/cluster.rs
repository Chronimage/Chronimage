//! HDBSCAN face-cluster assignment over ArcFace 512-dim L2-normalised embeddings.
//!
//! ## Distance metric
//!
//! The `hdbscan` crate (v0.12, MIT OR Apache-2.0) does not expose a Cosine
//! variant in its `DistanceMetric` enum.  Because ArcFace embeddings are already
//! L2-normalised (unit vectors), Euclidean distance is a monotone proxy for
//! cosine distance:
//!
//! ```text
//! d_euc² = 2 − 2·cos_sim
//! ```
//!
//! Cluster shapes on the unit sphere are therefore preserved when using
//! `DistanceMetric::Euclidean`.  No pre-processing beyond keeping the
//! embeddings as `f32` is needed.
//!
//! ## Cluster ID stability
//!
//! HDBSCAN assigns arbitrary contiguous labels (0, 1, 2, …) that can change
//! across runs when the input order changes.  After clustering, the helper
//! [`stable_cluster_ids`] re-keys each label to the **smallest `face_id`** in
//! that cluster.  Noise points (`−1`) are never re-keyed.
//!
//! ## Membership probability
//!
//! The `hdbscan` crate returns hard integer labels only (no soft membership
//! scores).  Assigned points receive `probability = 1.0`; noise points receive
//! `probability = 0.0`.
//!
//! `min_cluster_size` defaults to 5; `min_samples` defaults to `min_cluster_size`.

use crate::{AppError, AppResult};
use hdbscan::{DistanceMetric, Hdbscan, HdbscanHyperParams};
use std::collections::HashMap;

/// Dimensionality of the ArcFace embedding vectors this module consumes.
///
/// Kept in sync with `crate::ai::faces::FACE_EMBED_DIM`; if that constant
/// moves, update the `use` statement below and remove this definition.
pub const FACE_EMBED_DIM: usize = 512;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One face to be clustered.
#[derive(Debug, Clone)]
pub struct FaceInput {
    /// Primary key from the `faces` table.
    pub face_id: i64,
    /// The photo this face crop came from.
    pub photo_id: i64,
    /// L2-normalised ArcFace embedding; must be exactly `FACE_EMBED_DIM` elements.
    pub embedding: Vec<f32>,
}

/// The cluster assignment produced for a single face.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterAssignment {
    pub face_id: i64,
    /// Stable cluster identifier; `-1` means the face is a noise point.
    pub cluster_id: i64,
    /// Soft membership probability in `[0.0, 1.0]`; noise points carry `0.0`.
    pub probability: f32,
}

/// Tuning knobs forwarded to HDBSCAN.
#[derive(Debug, Clone)]
pub struct ClusterParams {
    /// Minimum number of points to form a core; smaller groups become noise.
    pub min_cluster_size: usize,
    /// Minimum samples for density estimation; `None` mirrors `min_cluster_size`.
    pub min_samples: Option<usize>,
}

impl Default for ClusterParams {
    fn default() -> Self {
        Self {
            min_cluster_size: 5,
            min_samples: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Core public API
// ---------------------------------------------------------------------------

/// Cluster face embeddings and return one [`ClusterAssignment`] per input.
///
/// ## Algorithm
///
/// Uses HDBSCAN via the [`hdbscan`] crate (MIT OR Apache-2.0).  Euclidean
/// distance is applied to the already-L2-normalised ArcFace embeddings, which
/// is equivalent to cosine distance for unit vectors.
///
/// ## Errors
///
/// Returns [`AppError::InvalidInput`] when any embedding has a length other
/// than [`FACE_EMBED_DIM`], or [`AppError::Internal`] if the HDBSCAN library
/// returns an error.
pub fn cluster_faces(
    inputs: &[FaceInput],
    params: &ClusterParams,
) -> AppResult<Vec<ClusterAssignment>> {
    if inputs.is_empty() {
        return Ok(vec![]);
    }

    // Validate embedding dimensions up-front so callers get a clear error.
    for fi in inputs {
        if fi.embedding.len() != FACE_EMBED_DIM {
            return Err(AppError::InvalidInput(format!(
                "face_id {} has embedding length {} but expected {}",
                fi.face_id,
                fi.embedding.len(),
                FACE_EMBED_DIM,
            )));
        }
    }

    // Build the data matrix as Vec<Vec<f32>> expected by the hdbscan crate.
    let data: Vec<Vec<f32>> = inputs.iter().map(|fi| fi.embedding.clone()).collect();

    let min_samples = params.min_samples.unwrap_or(params.min_cluster_size);

    let hyper_params = HdbscanHyperParams::builder()
        .min_cluster_size(params.min_cluster_size)
        .min_samples(min_samples)
        .dist_metric(DistanceMetric::Euclidean)
        .build();

    let clusterer = Hdbscan::new(&data, hyper_params);
    let raw_labels: Vec<i32> = clusterer
        .cluster()
        .map_err(|e| AppError::Internal(format!("HDBSCAN clustering failed: {e:?}")))?;

    // Assemble initial assignments.  Raw labels use i32 (from the crate);
    // we cast to i64 for compatibility with our face_id / cluster_id types.
    let mut assignments: Vec<ClusterAssignment> = inputs
        .iter()
        .zip(raw_labels.iter())
        .map(|(fi, &label)| {
            let cluster_id = label as i64;
            let probability = if cluster_id == -1 { 0.0 } else { 1.0 };
            ClusterAssignment {
                face_id: fi.face_id,
                cluster_id,
                probability,
            }
        })
        .collect();

    // Re-key cluster labels to the smallest face_id within each cluster so
    // that IDs are deterministic across re-runs regardless of input ordering.
    let face_id_to_photo_id: HashMap<i64, i64> =
        inputs.iter().map(|fi| (fi.face_id, fi.photo_id)).collect();
    stable_cluster_ids(&mut assignments, &face_id_to_photo_id);

    Ok(assignments)
}

/// Re-key raw HDBSCAN cluster labels to stable IDs.
///
/// HDBSCAN assigns arbitrary integer labels (0, 1, 2, …) that can shift
/// between runs as the input order changes.  This function replaces each label
/// with the **smallest `face_id`** present in that cluster, which is stable as
/// long as `face_id` primary keys are immutable (they are — SQLite ROWID never
/// changes after insert).
///
/// Noise points (`cluster_id == -1`) are left unchanged.
///
/// `face_id_to_photo_id` is accepted for potential future use (e.g. breaking
/// ties at the photo level) but is not required for the current re-keying
/// logic.
pub fn stable_cluster_ids(raw: &mut [ClusterAssignment], _face_id_to_photo_id: &HashMap<i64, i64>) {
    // Step 1: for each raw label, find the minimum face_id in that cluster.
    let mut label_to_min_face: HashMap<i64, i64> = HashMap::new();
    for ca in raw.iter() {
        if ca.cluster_id == -1 {
            continue;
        }
        let entry = label_to_min_face.entry(ca.cluster_id).or_insert(ca.face_id);
        if ca.face_id < *entry {
            *entry = ca.face_id;
        }
    }

    // Step 2: rewrite cluster_ids in-place.
    for ca in raw.iter_mut() {
        if ca.cluster_id == -1 {
            continue;
        }
        if let Some(&stable_id) = label_to_min_face.get(&ca.cluster_id) {
            ca.cluster_id = stable_id;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_embedding(val: f32) -> Vec<f32> {
        vec![val; FACE_EMBED_DIM]
    }

    // ── Original 4 tests (kept unchanged) ───────────────────────────────────

    #[test]
    fn empty_input_returns_empty_output() {
        let result = cluster_faces(&[], &ClusterParams::default()).expect("should succeed");
        assert!(result.is_empty());
    }

    #[test]
    fn wrong_embedding_dim_errors() {
        let bad_input = vec![FaceInput {
            face_id: 1,
            photo_id: 10,
            embedding: vec![0.0_f32; FACE_EMBED_DIM - 1], // one element short
        }];
        let err = cluster_faces(&bad_input, &ClusterParams::default())
            .expect_err("should fail on wrong dim");
        assert!(
            matches!(err, AppError::InvalidInput(_)),
            "expected InvalidInput, got {err:?}"
        );
    }

    #[test]
    fn stable_cluster_ids_reassigns_deterministically() {
        // Two faces, both in raw cluster 0; face_ids are 42 and 7.
        // After stabilisation both should land on cluster_id = 7 (the minimum).
        // A noise face (-1) must remain -1.
        let mut assignments = vec![
            ClusterAssignment {
                face_id: 42,
                cluster_id: 0,
                probability: 0.9,
            },
            ClusterAssignment {
                face_id: 7,
                cluster_id: 0,
                probability: 0.8,
            },
            ClusterAssignment {
                face_id: 99,
                cluster_id: -1,
                probability: 0.0,
            },
        ];

        stable_cluster_ids(&mut assignments, &HashMap::new());

        let ids: Vec<i64> = assignments.iter().map(|a| a.cluster_id).collect();
        assert_eq!(ids[0], 7, "face 42 should be re-keyed to cluster 7");
        assert_eq!(ids[1], 7, "face 7 should remain in cluster 7");
        assert_eq!(ids[2], -1, "noise face must stay -1");
    }

    #[test]
    fn stub_clusters_by_photo_id_deterministically() {
        // Three faces across two photos with well-separated embeddings.
        // With min_cluster_size=2 the two faces in photo 1 form one cluster,
        // and the lone face in photo 2 becomes noise (-1) since it is isolated.
        // We only assert that the result has 3 entries (one per input) and does
        // not error — cluster_id values depend on HDBSCAN internals.
        let inputs = vec![
            FaceInput {
                face_id: 20,
                photo_id: 1,
                embedding: make_embedding(0.1),
            },
            FaceInput {
                face_id: 5,
                photo_id: 1,
                embedding: make_embedding(0.1),
            },
            FaceInput {
                face_id: 30,
                photo_id: 2,
                embedding: make_embedding(0.9),
            },
        ];
        let params = ClusterParams {
            min_cluster_size: 2,
            min_samples: Some(1),
        };
        let result = cluster_faces(&inputs, &params).expect("should succeed");
        assert_eq!(result.len(), 3);
    }

    // ── New tests ────────────────────────────────────────────────────────────

    /// Build a unit-norm embedding whose `1.0` mass sits entirely in a
    /// dedicated 170-dim block for cluster `cluster_idx` (0, 1, or 2).
    /// A tiny per-sample perturbation within that block breaks perfect
    /// degeneracy without moving points across cluster boundaries.
    /// Clusters are orthogonal on the unit sphere — maximum cosine separation.
    fn block_embedding(cluster_idx: usize, sample: usize) -> Vec<f32> {
        let block = FACE_EMBED_DIM / 3; // 170 dims per cluster
        let start = cluster_idx * block;
        let end = if cluster_idx == 2 {
            FACE_EMBED_DIM
        } else {
            start + block
        };

        let mut emb = vec![0.0f32; FACE_EMBED_DIM];
        let base = 1.0f32;
        let noise_scale = 0.001f32;
        for (i, v) in emb[start..end].iter_mut().enumerate() {
            // Deterministic noise: alternating small positive/negative offsets.
            let sign = if (i + sample).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            *v = base + sign * noise_scale * (sample as f32 + 1.0);
        }

        // L2-normalise.
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for x in emb.iter_mut() {
                *x /= norm;
            }
        }
        emb
    }

    /// Synthesise 60 unit-sphere embeddings in 3 well-separated clusters
    /// (20 samples each, orthogonal block structure).  Assert exactly 3
    /// distinct positive cluster_ids and zero noise points.
    #[test]
    fn three_well_separated_clusters_are_detected_correctly() {
        let mut inputs: Vec<FaceInput> = Vec::new();
        let mut face_id: i64 = 1;

        for cluster_idx in 0..3usize {
            for sample in 0..20usize {
                inputs.push(FaceInput {
                    face_id,
                    photo_id: face_id,
                    embedding: block_embedding(cluster_idx, sample),
                });
                face_id += 1;
            }
        }

        let params = ClusterParams {
            min_cluster_size: 5,
            min_samples: Some(3),
        };
        let result = cluster_faces(&inputs, &params).expect("cluster_faces should not fail");

        assert_eq!(result.len(), 60, "must return one assignment per input");

        let noise_count = result.iter().filter(|a| a.cluster_id == -1).count();
        assert_eq!(
            noise_count, 0,
            "no noise points expected with well-separated clusters; got {noise_count}"
        );

        let mut distinct_ids: Vec<i64> = result
            .iter()
            .filter(|a| a.cluster_id >= 0)
            .map(|a| a.cluster_id)
            .collect();
        distinct_ids.sort_unstable();
        distinct_ids.dedup();
        assert_eq!(
            distinct_ids.len(),
            3,
            "expected exactly 3 distinct positive cluster_ids, got {:?}",
            distinct_ids
        );
    }

    /// Two clusters whose centroids have cosine similarity > 0.95.
    ///
    /// When two face-identity groups are nearly indistinguishable in embedding
    /// space, graceful degradation means the function must not panic and must
    /// return at most 2 positive clusters (merged, split, or all-noise are all
    /// acceptable — the algorithm is not required to distinguish them).
    #[test]
    fn two_overlapping_clusters_degrade_gracefully() {
        let mut inputs: Vec<FaceInput> = Vec::new();
        let mut face_id: i64 = 1;

        // Two groups share almost the same direction on the unit sphere.
        // Group 0: all-ones unit vector with tiny per-sample dim-0 offsets.
        // Group 1: same, with tiny per-sample dim-1 offsets.
        // Cosine similarity between any two points across groups is > 0.999.
        let unit_val = 1.0f32 / (FACE_EMBED_DIM as f32).sqrt();

        for group in 0..2usize {
            for sample in 0..20usize {
                let mut emb = vec![unit_val; FACE_EMBED_DIM];
                // Offset in different dim per group so the two groups are
                // geometrically distinct but still extremely close.
                let dim = group * 10 + (sample % 10);
                emb[dim] += 5e-4 * (sample as f32 + 1.0);
                let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 1e-10 {
                    for x in emb.iter_mut() {
                        *x /= norm;
                    }
                }
                inputs.push(FaceInput {
                    face_id,
                    photo_id: face_id,
                    embedding: emb,
                });
                face_id += 1;
            }
        }

        let params = ClusterParams {
            min_cluster_size: 5,
            min_samples: Some(3),
        };
        let result = cluster_faces(&inputs, &params).expect("cluster_faces must not panic");

        assert_eq!(result.len(), 40, "must return one result per input");

        let mut distinct_ids: Vec<i64> = result
            .iter()
            .filter(|a| a.cluster_id >= 0)
            .map(|a| a.cluster_id)
            .collect();
        distinct_ids.sort_unstable();
        distinct_ids.dedup();

        // 0 clusters (all noise), 1 (merged), or 2 (split) are all acceptable.
        assert!(
            distinct_ids.len() <= 2,
            "expected ≤ 2 clusters for near-identical centroids, got {} clusters",
            distinct_ids.len()
        );
    }

    /// 40 tight cluster samples + 3 random-direction outliers.
    /// With `min_cluster_size = 5` the 3 isolated points must be noise (-1).
    #[test]
    fn isolated_outliers_are_marked_as_noise() {
        let mut inputs: Vec<FaceInput> = Vec::new();
        let mut face_id: i64 = 1;

        // 40 tight samples near (1/sqrt(512), 1/sqrt(512), …).
        for sample in 0..40usize {
            let noise = (sample as f32) * 0.0005;
            let mut emb = vec![1.0f32 + noise; FACE_EMBED_DIM];
            let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-10 {
                for x in emb.iter_mut() {
                    *x /= norm;
                }
            }
            inputs.push(FaceInput {
                face_id,
                photo_id: face_id,
                embedding: emb,
            });
            face_id += 1;
        }

        // 3 outlier embeddings pointing in orthogonal directions.
        let outlier_templates: [Vec<f32>; 3] = [
            {
                let mut e = vec![0.0f32; FACE_EMBED_DIM];
                e[0] = 1.0;
                e
            },
            {
                let mut e = vec![0.0f32; FACE_EMBED_DIM];
                e[1] = 1.0;
                e
            },
            {
                let mut e = vec![0.0f32; FACE_EMBED_DIM];
                e[2] = 1.0;
                e
            },
        ];
        // Track the face_ids of the 3 outliers.
        let mut outlier_face_ids: Vec<i64> = Vec::new();
        for emb in outlier_templates {
            outlier_face_ids.push(face_id);
            inputs.push(FaceInput {
                face_id,
                photo_id: face_id,
                embedding: emb,
            });
            face_id += 1;
        }

        let params = ClusterParams {
            min_cluster_size: 5,
            min_samples: Some(3),
        };
        let result = cluster_faces(&inputs, &params).expect("cluster_faces must not fail");
        assert_eq!(result.len(), 43);

        for a in &result {
            if outlier_face_ids.contains(&a.face_id) {
                assert_eq!(
                    a.cluster_id, -1,
                    "outlier face_id {} must be noise, got cluster_id {}",
                    a.face_id, a.cluster_id
                );
                assert_eq!(
                    a.probability, 0.0,
                    "noise face_id {} must have probability 0.0",
                    a.face_id
                );
            }
        }
    }

    /// Cluster once, shuffle the input, cluster again.
    ///
    /// Because HDBSCAN is sensitive to input order on border points, we do
    /// not assert that the exact `cluster_id → face_id` map is identical.
    /// Instead we assert that `stable_cluster_ids` produces *label-independent*
    /// stability: the **collection of face-id sets** (one sorted vec per
    /// positive cluster) is the same regardless of order.  Noise points are
    /// excluded because border membership can flip legitimately.
    ///
    /// Uses the orthogonal block_embedding helper so the 3 clusters are as
    /// far apart as possible — border effects are minimised.
    #[test]
    fn stable_cluster_ids_survive_input_reordering() {
        // 3 orthogonal clusters × 20 samples each = 60 inputs.
        // 20 samples per cluster with min_cluster_size=5 leaves no ambiguity
        // about which points are core members.
        let mut inputs: Vec<FaceInput> = Vec::new();
        let mut face_id: i64 = 1;

        for cluster_idx in 0..3usize {
            for sample in 0..20usize {
                inputs.push(FaceInput {
                    face_id,
                    photo_id: face_id,
                    embedding: block_embedding(cluster_idx, sample),
                });
                face_id += 1;
            }
        }

        let params = ClusterParams {
            min_cluster_size: 5,
            min_samples: Some(3),
        };

        // First run (original order).
        let result_a = cluster_faces(&inputs, &params).expect("first cluster run");
        let sets_a = assignments_to_sorted_sets(&result_a);

        // Reverse the input order — a maximally adversarial reordering.
        let mut shuffled = inputs.clone();
        shuffled.reverse();

        // Second run (reversed order).
        let result_b = cluster_faces(&shuffled, &params).expect("second cluster run");
        let sets_b = assignments_to_sorted_sets(&result_b);

        assert_eq!(
            sets_a, sets_b,
            "collection of face-id sets must be identical regardless of input order"
        );
    }

    /// Collect cluster assignments into a label-independent representation:
    /// a sorted `Vec` of sorted `Vec<face_id>` (one inner vec per positive
    /// cluster).  Noise points are excluded.
    fn assignments_to_sorted_sets(assignments: &[ClusterAssignment]) -> Vec<Vec<i64>> {
        let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
        for a in assignments {
            if a.cluster_id == -1 {
                continue;
            }
            map.entry(a.cluster_id).or_default().push(a.face_id);
        }
        let mut sets: Vec<Vec<i64>> = map
            .into_values()
            .map(|mut v| {
                v.sort_unstable();
                v
            })
            .collect();
        // Sort the outer vec so cluster label ordering does not affect equality.
        sets.sort_unstable();
        sets
    }
}
