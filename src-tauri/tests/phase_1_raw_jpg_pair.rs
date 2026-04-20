//! Phase 1 exit criterion: RAW+JPG pair stacking precision ≥ 99.5% on
//! a 5k-pair fixture.
//!
//! PRD reference: `docs/prds/phase-1.md` § Exit criteria.
//!
//! ## Fixture layout
//!
//! The fixture lives outside the repo by default — RAW files are ~25 MB and
//! JPGs are ~8 MB each, so 5 000 pairs is ~150–250 GB. Generate it with
//! `scripts/scan-raw-jpg-pairs.ps1` and point the test at it via:
//!
//! ```ignore
//! $env:CHRONIMAGE_RAW_JPG_FIXTURE = 'C:\Users\jayas\OneDrive\Pictures\raw pairs'
//! cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_raw_jpg_pair -- --ignored
//! ```
//!
//! The fixture dir must contain:
//!   - `manifest.json` — ground-truth pair list produced by the scanner script
//!   - the RAW + JPG files themselves, named with the stems in the manifest
//!
//! ## What this measures
//!
//! - **Precision** = `|predicted ∩ truth| / |predicted|` — when our pair
//!   detector calls two files a pair, how often is it correct against the
//!   manifest's ground truth? PRD target: ≥ 0.995.
//! - **Recall** = `|predicted ∩ truth| / |truth|` — reported as a sanity
//!   number; not a PRD gate. A precision-only metric keeps us honest when
//!   the detector leans conservative (skipping ambiguous multi-raw groups).

use chronimage::import::{detect_pairs, scan_dir, ScanOptions};
use serde::Deserialize;
use std::{collections::HashSet, path::PathBuf};

const PRECISION_THRESHOLD: f64 = 0.995;

#[derive(Debug, Deserialize)]
struct Manifest {
    pairs: Vec<ManifestPair>,
}

#[derive(Debug, Deserialize)]
struct ManifestPair {
    raw: String,
    jpg: String,
}

/// Fixture resolution order:
///  1. `$CHRONIMAGE_RAW_JPG_FIXTURE` (explicit override for ad-hoc runs)
///  2. `<repo>/tests/fixtures/raw-jpg-pairs/` (the canonical local-disk path;
///     gitignored — regenerate with `scripts/scan-raw-jpg-pairs.ps1`)
///  3. `None` (test skips with a friendly message)
fn fixture_dir() -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("CHRONIMAGE_RAW_JPG_FIXTURE") {
        return Some(PathBuf::from(env));
    }
    // CARGO_MANIFEST_DIR points at `src-tauri/` — fixtures live under repo root.
    let repo_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("tests").join("fixtures").join("raw-jpg-pairs"))?;
    if repo_fixture.join("manifest.json").exists() {
        return Some(repo_fixture);
    }
    None
}

/// Build the canonical unordered-pair key `(lower_raw, lower_jpg)` so set
/// membership is stable regardless of path case or whether the detector
/// surfaced `raw`-first or `jpg`-first.
fn pair_key(raw: &str, jpg: &str) -> (String, String) {
    let r = raw.to_ascii_lowercase();
    let j = jpg.to_ascii_lowercase();
    if r < j {
        (r, j)
    } else {
        (j, r)
    }
}

#[test]
#[ignore = "needs fixture at tests/fixtures/raw-jpg-pairs/ or $CHRONIMAGE_RAW_JPG_FIXTURE"]
fn raw_jpg_pair_precision_ge_99_5_percent() {
    let Some(root) = fixture_dir() else {
        eprintln!(
            "skipping: no fixture found at tests/fixtures/raw-jpg-pairs/ and \
             $CHRONIMAGE_RAW_JPG_FIXTURE not set — regenerate via \
             scripts/scan-raw-jpg-pairs.ps1"
        );
        return;
    };
    if !root.exists() {
        panic!("fixture dir does not exist: {root:?}");
    }

    // 1. Load ground-truth manifest. PowerShell writes UTF-8 with a BOM
    // by default; strip it before handing to serde_json.
    let manifest_path = root.join("manifest.json");
    let raw_bytes =
        std::fs::read(&manifest_path).unwrap_or_else(|e| panic!("read {manifest_path:?}: {e}"));
    let manifest_bytes: &[u8] = raw_bytes
        .strip_prefix(&[0xEF_u8, 0xBB, 0xBF])
        .unwrap_or(&raw_bytes);
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)
        .unwrap_or_else(|e| panic!("parse {manifest_path:?}: {e}"));
    assert!(
        !manifest.pairs.is_empty(),
        "manifest has zero pairs — regenerate with scripts/scan-raw-jpg-pairs.ps1"
    );

    let truth: HashSet<(String, String)> = manifest
        .pairs
        .iter()
        .map(|p| pair_key(&p.raw, &p.jpg))
        .collect();

    // 2. Run the real detector against every image in the fixture.
    let entries = scan_dir(&ScanOptions::new(&root)).expect("scan_dir");
    let all_paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
    let (predicted_pairs, _leftovers) = detect_pairs(all_paths);

    assert!(
        !predicted_pairs.is_empty(),
        "detect_pairs returned zero — fixture may be malformed"
    );

    // 3. Convert predictions to the same case-insensitive unordered-pair keys.
    let predicted: HashSet<(String, String)> = predicted_pairs
        .iter()
        .map(|p| {
            let r = p
                .raw
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let j = p
                .jpg
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            pair_key(r, j)
        })
        .collect();

    let correct = predicted.intersection(&truth).count();
    let predicted_count = predicted.len();
    let truth_count = truth.len();

    let precision = correct as f64 / predicted_count as f64;
    let recall = correct as f64 / truth_count as f64;

    println!(
        "pair detector: predicted={predicted_count}, truth={truth_count}, \
         correct={correct}, precision={precision:.5}, recall={recall:.5}"
    );

    assert!(
        precision >= PRECISION_THRESHOLD,
        "pair precision {precision:.5} below threshold {PRECISION_THRESHOLD:.3} \
         (predicted={predicted_count}, correct={correct}) — fixture size {truth_count}"
    );
}
