//! Phase 1 exit criterion: 8-hour import + search + face-cluster loop, zero panics.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria (nightly only).
//!
//! Always `#[ignore]`; CI runs this in the nightly workflow with `--ignored`.

#[test]
#[ignore = "8-hour stress loop — nightly only"]
fn eight_hour_loop_no_panics() {
    // TODO(cc): spawn the full import pipeline over a seeded 50k-photo fixture,
    // interleave search queries + face cluster rebuilds for 8 hours,
    // assert no panic hook fired, no unclosed SQLite transactions, RSS under 2 GB peak.
    unimplemented!("phase-1 nightly stress test — see PRD §Exit criteria");
}
