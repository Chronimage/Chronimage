//! Phase 1 exit criterion: face clustering F1 ≥ 0.95 on labeled fixture.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria.
//!
//! Marked `#[ignore]` until ArcFace+HDBSCAN inference lands (currently stubbed —
//! see TODO(cc) in src-tauri/src/ai/faces.rs and ai/cluster.rs) AND the labeled
//! fixture set exists at `tests/fixtures/face-clusters/`.
//! Run with: `cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_face_clustering -- --ignored`.

#[test]
#[ignore = "needs real ArcFace inference + labeled fixture set"]
fn face_clustering_f1_ge_0_95() {
    // TODO(cc): load tests/fixtures/face-clusters/labels.json (photo_id -> ground-truth cluster),
    // run the full import pipeline through RetinaFace + ArcFace + HDBSCAN,
    // compute precision/recall/F1 per predicted cluster against ground truth,
    // assert F1 >= 0.95 for the primary (largest) cluster.
    unimplemented!("phase-1 exit test — see PRD §Exit criteria");
}
