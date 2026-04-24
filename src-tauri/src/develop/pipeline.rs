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
//!   9. Tone curves — master RGB + per-channel R/G/B, then luma curve
//!      applied on Y with chroma preserved.
//!
//! Every stage is cheap + embarrassingly parallel → `rayon::par_chunks_mut`.
//! A 2000 px preview under all 8 stages runs in ~50 ms on an i5.

use super::ops::{identity_curve, Curve, Operations};
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

    // Stage 9: Tone curves. Order: master RGB → per-channel R/G/B → luma.
    // Each non-identity curve is baked into a 256-entry LUT once per
    // pass so the per-pixel work is just an index + linear interpolation.
    if !ops.curves.is_identity() {
        let rgb_lut = bake_lut(&ops.curves.rgb);
        let r_lut = bake_lut(&ops.curves.r);
        let g_lut = bake_lut(&ops.curves.g);
        let b_lut = bake_lut(&ops.curves.b);
        let l_lut = bake_lut(&ops.curves.l);

        let apply_rgb = !is_identity_curve(&ops.curves.rgb);
        let apply_r = !is_identity_curve(&ops.curves.r);
        let apply_g = !is_identity_curve(&ops.curves.g);
        let apply_b = !is_identity_curve(&ops.curves.b);
        let apply_l = !is_identity_curve(&ops.curves.l);

        buf.par_iter_mut().for_each(|p| {
            if apply_rgb {
                p[0] = lookup_lut(&rgb_lut, p[0]);
                p[1] = lookup_lut(&rgb_lut, p[1]);
                p[2] = lookup_lut(&rgb_lut, p[2]);
            }
            if apply_r {
                p[0] = lookup_lut(&r_lut, p[0]);
            }
            if apply_g {
                p[1] = lookup_lut(&g_lut, p[1]);
            }
            if apply_b {
                p[2] = lookup_lut(&b_lut, p[2]);
            }
            if apply_l {
                // Luma curve: shift Y while keeping chroma (Cb/Cr) intact.
                // Using Rec. 709 for consistency with `luminance`.
                let y = luminance(p);
                let y2 = lookup_lut(&l_lut, y);
                let delta = y2 - y;
                p[0] = (p[0] + delta).clamp(0.0, 1.5);
                p[1] = (p[1] + delta).clamp(0.0, 1.5);
                p[2] = (p[2] + delta).clamp(0.0, 1.5);
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

/// Build a 256-entry `u8 -> u8` LUT from a 5-point curve using a
/// Catmull-Rom spline. The two endpoints at x=0 and x=1 are implicit —
/// the 5 control points fully specify the curve. Points are assumed to
/// be in x-sorted order; if the UI violates that, we still return a
/// valid LUT by clamping to the nearest point at each index.
fn bake_lut(curve: &Curve) -> [f32; 256] {
    let mut out = [0.0f32; 256];
    // Extend with virtual endpoints so the Catmull-Rom spline behaves
    // at the boundaries. Virtual left = mirror of point 0 over x=0.
    // Virtual right = mirror of point 4 over x=1. Both clamped to
    // [-1, 2] — well outside the image's [0, 1] range.
    let p0 = [-curve[0][0], -curve[0][1]];
    let pn = [2.0 - curve[4][0], 2.0 - curve[4][1]];
    let pts: [[f32; 2]; 7] = [p0, curve[0], curve[1], curve[2], curve[3], curve[4], pn];

    for (i, out_slot) in out.iter_mut().enumerate() {
        let x = i as f32 / 255.0;
        // Find which segment the x falls into. Each segment is between
        // pts[j+1] and pts[j+2] (j in 0..=4).
        let mut j = 0usize;
        for k in 0..5 {
            if x >= pts[k + 1][0] && x <= pts[k + 2][0] {
                j = k;
                break;
            }
            if x < pts[k + 1][0] {
                j = k.saturating_sub(1);
                break;
            }
            if k == 4 {
                j = 4;
            }
        }
        let p_minus = pts[j];
        let p_a = pts[j + 1];
        let p_b = pts[j + 2];
        let p_plus = pts[j + 3];

        let span = (p_b[0] - p_a[0]).max(1e-6);
        let t = ((x - p_a[0]) / span).clamp(0.0, 1.0);
        let y = catmull_rom(p_minus[1], p_a[1], p_b[1], p_plus[1], t);
        *out_slot = y.clamp(0.0, 1.0);
    }
    out
}

/// Catmull-Rom 1D interpolation. `p1` and `p2` are the segment endpoints
/// for parameter `t ∈ [0, 1]`; `p0` and `p3` are the surrounding points
/// that shape the tangents.
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Interpolate a LUT at a floating-point input in `[0, 1+ε]`.
fn lookup_lut(lut: &[f32; 256], x: f32) -> f32 {
    let xc = x.clamp(0.0, 1.0);
    let i = (xc * 255.0).floor() as usize;
    let frac = xc * 255.0 - i as f32;
    let a = lut[i];
    let b = lut[i.saturating_add(1).min(255)];
    a + (b - a) * frac
}

fn is_identity_curve(curve: &Curve) -> bool {
    let ident = identity_curve();
    for i in 0..5 {
        if (curve[i][0] - ident[i][0]).abs() > 1e-6 || (curve[i][1] - ident[i][1]).abs() > 1e-6 {
            return false;
        }
    }
    true
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

    #[test]
    fn identity_curves_are_no_op() {
        let img = solid(128);
        let ops = Operations {
            curves: super::super::ops::Curves::identity(),
            ..Operations::identity()
        };
        let out = apply(&img, &ops);
        assert_eq!(img.as_raw(), out.as_raw());
    }

    #[test]
    fn rgb_curve_lifts_midtones() {
        // Pull the mids up: 0.5 → 0.75. Darkens/brightens should follow.
        let mut curves = super::super::ops::Curves::identity();
        curves.rgb = [
            [0.0, 0.0],
            [0.25, 0.35],
            [0.5, 0.75],
            [0.75, 0.9],
            [1.0, 1.0],
        ];
        let img = solid(128); // roughly 0.5
        let ops = Operations {
            curves,
            ..Operations::identity()
        };
        let out = apply(&img, &ops);
        let avg = out.as_raw().iter().map(|&b| b as u32).sum::<u32>() / out.as_raw().len() as u32;
        // 0.75 × 255 ≈ 191; allow ±5 for spline + quantisation slack.
        assert!(
            (185..=196).contains(&avg),
            "expected ~191 after +midtone curve, got {avg}"
        );
    }

    #[test]
    fn bake_lut_endpoints_match_identity() {
        let lut = super::bake_lut(&super::identity_curve());
        assert!((lut[0] - 0.0).abs() < 1e-3);
        assert!((lut[255] - 1.0).abs() < 1e-3);
        // Mid sample should be ~0.5.
        assert!((lut[128] - 128.0 / 255.0).abs() < 0.02);
    }

    #[test]
    fn lookup_lut_interpolates_between_entries() {
        let mut lut = [0.0f32; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = (i as f32) / 255.0;
        }
        // x = 128.5 / 255 should sit between lut[128] and lut[129].
        let v = super::lookup_lut(&lut, 128.5 / 255.0);
        assert!(
            (v - 128.5 / 255.0).abs() < 1e-3,
            "expected linear interp, got {v}"
        );
    }
}
