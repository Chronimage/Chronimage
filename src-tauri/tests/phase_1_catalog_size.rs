//! Phase 1 exit criterion: catalog DB size ≤ 2% of library bytes on a 10k-photo fixture.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria.
//!
//! Marked `#[ignore]` until the 10k-photo fixture is wired.
//! Run with: `cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_catalog_size -- --ignored`.

#[test]
#[ignore = "needs 10k-photo fixture at tests/fixtures/catalog-size/"]
fn catalog_db_size_le_2_percent_of_library_bytes() {
    // TODO(cc): import tests/fixtures/catalog-size/ (10k photos), sum on-disk bytes
    // of the fixture, measure catalog.db + catalog.db-wal size after the import
    // completes (and after one WAL checkpoint), assert ratio <= 0.02.
    unimplemented!("phase-1 exit test — see PRD §Exit criteria");
}
