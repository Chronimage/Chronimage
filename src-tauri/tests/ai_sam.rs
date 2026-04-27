//! Integration tests for the SAM2.1 ONNX masking pipeline.
//!
//! Ignored tests require `sam2.1_hiera_large.encoder.onnx` and
//! `sam2.1_hiera_large.decoder.onnx` in `CHRONIMAGE_MODELS_DIR`.

use chronimage::develop::sam::SamSession;
use chronimage::develop::segmentation::FaceHint;
use image::{Rgb, RgbImage};

#[test]
fn sam2_load_reports_missing_components() {
    let missing = std::path::Path::new("__missing_sam2__.onnx");
    let err = SamSession::load(missing, missing).expect_err("missing models must error");
    assert!(
        matches!(err, chronimage::AppError::NotFound(_)),
        "missing SAM2.1 files must surface NotFound, got: {err:?}"
    );
}

fn models_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("CHRONIMAGE_MODELS_DIR") {
        return std::path::PathBuf::from(d);
    }
    dirs::data_local_dir()
        .expect("data_local_dir")
        .join("app.chronimage.desktop")
        .join("models")
}

fn sam2_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = models_dir();
    let enc = dir.join("sam2.1_hiera_large.encoder.onnx");
    let dec = dir.join("sam2.1_hiera_large.decoder.onnx");
    if enc.exists() && dec.exists() {
        Some((enc, dec))
    } else {
        eprintln!("Integration test skipped: SAM2.1 Large models not found at {dir:?}");
        None
    }
}

#[test]
#[ignore]
fn sam2_subject_mask_shape_matches_input() {
    let Some((enc, dec)) = sam2_paths() else {
        return;
    };
    let sess = SamSession::load(&enc, &dec).expect("load SAM2.1 sessions");
    let img = RgbImage::from_pixel(256, 256, Rgb([120, 100, 80]));
    let mask = sess
        .generate_bitmap_mask(&img, "subject", &[])
        .expect("generate SAM2.1 mask");

    assert_eq!(mask.width, 256);
    assert_eq!(mask.height, 256);
    assert!(!mask.data_b64.is_empty(), "mask data must not be empty");
    assert!((0.0..=1.0).contains(&mask.confidence));
    assert_eq!(mask.model, "sam2.1-hiera-large");
}

#[test]
#[ignore]
fn sam2_person_with_face_hint_succeeds() {
    let Some((enc, dec)) = sam2_paths() else {
        return;
    };
    let sess = SamSession::load(&enc, &dec).expect("load SAM2.1 sessions");
    let img = RgbImage::from_pixel(256, 384, Rgb([180, 160, 140]));
    let hints = [FaceHint {
        x: 0.35,
        y: 0.05,
        w: 0.30,
        h: 0.20,
    }];
    let mask = sess
        .generate_bitmap_mask(&img, "person", &hints)
        .expect("generate person mask with face hint");
    assert_eq!(mask.width, 256);
    assert_eq!(mask.height, 384);
}
