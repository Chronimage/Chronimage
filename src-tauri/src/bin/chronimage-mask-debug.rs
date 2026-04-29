//! Standalone subject-mask debugging harness.
//!
//! The full Tauri app routes `develop_mask_generate` → `SamSession::generate_bitmap_mask`,
//! which builds a fixed prompt set inside `develop::sam::build_prompts`. When that
//! prompt set lands on the wrong region (e.g. a center-positive falling on a
//! decorative element instead of the subject) the user just sees a bad mask with
//! no way to inspect why. This bin reproduces the same encoder/decoder pipeline
//! against a single image, runs every named prompt strategy in turn, and writes
//! `<strategy>.alpha.png` + `<strategy>.overlay.png` so we can see what SAM saw.
//!
//! Encoding is amortised: SAM2-large's image encoder is the slow part (~10–30 s
//! on CPU, faster on DirectML); decoding with a new prompt set is sub-second.
//! So the loop is "encode once, decode many".
//!
//! Usage:
//!   chronimage-mask-debug --image <path> [--out <dir>] [--strategy <name>]
//!
//! With no `--strategy`, every strategy in `STRATEGIES` runs. Add more entries
//! to that array to test new prompt layouts without touching `develop::sam`.

use chronimage::ai::faces::{FaceBox, FacesSession};
use chronimage::develop::sam::{NormalizedPrompt, SamSession};
use chronimage::develop::segmentation::FaceHint;
use clap::Parser;
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "chronimage-mask-debug",
    about = "Iterate SAM2.1 subject-mask prompt strategies on a single image"
)]
struct Cli {
    /// Path to the test image (JPEG / PNG / HEIC handled by the `image` crate).
    #[arg(long)]
    image: PathBuf,
    /// Output directory for alpha + overlay PNGs.
    #[arg(long, default_value = "mask-debug-out")]
    out: PathBuf,
    /// Run only one named strategy (default: all).
    #[arg(long)]
    strategy: Option<String>,
    /// Override the bundled-models directory.
    #[arg(long)]
    models: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.out)?;

    let models_dir = cli.models.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("models")
            .join("bundled")
    });
    eprintln!("models dir: {}", models_dir.display());

    let img_dyn = image::open(&cli.image)?;
    let img = img_dyn.to_rgb8();
    let (w, h) = img.dimensions();
    eprintln!("image: {}×{}", w, h);

    // ── Face detection (SCRFD-10g) ───────────────────────────────────
    let scrfd = models_dir.join("det_10g.onnx");
    let arcface = models_dir.join("w600k_r50.onnx");
    let faces = if scrfd.exists() && arcface.exists() {
        FacesSession::load(&scrfd, &arcface).ok()
    } else {
        eprintln!(
            "face models not found at {} — running without face hints",
            scrfd.display()
        );
        None
    };

    let face_boxes: Vec<FaceBox> = if let Some(s) = &faces {
        let t = Instant::now();
        let detected = s.detect_faces(&cli.image, None).unwrap_or_default();
        eprintln!("scrfd: {} face(s) in {:?}", detected.len(), t.elapsed());
        for (i, fb) in detected.iter().enumerate() {
            eprintln!(
                "  face[{i}]: x={:.1} y={:.1} w={:.1} h={:.1} score={:.3}",
                fb.x, fb.y, fb.w, fb.h, fb.score
            );
        }
        detected
    } else {
        Vec::new()
    };
    let face_hints: Vec<FaceHint> = face_boxes
        .iter()
        .map(|fb| FaceHint {
            x: fb.x / w as f32,
            y: fb.y / h as f32,
            w: fb.w / w as f32,
            h: fb.h / h as f32,
        })
        .collect();
    let primary_face = face_hints.iter().copied().max_by(|a, b| {
        (a.w * a.h)
            .partial_cmp(&(b.w * b.h))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ── SAM2.1 ────────────────────────────────────────────────────────
    let sam_enc = models_dir.join("sam2.1_hiera_large.encoder.onnx");
    let sam_dec = models_dir.join("sam2.1_hiera_large.decoder.onnx");
    eprintln!("loading SAM2.1 sessions...");
    let sam = SamSession::load(&sam_enc, &sam_dec)?;

    eprintln!("encoding image...");
    let t = Instant::now();
    let feats = sam.encode_features(&img)?;
    eprintln!("encoded in {:?}", t.elapsed());

    // ── Strategies ────────────────────────────────────────────────────
    let strategies = build_strategies(primary_face, &face_hints);
    let mut ran = 0;
    for (name, prompts) in &strategies {
        if let Some(only) = &cli.strategy {
            if name != only {
                continue;
            }
        }
        eprintln!("\n── strategy: {name} (n={}) ──", prompts.len());
        for (xn, yn, lab) in prompts {
            eprintln!(
                "  {} ({:.3}, {:.3})",
                if *lab > 0.5 { "+" } else { "-" },
                xn,
                yn
            );
        }
        let t = Instant::now();
        let mask = sam.decode_normalized(&feats, prompts, &img, false)?;
        eprintln!(
            "  decoded in {:?}, confidence={:.3}",
            t.elapsed(),
            mask.confidence
        );
        let alpha = decode_mask_alpha(&mask.data_b64, w, h)?;
        let coverage = alpha.iter().filter(|&&a| a > 64).count() as f64 / alpha.len() as f64;
        eprintln!("  coverage (alpha>64) = {:.1}%", coverage * 100.0);

        let alpha_path = cli.out.join(format!("{name}.alpha.png"));
        save_alpha_png(&alpha_path, w, h, &alpha)?;
        let overlay_path = cli.out.join(format!("{name}.overlay.png"));
        save_overlay_png(&overlay_path, &img, &alpha, prompts, &face_boxes)?;
        ran += 1;
    }

    if ran == 0 {
        eprintln!("no strategy matched --strategy filter");
    } else {
        eprintln!("\nwrote {ran} strategy output(s) to {}", cli.out.display());
    }
    Ok(())
}

fn build_strategies(
    primary_face: Option<FaceHint>,
    all_faces: &[FaceHint],
) -> Vec<(String, Vec<NormalizedPrompt>)> {
    let mut out: Vec<(String, Vec<NormalizedPrompt>)> = Vec::new();

    // S1 — current production "subject" path with no face hint: single
    // center positive + 8 dense border negatives. Reproduces what users
    // see when no face is detected.
    out.push((
        "01_center_only_no_face".into(),
        with_dense_border_negatives(vec![(0.5, 0.5, 1.0)]),
    ));

    // S2 — current production "subject" path WITH face hint: face center +
    // torso anchor + dense border negatives. The strategy that should
    // already be deployed in `build_prompts`.
    if let Some(face) = primary_face {
        let cx = face.x + face.w * 0.5;
        let cy = face.y + face.h * 0.5;
        let torso_y = (face.y + face.h * 3.5).clamp(0.0, 1.0);
        out.push((
            "02_face_center_plus_torso".into(),
            with_dense_border_negatives(vec![(cx, cy, 1.0), (cx, torso_y, 1.0)]),
        ));

        // S3 — face only (no torso assumption). For seated/lying poses
        // the torso anchor at face_y + 3.5*face_h leaves the image, then
        // gets clamped to y=1.0, which lands on the background border.
        out.push((
            "03_face_only".into(),
            with_dense_border_negatives(vec![(cx, cy, 1.0)]),
        ));

        // S4 — face + body bbox guess. Use a softer body extension that
        // stays near the face when the photo is tight, expands when the
        // face is small. Cap at 0.85 so we never push positives onto the
        // bottom border of the frame.
        let body_y = (face.y + face.h * 2.5).clamp(0.0, 0.85);
        out.push((
            "04_face_plus_short_torso".into(),
            with_dense_border_negatives(vec![(cx, cy, 1.0), (cx, body_y, 1.0)]),
        ));

        // S5 — face center + an explicit negative on the geometric image
        // center. The bug we are chasing: when the subject is off-center,
        // a corner/edge negative set isn't enough to prevent SAM from
        // bleeding the mask through the geometric center where another
        // visually salient region (rangoli, art) is sitting.
        let mut prompts = with_dense_border_negatives(vec![(cx, cy, 1.0)]);
        prompts.push((0.5, 0.5, 0.0));
        out.push(("05_face_with_center_negative".into(), prompts));

        // S6 — same as S5 but also negative on the *opposite* half of the
        // image at the same vertical band as the face. If the face is on
        // the right (cx > 0.5), drop a strong negative at (1 - cx, cy).
        let mut prompts = with_dense_border_negatives(vec![(cx, cy, 1.0)]);
        let mirror_x = (1.0 - cx).clamp(0.05, 0.95);
        prompts.push((mirror_x, cy, 0.0));
        prompts.push((0.5, 0.5, 0.0));
        out.push(("06_face_with_mirror_negative".into(), prompts));

        // S7 — bounding-box prompts: pad the face box outward by 1.5×
        // (capturing torso/shoulders) and use the rectangle as 2 positive
        // points at the corners (SAM2 treats two diagonal positives as
        // a soft bbox). Plus dense negatives.
        let pad_x = face.w * 1.5;
        let pad_y_top = face.h * 0.6;
        let pad_y_bot = face.h * 5.0;
        let x0 = (face.x - pad_x).clamp(0.02, 0.98);
        let y0 = (face.y - pad_y_top).clamp(0.02, 0.98);
        let x1 = (face.x + face.w + pad_x).clamp(0.02, 0.98);
        let y1 = (face.y + face.h + pad_y_bot).clamp(0.02, 0.98);
        out.push((
            "07_bbox_corners".into(),
            with_dense_border_negatives(vec![(cx, cy, 1.0), (x0, y0, 1.0), (x1, y1, 1.0)]),
        ));

        // S8 — dense face cluster + body strip: 5 positives sampled near
        // the face landmarks region, plus a vertical strip of body
        // positives. Gives SAM more anchor points to lock onto.
        let mut prompts: Vec<NormalizedPrompt> = vec![
            (cx, cy, 1.0),
            (cx - face.w * 0.25, cy, 1.0),
            (cx + face.w * 0.25, cy, 1.0),
            (cx, cy - face.h * 0.25, 1.0),
            (cx, cy + face.h * 0.25, 1.0),
        ];
        for k in 1..=4 {
            let yk = (face.y + face.h * (1.5 + 0.6 * k as f32)).clamp(0.0, 0.85);
            prompts.push((cx, yk, 1.0));
        }
        out.push((
            name_for("08_dense_cluster"),
            with_dense_border_negatives(prompts),
        ));

        // S10 — box prompt + face anchor + dense negatives. Mirrors the
        // production `build_prompts("subject", ...)` path so the bin
        // exercises the same prompt set the Tauri app produces. SAM2 was
        // trained extensively on box-prompted ground truth, so this
        // typically beats every point-only strategy on object masks.
        // Labels: 2.0 = box top-left, 3.0 = box bottom-right.
        let bx0 = (face.x - face.w * 1.4).clamp(0.005, 0.995);
        let by0 = (face.y - face.h * 0.5).clamp(0.005, 0.995);
        let bx1 = (face.x + face.w + face.w * 1.4).clamp(0.005, 0.995);
        let by1 = (face.y + face.h * 5.5).clamp(0.005, 0.995);
        let mut box_prompts: Vec<NormalizedPrompt> =
            vec![(bx0, by0, 2.0), (bx1, by1, 3.0), (cx, cy, 1.0)];
        box_prompts = with_dense_border_negatives(box_prompts);
        out.push(("10_box_plus_face_anchor".into(), box_prompts));
    }

    // S9 — multi-face: every face contributes a positive; everything else
    // is a border negative.
    if all_faces.len() > 1 {
        let mut prompts: Vec<NormalizedPrompt> = Vec::new();
        for f in all_faces {
            let cx = f.x + f.w * 0.5;
            let cy = f.y + f.h * 0.5;
            prompts.push((cx, cy, 1.0));
        }
        out.push(("09_all_faces".into(), with_dense_border_negatives(prompts)));
    }

    out
}

fn name_for(s: &str) -> String {
    s.to_string()
}

fn with_dense_border_negatives(mut prompts: Vec<NormalizedPrompt>) -> Vec<NormalizedPrompt> {
    for &(nx, ny) in &[
        (0.02, 0.02),
        (0.98, 0.02),
        (0.02, 0.98),
        (0.98, 0.98),
        (0.5, 0.02),
        (0.5, 0.98),
        (0.02, 0.5),
        (0.98, 0.5),
    ] {
        prompts.push((nx, ny, 0.0));
    }
    prompts
}

fn decode_mask_alpha(
    data_b64: &str,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(data_b64)?;
    let dyn_img = image::load_from_memory(&bytes)?;
    let (dw, dh) = (dyn_img.width(), dyn_img.height());
    if (dw, dh) != (w, h) {
        return Err(format!("mask dims ({dw},{dh}) != image dims ({w},{h})").into());
    }
    // SamSession writes the matte into the alpha channel of a LumaA8
    // PNG (luma is constant 255). Reading luminance instead would give
    // a flat 255 buffer and report 100% coverage on every photo.
    let raw: Vec<u8> = if dyn_img.color().has_alpha() {
        dyn_img.to_rgba8().pixels().map(|p| p.0[3]).collect()
    } else {
        dyn_img.to_luma8().into_raw()
    };
    Ok(raw)
}

fn save_alpha_png(
    path: &Path,
    w: u32,
    h: u32,
    alpha: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let img =
        image::GrayImage::from_raw(w, h, alpha.to_vec()).ok_or("alpha buffer wrong length")?;
    img.save(path)?;
    Ok(())
}

fn save_overlay_png(
    path: &Path,
    img: &RgbImage,
    alpha: &[u8],
    prompts: &[NormalizedPrompt],
    faces: &[FaceBox],
) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    // Tint mask region red, fade the rest to grey to make coverage obvious.
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + x as usize;
            let a = alpha[i] as f32 / 255.0;
            let p = out.get_pixel_mut(x, y);
            let r = p.0[0] as f32;
            let g = p.0[1] as f32;
            let b = p.0[2] as f32;
            let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 255.0);
            // Outside mask → 60% grey. Inside mask → red-tinted original.
            let outside_r = luma * 0.6;
            let outside_g = luma * 0.6;
            let outside_b = luma * 0.6;
            let inside_r = (r * 0.6 + 255.0 * 0.4).clamp(0.0, 255.0);
            let inside_g = g * 0.6;
            let inside_b = b * 0.6;
            p.0[0] = (outside_r * (1.0 - a) + inside_r * a).round() as u8;
            p.0[1] = (outside_g * (1.0 - a) + inside_g * a).round() as u8;
            p.0[2] = (outside_b * (1.0 - a) + inside_b * a).round() as u8;
        }
    }
    // Draw face boxes (cyan) so we can see where SCRFD found them.
    for fb in faces {
        let x0 = fb.x.max(0.0) as i32;
        let y0 = fb.y.max(0.0) as i32;
        let x1 = (fb.x + fb.w).min(w as f32 - 1.0) as i32;
        let y1 = (fb.y + fb.h).min(h as f32 - 1.0) as i32;
        draw_rect(&mut out, x0, y0, x1, y1, Rgb([0, 255, 255]));
    }
    // Draw prompt markers: positives green, negatives magenta.
    for (xn, yn, lab) in prompts {
        let cx = (xn * (w as f32 - 1.0)).round() as i32;
        let cy = (yn * (h as f32 - 1.0)).round() as i32;
        let color = if *lab > 0.5 {
            Rgb([0, 255, 0])
        } else {
            Rgb([255, 0, 255])
        };
        draw_marker(&mut out, cx, cy, 6, color);
    }
    out.save(path)?;
    Ok(())
}

fn draw_rect(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    let (w, h) = img.dimensions();
    let xs = [x0, x1];
    let ys = [y0, y1];
    for &x in &xs {
        if x < 0 || x >= w as i32 {
            continue;
        }
        for y in y0..=y1 {
            if y >= 0 && y < h as i32 {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
    for &y in &ys {
        if y < 0 || y >= h as i32 {
            continue;
        }
        for x in x0..=x1 {
            if x >= 0 && x < w as i32 {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn draw_marker(img: &mut RgbImage, cx: i32, cy: i32, size: i32, color: Rgb<u8>) {
    let (w, h) = img.dimensions();
    for dy in -size..=size {
        for dx in -size..=size {
            if dx.abs() + dy.abs() > size {
                continue;
            }
            let x = cx + dx;
            let y = cy + dy;
            if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
    // Outline ring for visibility against any background.
    for d in [-size - 1, size + 1] {
        for k in -size..=size {
            for (x, y) in [(cx + d, cy + k), (cx + k, cy + d)] {
                if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                    img.put_pixel(x as u32, y as u32, Rgb([0, 0, 0]));
                }
            }
        }
    }
}
