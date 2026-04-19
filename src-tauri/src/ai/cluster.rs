//! HDBSCAN face-cluster assignment over ArcFace 512-dim L2-normalised embeddings.
//!
//! Distance metric: cosine (equivalent to Euclidean on L2-normalised vectors).
//! `min_cluster_size` defaults to 5; noise points receive `cluster_id = -1`.
//!
//! Cluster IDs are *stable across re-runs* because each cluster is re-keyed to
//! the smallest `face_id` found in that cluster after HDBSCAN runs.  Noise (-1)
//! is never re-keyed.
//!
//! # Phase-1b follow-up
//! Replace the stub path in `cluster_faces` with a real HDBSCAN implementation
//! via the `hdbscan` crate (or `linfa-clustering` once it ships HDBSCAN).  The
//! public API surface does not change.

use crate::{AppError, AppResult};
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
/// # Errors
/// Returns [`AppError::InvalidInput`] when any embedding has a length other
/// than [`FACE_EMBED_DIM`].
///
/// # Stub behaviour (phase 1a)
/// Until the real HDBSCAN wiring lands in phase 1b, every face is placed in a
/// cluster whose `cluster_id` equals the **smallest `face_id` belonging to the
/// same `photo_id`**.  This is deterministic and produces realistic output
/// shapes for downstream code, but does not actually group faces by identity.
pub fn cluster_faces(
    inputs: &[FaceInput],
    _params: &ClusterParams,
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

    // Phase-1b: replace block below with real HDBSCAN.
    // todo!("hdbscan via linfa-clustering or hdbscan crate")

    // -----------------------------------------------------------------------
    // Stub path: one cluster per photo_id, cluster_id = smallest face_id in
    // that photo.  Probability is 1.0 for all (no noise in stub mode).
    // -----------------------------------------------------------------------

    // Build photo_id → min face_id map.
    let mut photo_min_face: HashMap<i64, i64> = HashMap::new();
    for fi in inputs {
        let entry = photo_min_face.entry(fi.photo_id).or_insert(fi.face_id);
        if fi.face_id < *entry {
            *entry = fi.face_id;
        }
    }

    let assignments = inputs
        .iter()
        .map(|fi| {
            let cluster_id = photo_min_face
                .get(&fi.photo_id)
                .copied()
                .unwrap_or(fi.face_id);
            ClusterAssignment {
                face_id: fi.face_id,
                cluster_id,
                probability: 1.0,
            }
        })
        .collect();

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
        // Three faces across two photos.  Within each photo the smallest face_id
        // becomes the cluster_id.
        let inputs = vec![
            FaceInput {
                face_id: 20,
                photo_id: 1,
                embedding: make_embedding(0.1),
            },
            FaceInput {
                face_id: 5,
                photo_id: 1,
                embedding: make_embedding(0.2),
            },
            FaceInput {
                face_id: 30,
                photo_id: 2,
                embedding: make_embedding(0.3),
            },
        ];
        let result = cluster_faces(&inputs, &ClusterParams::default()).expect("should succeed");

        assert_eq!(result.len(), 3);
        // photo 1: min face_id = 5 → both faces in cluster 5
        assert_eq!(result[0].cluster_id, 5);
        assert_eq!(result[1].cluster_id, 5);
        // photo 2: only face 30 → cluster 30
        assert_eq!(result[2].cluster_id, 30);
    }
}
