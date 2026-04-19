//! Phase 1 exit criterion: RAW+JPG pair stacking precision ≥ 99.5% on 5k-pair fixture.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria.
//!
//! Marked `#[ignore]` until the 5k-pair fixture set lands in `tests/fixtures/raw-jpg-pairs/`.
//! Run with: `cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_raw_jpg_pair -- --ignored`.

#[test]
#[ignore = "needs 5k-pair fixture set at tests/fixtures/raw-jpg-pairs/"]
fn raw_jpg_pair_precision_ge_99_5_percent() {
    // TODO(cc): load tests/fixtures/raw-jpg-pairs/manifest.json (expected pairs),
    // drive chronimage::import::pair::detect over the fixture directory,
    // compute precision = correct_pairs / predicted_pairs,
    // assert precision >= 0.995.
    unimplemented!("phase-1 exit test — see PRD §Exit criteria");
}
