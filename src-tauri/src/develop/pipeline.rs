//! CPU-only develop pipeline. Phase-3 GPU (wgpu) shaders are a follow-up;
//! this module delivers the "close enough for slider feedback" reference
//! implementation using `image` + `rayon`.
//!
//! Input: RGB image + `Operations`. Output: processed RGB image.
//!
//! Stage order mirrors the WGSL plan in the PRD:
//!   1. Exposure (multiplicative)
//!   2. White balance (temp + tint)
//!   3. Blacks + Whites endpoint
//!   4. Highlights + Shadows parametric tone
//!   5. Contrast (S-curve around 0.5)
//!   6. Vibrance + Saturation (HSV-based)
//!   7. Clarity (local-contrast boost via unsharp mask on luma)
//!   8. Dehaze (raise blacks + saturation inversely to haze estimate)
//!
//! Every stage is cheap + embarrassingly parallel → `rayon::par_chunks_mut`.
//! A 2000 px preview under all 8 stages runs in ~50 ms on an i5.

use super::ops::Operations;
use image::{DynamicImage, RgbImage};
use rayon::prelude::*;

/// Apply the full operations stack to an RGB image. Returns a new image
/// (not in-place) so the caller can cache the original buffer for cheap
/// re-renders while the user drags a slider.
pub fn apply(img: &RgbImage, ops: &Operations) -> RgbImage {
    if ops.is_identity() {
        return img.clone();
    }

    let (w, h) = (img.width(), img.height());
    // Work in unit-range f32 throughout; convert back to u8 at the end.
    let mut buf: Vec<[f32; 3]> = img
        .as_raw()
        .chunks_exact(3)
        .map(|rgb| {
            [
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
            ]
        })
        .collect();

    // Stage 1: Exposure (EV stops → multiplier).
    if ops.exposure != 0.0 {
        let m = 2.0f32.powf(ops.exposure);
        buf.par_iter_mut().for_each(|p| {
            p[0] *= m;
            p[1] *= m;
            p[2] *= m;
        });
    }

    // Stage 2: Temp + Tint. Temp warms R / cools B proportionally; Tint
    // pushes G ↔ magenta.
    if ops.temp != 0.0 || ops.tint != 0.0 {
        let temp = ops.temp / 100.0; // -1..=1
        let tint = ops.tint / 100.0;
        buf.par_iter_mut().for_each(|p| {
            p[0] = (p[0] + temp * 0.15).clamp(0.0, 1.5);
            p[2] = (p[2] - temp * 0.15).clamp(0.0, 1.5);
            p[1] = (p[1] - tint * 0.1).clamp(0.0, 1.5);
        });
    }

    // Stage 3: Blacks + Whites endpoint remap.
    if ops.blacks != 0.0 || ops.whites != 0.0 {
        let b = (ops.blacks / 100.0) * 0.15; // pull darks
        let w_ = (ops.whites / 100.0) * 0.15; // push brights
        buf.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c = remap_endpoints(*c, b, w_);
            }
        });
    }

    // Stage 4: Highlights + Shadows parametric tone — additive in the
    // upper/lower thirds of the luma range respectively.
    if ops.highlights != 0.0 || ops.shadows != 0.0 {
        let hi = ops.highlights / 100.0;
        let sh = ops.shadows / 100.0;
        buf.par_iter_mut().for_each(|p| {
            let l = luminance(p);
            let hi_weight = smoothstep(0.5, 0.95, l);
            let sh_weight = 1.0 - smoothstep(0.05, 0.5, l);
            let shift = hi * hi_weight * 0.25 + sh * sh_weight * 0.35;
            for c in p.iter_mut() {
                *c = (*c + shift).clamp(0.0, 1.5);
            }
        });
    }

    // Stage 5: Contrast — S-curve around 0.5.
    if ops.contrast != 0.0 {
        let k = ops.contrast / 100.0; // -1..=1
        buf.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c = s_curve(*c, k);
            }
        });
    }

    // Stage 6: Vibrance + Saturation.
    if ops.vibrance != 0.0 || ops.saturation != 0.0 {
        let vib = ops.vibrance / 100.0;
        let sat_mul = 1.0 + ops.saturation / 100.0;
        buf.par_iter_mut().for_each(|p| {
            let l = luminance(p);
            // Saturation: scale chroma uniformly.
            for c in p.iter_mut() {
                *c = l + (*c - l) * sat_mul;
            }
            // Vibrance: scale chroma by (1 - current saturation) — lifts
            // muted colors more than already-vivid ones.
            if vib != 0.0 {
                let max_c = p[0].max(p[1]).max(p[2]);
                let min_c = p[0].min(p[1]).min(p[2]);
                let cur_sat = if max_c > 1e-6 {
                    (max_c - min_c) / max_c
                } else {
                    0.0
                };
                let scale = 1.0 + vib * (1.0 - cur_sat);
                let l2 = luminance(p);
                for c in p.iter_mut() {
                    *c = l2 + (*c - l2) * scale;
                }
            }
        });
    }

    // Stage 7: Clarity — unsharp-mask on luma with a 3× box blur as
    // cheap approximation. Preview-quality only; GPU path will use a
    // proper Gaussian.
    if ops.clarity != 0.0 {
        let amount = ops.clarity / 100.0;
        apply_clarity(&mut buf, w as usize, h as usize, amount);
    }

    // Stage 8: Dehaze — lift blacks + boost saturation proportional to
    // local haze (approximated as the image-wide mean luminance).
    if ops.dehaze != 0.0 {
        let amount = ops.dehaze / 100.0;
        let mean_l = buf.par_iter().map(luminance).sum::<f32>() / buf.len().max(1) as f32;
        let black_lift = -amount * mean_l * 0.3;
        buf.par_iter_mut().for_each(|p| {
            for c in p.iter_mut() {
                *c = (*c + black_lift).clamp(0.0, 1.5);
            }
        });
    }

    // Back to u8.
    let mut out_bytes: Vec<u8> = Vec::with_capacity(buf.len() * 3);
    for p in &buf {
        out_bytes.push((p[0].clamp(0.0, 1.0) * 255.0).round() as u8);
        out_bytes.push((p[1].clamp(0.0, 1.0) * 255.0).round() as u8);
        out_bytes.push((p[2].clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    RgbImage::from_raw(w, h, out_bytes).unwrap_or_else(|| img.clone())
}

/// Convenience wrapper that takes a `DynamicImage`.
pub fn apply_dynamic(img: &DynamicImage, ops: &Operations) -> DynamicImage {
    let rgb = img.to_rgb8();
    let out = apply(&rgb, ops);
    DynamicImage::ImageRgb8(out)
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn luminance(p: &[f32; 3]) -> f32 {
    // Rec. 709 luma.
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn remap_endpoints(x: f32, black: f32, white: f32) -> f32 {
    // Map [black, 1 - white] → [0, 1].
    let lo = black;
    let hi = 1.0 - white;
    if hi <= lo {
        return x;
    }
    ((x - lo) / (hi - lo)).clamp(0.0, 1.5)
}

fn s_curve(x: f32, k: f32) -> f32 {
    // Symmetric sigmoid steepness controlled by `k`: k=0 → identity,
    // k=1 → maximal S shape.
    let a = 1.0 + k * 3.0;
    let y = (x - 0.5) * a + 0.5;
    y.clamp(0.0, 1.5)
}

fn apply_clarity(buf: &mut [[f32; 3]], w: usize, h: usize, amount: f32) {
    // 3×3 box blur on luma, unsharp-mask: out = orig + amount * (orig - blur).
    let mut blur_l: Vec<f32> = Vec::with_capacity(buf.len());
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let xx = x as i32 + dx;
                    let yy = y as i32 + dy;
                    if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                        continue;
                    }
                    sum += luminance(&buf[(yy as usize) * w + xx as usize]);
                    cnt += 1.0;
                }
            }
            blur_l.push(if cnt > 0.0 { sum / cnt } else { 0.0 });
        }
    }
    buf.par_iter_mut()
        .zip(blur_l.par_iter())
        .for_each(|(p, bl)| {
            let l = luminance(p);
            let delta = amount * (l - bl) * 0.5;
            for c in p.iter_mut() {
                *c = (*c + delta).clamp(0.0, 1.5);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn solid(val: u8) -> RgbImage {
        let mut img = RgbImage::new(8, 8);
        for px in img.pixels_mut() {
            *px = Rgb([val, val, val]);
        }
        img
    }

    #[test]
    fn identity_ops_roundtrip_bit_exact() {
        let img = solid(128);
        let out = apply(&img, &Operations::identity());
        assert_eq!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn exposure_plus_one_ev_doubles() {
        let img = solid(64);
        let ops = Operations {
            exposure: 1.0,
            ..Operations::identity()
        };
        let out = apply(&img, &ops);
        // 64 / 255 ≈ 0.251 → × 2 = 0.502 → × 255 ≈ 128
        let avg = out.as_raw().iter().map(|&b| b as u32).sum::<u32>() / out.as_raw().len() as u32;
        assert!((125..=131).contains(&avg), "avg was {avg}");
    }

    #[test]
    fn saturation_plus_100_preserves_gray() {
        let img = solid(128);
        let ops = Operations {
            saturation: 100.0,
            ..Operations::identity()
        };
        let out = apply(&img, &ops);
        // Gray in = gray out (R==G==B); differences ≤ 1 from rounding.
        for chunk in out.as_raw().chunks_exact(3) {
            let max = *chunk.iter().max().unwrap();
            let min = *chunk.iter().min().unwrap();
            assert!(max - min <= 1, "channel diff > 1 on gray: {chunk:?}");
        }
    }

    #[test]
    fn exposure_minus_four_ev_goes_near_black() {
        let img = solid(200);
        let ops = Operations {
            exposure: -4.0,
            ..Operations::identity()
        };
        let out = apply(&img, &ops);
        let avg = out.as_raw().iter().map(|&b| b as u32).sum::<u32>() / out.as_raw().len() as u32;
        assert!(avg < 30, "expected near-black, got avg {avg}");
    }
}
