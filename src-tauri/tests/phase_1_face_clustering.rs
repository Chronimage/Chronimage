//! Phase 1 exit criterion: face clustering F1 ≥ 0.95 on a labelled fixture.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria.
//!
//! Drives the full stack end-to-end: SCRFD detect → ArcFace embed → HDBSCAN
//! cluster. Requires the real ONNX models (`det_10g.onnx`, `w600k_r50.onnx`)
//! and a labelled fixture at `tests/fixtures/face-clusters/`.
//!
//! Always `#[ignore]` — skips silently when the fixture or models are missing
//! so CI's default lib run stays fast. Nightly CI runs this with `--ignored`.
//!
//! ## Fixture layout expected
//!
//! ```text
//! tests/fixtures/face-clusters/
//!   labels.json      // [{"file": "...", "cluster": 0}, ...]
//!   photos/<photo_id>.jpg
//! ```
//!
//! `labels.json` is a list of `{file, cluster}` objects where `cluster` is the
//! ground-truth cluster ID (0-based, or -1 for noise). F1 is computed per
//! predicted cluster against the largest overlapping ground-truth cluster.

use chronimage::ai::{
    cluster::{cluster_faces, ClusterParams, FaceInput},
    faces::FacesSession,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

const F1_THRESHOLD: f64 = 0.95;

#[derive(Debug, Deserialize)]
struct Label {
    file: String,
    cluster: i64,
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("tests")
        .join("fixtures")
        .join("face-clusters")
}

fn models_present() -> Option<(PathBuf, PathBuf)> {
    let Ok(dir) = chronimage::util::paths::models_dir() else {
        return None;
    };
    let det = dir.join("det_10g.onnx");
    let arc = dir.join("w600k_r50.onnx");
    if det.exists() && arc.exists() {
        Some((det, arc))
    } else {
        None
    }
}

/// Primary-cluster F1. Maps each predicted cluster to its majority-overlap
/// ground-truth cluster and returns precision/recall/F1 for the prediction
/// with the highest face count.
fn primary_cluster_f1(
    predicted: &HashMap<i64, HashSet<i64>>,
    truth: &HashMap<i64, HashSet<i64>>,
) -> f64 {
    let Some((_, primary_pred)) = predicted
        .iter()
        .filter(|(k, _)| **k >= 0) // ignore noise
        .max_by_key(|(_, ids)| ids.len())
    else {
        return 0.0;
    };
    // Find best-matching truth cluster by overlap.
    let best_truth = truth
        .iter()
        .filter(|(k, _)| **k >= 0)
        .map(|(tk, tset)| (tk, primary_pred.intersection(tset).count()))
        .max_by_key(|(_, overlap)| *overlap);
    let Some((best_tk, overlap)) = best_truth else {
        return 0.0;
    };
    let tp = overlap as f64;
    let fp = (primary_pred.len() - overlap) as f64;
    let fn_ = truth.get(best_tk).map(|s| s.len()).unwrap_or(0) as f64 - tp;
    if tp == 0.0 {
        return 0.0;
    }
    let precision = tp / (tp + fp);
    let recall = tp / (tp + fn_);
    2.0 * precision * recall / (precision + recall)
}

use std::path::Path;

#[tokio::test]
#[ignore = "needs real ONNX models + labelled face-cluster fixture"]
async fn face_clustering_f1_ge_0_95() {
    let Some((scrfd, arcface)) = models_present() else {
        eprintln!("skipping: SCRFD or ArcFace model missing from models_dir");
        return;
    };

    let dir = fixture_dir();
    let labels_path = dir.join("labels.json");
    if !labels_path.exists() {
        eprintln!("skipping: {labels_path:?} not found");
        return;
    }
    let labels: Vec<Label> =
        serde_json::from_slice(&std::fs::read(&labels_path).expect("read labels.json"))
            .expect("parse labels.json");

    let session = FacesSession::load(&scrfd, &arcface).expect("load face session");
    let session = std::sync::Arc::new(session);

    // ── Detect + embed every labelled photo, remembering ground-truth cluster.
    let mut inputs: Vec<FaceInput> = Vec::new();
    let mut face_to_truth: HashMap<i64, i64> = HashMap::new();
    let mut next_face_id: i64 = 1;

    for (photo_id, label) in labels.iter().enumerate() {
        let photo_id = photo_id as i64;
        let path = dir.join(&label.file);
        let session_c = std::sync::Arc::clone(&session);
        let path_c = path.clone();
        let faces = tokio::task::spawn_blocking(move || session_c.detect_faces(&path_c))
            .await
            .expect("detect join")
            .expect("detect result");
        for face in faces {
            let session_c = std::sync::Arc::clone(&session);
            let path_c = path.clone();
            let face_c = face.clone();
            let embedding =
                tokio::task::spawn_blocking(move || session_c.embed_face(&path_c, &face_c))
                    .await
                    .expect("embed join")
                    .expect("embed result");
            inputs.push(FaceInput {
                face_id: next_face_id,
                photo_id,
                embedding,
            });
            face_to_truth.insert(next_face_id, label.cluster);
            next_face_id += 1;
        }
    }

    assert!(!inputs.is_empty(), "no faces detected in fixture");

    // ── Cluster.
    let assignments = cluster_faces(&inputs, &ClusterParams::default()).expect("cluster_faces");

    // ── Aggregate into predicted / truth sets of face_ids.
    let mut predicted: HashMap<i64, HashSet<i64>> = HashMap::new();
    for a in &assignments {
        predicted.entry(a.cluster_id).or_default().insert(a.face_id);
    }
    let mut truth: HashMap<i64, HashSet<i64>> = HashMap::new();
    for (face_id, tk) in &face_to_truth {
        truth.entry(*tk).or_default().insert(*face_id);
    }

    let f1 = primary_cluster_f1(&predicted, &truth);
    println!("primary-cluster F1 = {f1:.4} (threshold {F1_THRESHOLD})");
    assert!(
        f1 >= F1_THRESHOLD,
        "primary-cluster F1 {f1:.4} below exit-criterion threshold {F1_THRESHOLD}"
    );
}
