//! Face-cluster fixture labeling helper.
//!
//! Walks a source directory, runs real SCRFD detection (+ optional ArcFace
//! embedding for downstream cluster-tuning), saves a 128×128 PNG thumbnail
//! per detected face, and writes a `candidates.json` manifest that the
//! companion `tests/fixtures/face-clusters/label.html` page can consume.
//!
//! Typical flow:
//!
//! 1. `chronimage-face-labeler scan <src> --out tests/fixtures/face-clusters`
//! 2. Open `tests/fixtures/face-clusters/label.html` in a browser, type the
//!    cluster id (0, 1, 2, -1 for noise) next to each thumbnail, click Save.
//! 3. The page writes `labels.json` next to the candidates.
//!
//! Designed for the Phase 1 exit-criterion `phase_1_face_clustering.rs` test
//! (F1 ≥ 0.95 on labeled fixture). See docs/prds/phase-1.md § Exit criteria.
//!
//! This binary is dev-only — not shipped in the Tauri bundle.

use chronimage::ai::faces::FacesSession;
use clap::{Parser, Subcommand};
use image::imageops::FilterType;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(
    name = "chronimage-face-labeler",
    version,
    about = "Build a face-cluster fixture from a directory of photos"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Detect faces + write thumbnails + candidates.json.
    Scan {
        /// Directory to walk recursively.
        src: PathBuf,
        /// Output directory (will contain `thumbs/*.png` + `candidates.json`).
        #[arg(long, default_value = "tests/fixtures/face-clusters")]
        out: PathBuf,
        /// Max photos to process (0 = unlimited). Default 200 — sized to keep
        /// the labeling UI tractable.
        #[arg(long, default_value_t = 200)]
        max_photos: usize,
        /// Minimum face area (pixels²) to include. Default 4900 (70×70).
        #[arg(long, default_value_t = 4900)]
        min_face_area: u32,
    },
}

#[derive(Serialize, Debug)]
struct Candidate {
    /// Face id — unique per candidate. Indexes into `thumbs/<id>.png`.
    id: u64,
    /// Relative path to the source photo from the fixture `out` dir.
    photo: String,
    /// Bbox in original-image pixel coordinates (x, y, w, h).
    bbox: [f32; 4],
    /// Detector confidence 0–1.
    score: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Scan {
            src,
            out,
            max_photos,
            min_face_area,
        } => scan_dir(&src, &out, max_photos, min_face_area),
    }
}

fn scan_dir(
    src: &Path,
    out: &Path,
    max_photos: usize,
    min_face_area: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Locate SCRFD + ArcFace models. Bundled dir first, user-data dir second.
    let user_dir = dirs::data_local_dir()
        .ok_or("data_local_dir unavailable")?
        .join(chronimage::APP_ID)
        .join("models");
    let scrfd = user_dir.join("det_10g.onnx");
    let arcface = user_dir.join("w600k_r50.onnx");
    if !scrfd.exists() || !arcface.exists() {
        return Err(format!(
            "face models not found at {user_dir:?} — run `pwsh scripts/fetch-bundled-models.ps1` first"
        )
        .into());
    }
    eprintln!("loading face models from {user_dir:?} (takes ~2s)…");
    let session = FacesSession::load(&scrfd, &arcface)?;

    let thumbs_dir = out.join("thumbs");
    fs::create_dir_all(&thumbs_dir)?;

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut next_id: u64 = 1;
    let mut photos_seen: usize = 0;

    for entry in walkdir(src) {
        let photo_path = entry;
        let ext = photo_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
            continue;
        }
        if max_photos > 0 && photos_seen >= max_photos {
            break;
        }
        photos_seen += 1;
        eprintln!("[{photos_seen}] {photo_path:?}");

        let boxes = match session.detect_faces(&photo_path, None) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  detect error: {e} — skipping");
                continue;
            }
        };
        eprintln!("  {} face(s)", boxes.len());

        let img = match chronimage::ai::image_util::open_any(&photo_path) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("  image decode error: {e} — skipping");
                continue;
            }
        };
        let (w, h) = (img.width() as f32, img.height() as f32);

        for face in boxes {
            let area = (face.w * face.h) as u32;
            if area < min_face_area {
                continue;
            }
            let x0 = face.x.clamp(0.0, w - 1.0) as u32;
            let y0 = face.y.clamp(0.0, h - 1.0) as u32;
            let x1 = (face.x + face.w).clamp(0.0, w) as u32;
            let y1 = (face.y + face.h).clamp(0.0, h) as u32;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let crop = img.crop_imm(x0, y0, x1 - x0, y1 - y0);
            let thumb = crop.resize_exact(128, 128, FilterType::Triangle);
            let id = next_id;
            next_id += 1;
            let thumb_path = thumbs_dir.join(format!("{id}.png"));
            thumb.save_with_format(&thumb_path, image::ImageFormat::Png)?;

            let rel = photo_path
                .strip_prefix(src)
                .unwrap_or(&photo_path)
                .to_string_lossy()
                .into_owned();
            candidates.push(Candidate {
                id,
                photo: rel,
                bbox: [face.x, face.y, face.w, face.h],
                score: face.score,
            });
        }
    }

    // Write candidates.json next to the thumbs dir.
    let manifest_path = out.join("candidates.json");
    let body = serde_json::json!({
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "source": src.display().to_string(),
        "photos_seen": photos_seen,
        "candidates": candidates,
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&body)?)?;

    eprintln!(
        "\n{} photos walked, {} face candidates saved",
        photos_seen,
        candidates.len()
    );
    eprintln!("manifest: {manifest_path:?}");
    eprintln!("open: tests/fixtures/face-clusters/label.html");
    Ok(())
}

/// Minimal recursive walk — we avoid pulling `walkdir` to keep dev-binary
/// compile cost low (chronimage-face-labeler only runs from dev machines).
fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}
