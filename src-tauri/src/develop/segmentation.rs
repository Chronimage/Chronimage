//! Local bitmap mask generation for Develop.
//!
//! This is the non-placeholder path used by the Mask tab: it produces a real
//! per-pixel alpha matte that is persisted in `develop_masks.mask_payload`.
//! The SAM3 ONNX runner writes the same payload contract; this module remains
//! as a no-network fallback when the bundled runtime is unavailable.

use crate::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{codecs::png::PngEncoder, ImageEncoder, RgbImage};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct FaceHint {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedMask {
    pub width: u32,
    pub height: u32,
    pub data_b64: String,
    pub confidence: f64,
    pub model: &'static str,
}

pub fn generate_bitmap_mask(
    img: &RgbImage,
    source: &str,
    face_hints: &[FaceHint],
) -> AppResult<GeneratedMask> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(AppError::InvalidInput(
            "cannot generate mask for empty image".into(),
        ));
    }

    let bg = estimate_border_rgb(img);
    let mut alpha = vec![0_u8; (w as usize).saturating_mul(h as usize)];
    match source {
        "person" => generate_person_alpha(img, bg, face_hints, &mut alpha),
        "subject" | "object" => generate_subject_alpha(img, bg, face_hints, &mut alpha),
        "sky" => generate_sky_alpha(img, &mut alpha),
        "foreground" => generate_foreground_alpha(w, h, &mut alpha),
        "background" | "landscape" => {
            generate_subject_alpha(img, bg, face_hints, &mut alpha);
            for a in &mut alpha {
                *a = 255_u8.saturating_sub(*a);
            }
        }
        other => {
            return Err(AppError::InvalidInput(format!(
                "unsupported generated mask source {other}"
            )));
        }
    }

    blur_alpha(&mut alpha, w as usize, h as usize, 2);
    let confidence = mask_confidence(&alpha);
    let data_b64 = encode_luma_png(w, h, &alpha)?;
    Ok(GeneratedMask {
        width: w,
        height: h,
        data_b64,
        confidence,
        model: "local-segmentation-v1",
    })
}

fn generate_person_alpha(img: &RgbImage, bg: [f32; 3], face_hints: &[FaceHint], alpha: &mut [u8]) {
    let (w, h) = img.dimensions();
    for y in 0..h {
        let yn = normalized(y, h);
        for x in 0..w {
            let xn = normalized(x, w);
            let pixel = img.get_pixel(x, y).0;
            let distance = color_distance(pixel, bg);
            let center = ellipse_score(xn, yn, 0.5, 0.58, 0.33, 0.48, 0.35);
            let face_body = face_body_score(xn, yn, face_hints);
            let skin = skin_score(pixel);
            let score =
                (distance * 0.72 + center * 0.42 + face_body * 0.85 + skin * 0.28).clamp(0.0, 1.0);
            alpha[(y as usize * w as usize) + x as usize] = to_alpha(score, 0.34, 0.72);
        }
    }
}

fn generate_subject_alpha(img: &RgbImage, bg: [f32; 3], face_hints: &[FaceHint], alpha: &mut [u8]) {
    let (w, h) = img.dimensions();
    for y in 0..h {
        let yn = normalized(y, h);
        for x in 0..w {
            let xn = normalized(x, w);
            let pixel = img.get_pixel(x, y).0;
            let distance = color_distance(pixel, bg);
            let center = ellipse_score(xn, yn, 0.5, 0.54, 0.38, 0.42, 0.38);
            let face_body = face_body_score(xn, yn, face_hints);
            let score = (distance * 0.78 + center * 0.52 + face_body * 0.55).clamp(0.0, 1.0);
            alpha[(y as usize * w as usize) + x as usize] = to_alpha(score, 0.32, 0.70);
        }
    }
}

fn generate_sky_alpha(img: &RgbImage, alpha: &mut [u8]) {
    let (w, h) = img.dimensions();
    for y in 0..h {
        let yn = normalized(y, h);
        let top_prior = 1.0 - smoothstep(0.10, 0.62, yn);
        for x in 0..w {
            let pixel = img.get_pixel(x, y).0;
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            let blue = ((b - r).max(0.0) * 1.4 + (b - g).max(0.0)).clamp(0.0, 1.0);
            let bright = luma(pixel);
            let low_texture = 1.0 - local_contrast(img, x, y).min(1.0);
            let score = (top_prior * 0.62 + blue * 0.42 + bright * 0.16 + low_texture * 0.18)
                .clamp(0.0, 1.0);
            alpha[(y as usize * w as usize) + x as usize] = to_alpha(score, 0.46, 0.78);
        }
    }
}

fn generate_foreground_alpha(w: u32, h: u32, alpha: &mut [u8]) {
    for y in 0..h {
        let yn = normalized(y, h);
        let a = smoothstep(0.35, 0.92, yn);
        for x in 0..w {
            alpha[(y as usize * w as usize) + x as usize] = (a * 255.0).round() as u8;
        }
    }
}

fn estimate_border_rgb(img: &RgbImage) -> [f32; 3] {
    let (w, h) = img.dimensions();
    let border = ((w.min(h) as f32) * 0.06).round().clamp(1.0, 24.0) as u32;
    let mut acc = [0.0_f32; 3];
    let mut n = 0.0_f32;
    for y in 0..h {
        for x in 0..w {
            if x < border
                || y < border
                || x >= w.saturating_sub(border)
                || y >= h.saturating_sub(border)
            {
                let p = img.get_pixel(x, y).0;
                acc[0] += p[0] as f32 / 255.0;
                acc[1] += p[1] as f32 / 255.0;
                acc[2] += p[2] as f32 / 255.0;
                n += 1.0;
            }
        }
    }
    if n <= 0.0 {
        return [0.5, 0.5, 0.5];
    }
    [acc[0] / n, acc[1] / n, acc[2] / n]
}

fn face_body_score(x: f32, y: f32, faces: &[FaceHint]) -> f32 {
    faces
        .iter()
        .map(|face| {
            let cx = face.x + face.w * 0.5;
            let body_top = (face.y - face.h * 0.45).clamp(0.0, 1.0);
            let body_bottom = (face.y + face.h * 7.4).clamp(body_top + 0.05, 1.0);
            let cy = (body_top + body_bottom) * 0.5;
            let rx = (face.w * 3.8).clamp(0.12, 0.38);
            let ry = ((body_bottom - body_top) * 0.55).clamp(0.18, 0.58);
            ellipse_score(x, y, cx, cy, rx, ry, 0.25)
        })
        .fold(0.0_f32, f32::max)
}

fn ellipse_score(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32, feather: f32) -> f32 {
    let d = (((x - cx) / rx.max(1e-4)).powi(2) + ((y - cy) / ry.max(1e-4)).powi(2)).sqrt();
    1.0 - smoothstep(1.0 - feather.clamp(0.0, 0.95), 1.0, d)
}

fn color_distance(pixel: [u8; 3], bg: [f32; 3]) -> f32 {
    let r = pixel[0] as f32 / 255.0;
    let g = pixel[1] as f32 / 255.0;
    let b = pixel[2] as f32 / 255.0;
    let dist = ((r - bg[0]).powi(2) + (g - bg[1]).powi(2) + (b - bg[2]).powi(2)).sqrt();
    (dist * 1.45).clamp(0.0, 1.0)
}

fn skin_score(pixel: [u8; 3]) -> f32 {
    let r = pixel[0] as f32 / 255.0;
    let g = pixel[1] as f32 / 255.0;
    let b = pixel[2] as f32 / 255.0;
    let warm = (r - b).max(0.0) + (r - g).max(0.0) * 0.35;
    let brightness = smoothstep(0.18, 0.82, luma(pixel));
    (warm * 2.2 * brightness).clamp(0.0, 1.0)
}

fn luma(pixel: [u8; 3]) -> f32 {
    (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32) / 255.0
}

fn local_contrast(img: &RgbImage, x: u32, y: u32) -> f32 {
    let (w, h) = img.dimensions();
    let here = luma(img.get_pixel(x, y).0);
    let right = luma(img.get_pixel((x + 2).min(w.saturating_sub(1)), y).0);
    let down = luma(img.get_pixel(x, (y + 2).min(h.saturating_sub(1))).0);
    ((here - right).abs() + (here - down).abs()) * 4.0
}

fn to_alpha(score: f32, lo: f32, hi: f32) -> u8 {
    (smoothstep(lo, hi, score) * 255.0).round() as u8
}

fn normalized(v: u32, max: u32) -> f32 {
    if max <= 1 {
        0.0
    } else {
        v as f32 / (max - 1) as f32
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn blur_alpha(alpha: &mut [u8], w: usize, h: usize, passes: usize) {
    if w == 0 || h == 0 {
        return;
    }
    let mut tmp = alpha.to_vec();
    for _ in 0..passes {
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0_u32;
                let mut n = 0_u32;
                for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        sum += alpha[yy * w + xx] as u32;
                        n += 1;
                    }
                }
                tmp[y * w + x] = (sum / n.max(1)) as u8;
            }
        }
        alpha.copy_from_slice(&tmp);
    }
}

fn mask_confidence(alpha: &[u8]) -> f64 {
    if alpha.is_empty() {
        return 0.0;
    }
    let covered = alpha.iter().filter(|&&a| a > 32).count() as f64 / alpha.len() as f64;
    (0.35 + covered.clamp(0.0, 0.55)).clamp(0.0, 0.92)
}

pub(crate) fn encode_luma_png(width: u32, height: u32, alpha: &[u8]) -> AppResult<String> {
    let expected = (width as usize).saturating_mul(height as usize);
    if alpha.len() != expected {
        return Err(AppError::Internal(
            "mask alpha buffer has wrong size".into(),
        ));
    }
    let mut out = Vec::new();
    let encoder = PngEncoder::new(&mut out);
    encoder
        .write_image(alpha, width, height, image::ExtendedColorType::L8)
        .map_err(|e| AppError::Internal(format!("encode mask png: {e}")))?;
    Ok(B64.encode(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn person_mask_generates_bitmap_alpha() {
        let mut img = RgbImage::from_pixel(80, 100, Rgb([80, 80, 80]));
        for y in 20..88 {
            for x in 28..52 {
                img.put_pixel(x, y, Rgb([40, 34, 36]));
            }
        }
        for y in 18..32 {
            for x in 34..47 {
                img.put_pixel(x, y, Rgb([178, 123, 94]));
            }
        }

        let mask = generate_bitmap_mask(&img, "person", &[]).expect("mask generated");
        assert_eq!(mask.width, 80);
        assert_eq!(mask.height, 100);
        assert!(!mask.data_b64.is_empty());
        assert!(mask.confidence > 0.35);
    }
}
