//! The edit operations value object — a flat struct of slider values +
//! tone curves. Serde-serialises to JSON that lives in
//! `edits.operations_json`. Every field is a plain scalar (or a fixed-size
//! array for curves) so interpolation between presets (strength 0 → 100)
//! is a linear lerp per field / per control point.
//!
//! Sliders are all in `-100..=100` except:
//! - `exposure` — EV, -4..=4 (UI maps via × 25 scaler)
//! - `temp` — Kelvin delta, -50..=50 (UI cue only; pipeline interprets as
//!   a warm/cool RGB channel shift)
//!
//! Curves hold between 2 and 16 control points per channel; each point
//! is `[x, y]` in `[0, 1]`, sorted by x. The 5-point identity (diagonal
//! `y = x` with stops at blacks / shadows / mids / highlights / whites)
//! is the default shape, but the UI can insert or remove points
//! anywhere in the middle. The pipeline runs a **monotone cubic
//! Hermite** spline (Fritsch-Carlson) through the points to build a
//! 256-entry LUT — same interpolant used by Lightroom / Capture One.
//! Five channels are stored: `rgb` (composite master), `r`/`g`/`b`
//! per-channel, and `l` (luma).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operations {
    /// EV stops. Negative = darker.
    #[serde(default)]
    pub exposure: f32,
    #[serde(default)]
    pub contrast: f32,
    #[serde(default)]
    pub highlights: f32,
    #[serde(default)]
    pub shadows: f32,
    #[serde(default)]
    pub whites: f32,
    #[serde(default)]
    pub blacks: f32,
    /// Warm/cool shift.
    #[serde(default)]
    pub temp: f32,
    /// Magenta/green shift.
    #[serde(default)]
    pub tint: f32,
    #[serde(default)]
    pub vibrance: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub clarity: f32,
    #[serde(default)]
    pub dehaze: f32,
    /// Normalized crop rectangle. Identity is x=0, y=0, w=1, h=1.
    #[serde(default)]
    pub crop_x: f32,
    #[serde(default)]
    pub crop_y: f32,
    #[serde(default = "unit")]
    pub crop_w: f32,
    #[serde(default = "unit")]
    pub crop_h: f32,
    /// Degrees clockwise. `straighten` is a fine adjustment for horizon tools.
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub straighten: f32,
    #[serde(default)]
    pub transform_h: f32,
    #[serde(default)]
    pub transform_v: f32,
    /// Lens correction controls. Stored now; applied by the full RAW renderer.
    #[serde(default)]
    pub lens_distortion: f32,
    #[serde(default)]
    pub lens_vignette: f32,
    #[serde(default)]
    pub chromatic_aberration: f32,
    #[serde(default)]
    pub spot_heal_count: f32,
    #[serde(default)]
    pub lens_blur_amount: f32,
    #[serde(default)]
    pub lens_blur_focus_near: f32,
    #[serde(default = "unit")]
    pub lens_blur_focus_far: f32,
    #[serde(default)]
    pub lens_blur_bokeh_boost: f32,
    #[serde(default)]
    pub lens_blur_cat_eye: f32,
    /// Mid-frequency local-contrast slider, distinct from `clarity` which
    /// works on a smaller scale. -100..100; matches Lightroom Texture.
    #[serde(default)]
    pub texture: f32,
    /// Capture sharpening (post-decode). All fields zero is a no-op.
    #[serde(default)]
    pub sharpening: Sharpening,
    /// Procedural luma grain. Identity is amount=0.
    #[serde(default)]
    pub grain: Grain,
    /// Per-hue-band Hue / Saturation / Luminance offsets — Lightroom's
    /// "Color Mixer" panel. Eight bands centered at red/orange/yellow/
    /// green/aqua/blue/purple/magenta.
    #[serde(default)]
    pub color_mixer: ColorMixer,
    /// 3-zone tone-weighted hue/saturation/luminance shifts — Lightroom's
    /// "Color Grading" panel. Identity is every wheel + the global wheel
    /// at saturation=0 (hue is then irrelevant) and balance=0.
    #[serde(default)]
    pub color_grading: ColorGrading,
    /// Defringe (chromatic aberration cleanup) — purple + green channels
    /// independently, each with an amount and hue-band range.
    #[serde(default)]
    pub defringe: Defringe,
    /// Tone curves — master RGB + per-channel R/G/B + luma. Default is
    /// identity on every channel (no-op). See module doc.
    #[serde(default)]
    pub curves: Curves,
}

/// Capture sharpening parameters (Lightroom's Detail panel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Sharpening {
    /// 0..150 (Lightroom uses 0..150). 0 = off.
    #[serde(default)]
    pub amount: f32,
    /// Gaussian radius in px. Lightroom range 0.5..3.0; default 1.0.
    #[serde(default)]
    pub radius: f32,
    /// 0..100. Higher = sharpens fine details, lower = sharpens edges.
    #[serde(default)]
    pub detail: f32,
    /// 0..100. Edge mask threshold — gates sharpening to high-contrast
    /// regions, leaving smooth tonal areas (skin, sky) unsharpened.
    #[serde(default)]
    pub masking: f32,
}

/// Procedural luma grain (Lightroom's Effects panel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Grain {
    /// 0..100. 0 = off.
    #[serde(default)]
    pub amount: f32,
    /// 0..100. Larger size = chunkier grain.
    #[serde(default)]
    pub size: f32,
    /// 0..100. Variance of the grain pattern.
    #[serde(default)]
    pub roughness: f32,
}

/// Per-pixel HSL offsets for one Color Mixer band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct HslAdjust {
    /// -100..100, mapped to ±30° hue shift inside the band.
    #[serde(default)]
    pub hue: f32,
    /// -100..100. Negative desaturates, positive boosts.
    #[serde(default)]
    pub saturation: f32,
    /// -100..100. Lightens or darkens the band.
    #[serde(default)]
    pub luminance: f32,
}

/// 8-band Color Mixer — the colors are fixed at the standard Lightroom
/// hue centers (every 45°): red 0°, orange 30°, yellow 60°, green 120°,
/// aqua 180°, blue 240°, purple 270°, magenta 300°.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ColorMixer {
    #[serde(default)]
    pub red: HslAdjust,
    #[serde(default)]
    pub orange: HslAdjust,
    #[serde(default)]
    pub yellow: HslAdjust,
    #[serde(default)]
    pub green: HslAdjust,
    #[serde(default)]
    pub aqua: HslAdjust,
    #[serde(default)]
    pub blue: HslAdjust,
    #[serde(default)]
    pub purple: HslAdjust,
    #[serde(default)]
    pub magenta: HslAdjust,
}

/// Hue/Sat/Luminance wheel for one Color Grading zone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct HslWheel {
    /// 0..360. Irrelevant when saturation = 0.
    #[serde(default)]
    pub hue: f32,
    /// 0..100. 0 = neutral (no tint).
    #[serde(default)]
    pub saturation: f32,
    /// -100..100. Lightens or darkens the zone.
    #[serde(default)]
    pub luminance: f32,
}

/// 3-zone Color Grading: shadows / midtones / highlights, plus a global
/// wheel applied across the whole image. `blending` controls how much
/// adjacent zones overlap; `balance` shifts the luminance threshold
/// between shadows and highlights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorGrading {
    #[serde(default)]
    pub shadows: HslWheel,
    #[serde(default)]
    pub midtones: HslWheel,
    #[serde(default)]
    pub highlights: HslWheel,
    #[serde(default)]
    pub global: HslWheel,
    /// 0..100; identity when 50 (default — overlap matches Lightroom).
    #[serde(default = "default_blending")]
    pub blending: f32,
    /// -100..100. Identity = 0.
    #[serde(default)]
    pub balance: f32,
}

impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            shadows: HslWheel::default(),
            midtones: HslWheel::default(),
            highlights: HslWheel::default(),
            global: HslWheel::default(),
            blending: default_blending(),
            balance: 0.0,
        }
    }
}

fn default_blending() -> f32 {
    50.0
}

/// Defringe (chromatic aberration cleanup).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Defringe {
    /// Purple amount, 0..20. 0 = off.
    #[serde(default)]
    pub purple_amount: f32,
    /// Purple hue-band width, 0..100.
    #[serde(default)]
    pub purple_hue_range: f32,
    /// Green amount, 0..20. 0 = off.
    #[serde(default)]
    pub green_amount: f32,
    /// Green hue-band width, 0..100.
    #[serde(default)]
    pub green_hue_range: f32,
}

/// Variable-length curve for one channel: 2..=16 control points in
/// `[0,1]^2`, sorted left-to-right on the x axis. The UI lets the user
/// insert or remove points in the middle; endpoints stay at x=0 and x=1.
pub type Curve = Vec<[f32; 2]>;

/// Identity curve — the diagonal `y = x` at five evenly spaced x points.
pub fn identity_curve() -> Curve {
    vec![
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ]
}

fn unit() -> f32 {
    1.0
}

/// Maximum number of control points per channel. Keeps the on-disk JSON
/// bounded + gives the LUT baker a predictable upper bound on work.
pub const MAX_CURVE_POINTS: usize = 16;

/// Per-channel tone curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curves {
    #[serde(default = "identity_curve")]
    pub rgb: Curve,
    #[serde(default = "identity_curve")]
    pub r: Curve,
    #[serde(default = "identity_curve")]
    pub g: Curve,
    #[serde(default = "identity_curve")]
    pub b: Curve,
    #[serde(default = "identity_curve")]
    pub l: Curve,
}

impl Curves {
    pub fn identity() -> Self {
        Self {
            rgb: identity_curve(),
            r: identity_curve(),
            g: identity_curve(),
            b: identity_curve(),
            l: identity_curve(),
        }
    }

    /// True iff every channel is an identity curve — any variant that
    /// evaluates to y = x at every x (any length ≥ 2 with all points on
    /// the diagonal). The pipeline skips the whole curves stage when
    /// this is true.
    pub fn is_identity(&self) -> bool {
        is_identity_curve(&self.rgb)
            && is_identity_curve(&self.r)
            && is_identity_curve(&self.g)
            && is_identity_curve(&self.b)
            && is_identity_curve(&self.l)
    }

    fn blend_channel(a: &Curve, b: &Curve, t: f32) -> Curve {
        // Curve lengths can differ; if they do, fall back to whichever
        // side has more points (no interpolation). That's the safe
        // choice — blending between different-shape curves isn't a
        // supported operation yet; presets define their own.
        if a.len() != b.len() {
            return if t < 0.5 { a.clone() } else { b.clone() };
        }
        let mut out: Curve = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            let ax = a[i][0];
            let ay = a[i][1];
            let bx = b[i][0];
            let by = b[i][1];
            out.push([ax + (bx - ax) * t, ay + (by - ay) * t]);
        }
        out
    }

    pub fn blend(&self, other: &Self, t: f32) -> Self {
        Self {
            rgb: Self::blend_channel(&self.rgb, &other.rgb, t),
            r: Self::blend_channel(&self.r, &other.r, t),
            g: Self::blend_channel(&self.g, &other.g, t),
            b: Self::blend_channel(&self.b, &other.b, t),
            l: Self::blend_channel(&self.l, &other.l, t),
        }
    }
}

impl Default for Curves {
    fn default() -> Self {
        Self::identity()
    }
}

/// Check whether a curve is effectively `y = x`. Any length ≥ 2 counts
/// as identity as long as every control point sits on the diagonal.
pub fn is_identity_curve(curve: &Curve) -> bool {
    const EPS: f32 = 1e-4;
    for p in curve {
        if (p[0] - p[1]).abs() > EPS {
            return false;
        }
    }
    // Also require the two endpoints sit at the corners so we don't
    // misclassify a partial 2-point curve like [[0,0],[0.5,0.5]] — the
    // x=1 side is implicitly the diagonal extension too, so that case
    // is genuinely identity in the 0..0.5 range but the LUT baker
    // would extrapolate a flat tail. Keep strict for correctness.
    if curve.len() < 2 {
        return false;
    }
    let first = curve[0];
    let last = curve[curve.len() - 1];
    (first[0]).abs() < EPS
        && (first[1]).abs() < EPS
        && (last[0] - 1.0).abs() < EPS
        && (last[1] - 1.0).abs() < EPS
}

impl Default for Operations {
    fn default() -> Self {
        Self::identity()
    }
}

impl Operations {
    /// "As imported" — no edits applied. Every field is 0; curves are
    /// the diagonal; the pipeline returns its input untouched.
    pub fn identity() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temp: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_w: 1.0,
            crop_h: 1.0,
            rotation: 0.0,
            straighten: 0.0,
            transform_h: 0.0,
            transform_v: 0.0,
            lens_distortion: 0.0,
            lens_vignette: 0.0,
            chromatic_aberration: 0.0,
            spot_heal_count: 0.0,
            lens_blur_amount: 0.0,
            lens_blur_focus_near: 0.0,
            lens_blur_focus_far: 1.0,
            lens_blur_bokeh_boost: 0.0,
            lens_blur_cat_eye: 0.0,
            texture: 0.0,
            sharpening: Sharpening::default(),
            grain: Grain::default(),
            color_mixer: ColorMixer::default(),
            color_grading: ColorGrading {
                blending: 50.0,
                ..ColorGrading::default()
            },
            defringe: Defringe::default(),
            curves: Curves::identity(),
        }
    }

    /// True iff every field is identity-equal. Used by the pipeline to
    /// skip the whole per-pixel pass when the preset is a no-op.
    pub fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    /// Blend `self` toward `target` by `strength ∈ [0, 100]`. Linear per
    /// field. Returns `self` when strength=0, `target` when strength=100.
    pub fn blend(self, target: Self, strength: u8) -> Self {
        let t = (strength.min(100) as f32) / 100.0;
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Self {
            exposure: lerp(self.exposure, target.exposure),
            contrast: lerp(self.contrast, target.contrast),
            highlights: lerp(self.highlights, target.highlights),
            shadows: lerp(self.shadows, target.shadows),
            whites: lerp(self.whites, target.whites),
            blacks: lerp(self.blacks, target.blacks),
            temp: lerp(self.temp, target.temp),
            tint: lerp(self.tint, target.tint),
            vibrance: lerp(self.vibrance, target.vibrance),
            saturation: lerp(self.saturation, target.saturation),
            clarity: lerp(self.clarity, target.clarity),
            dehaze: lerp(self.dehaze, target.dehaze),
            crop_x: lerp(self.crop_x, target.crop_x),
            crop_y: lerp(self.crop_y, target.crop_y),
            crop_w: lerp(self.crop_w, target.crop_w),
            crop_h: lerp(self.crop_h, target.crop_h),
            rotation: lerp(self.rotation, target.rotation),
            straighten: lerp(self.straighten, target.straighten),
            transform_h: lerp(self.transform_h, target.transform_h),
            transform_v: lerp(self.transform_v, target.transform_v),
            lens_distortion: lerp(self.lens_distortion, target.lens_distortion),
            lens_vignette: lerp(self.lens_vignette, target.lens_vignette),
            chromatic_aberration: lerp(self.chromatic_aberration, target.chromatic_aberration),
            spot_heal_count: lerp(self.spot_heal_count, target.spot_heal_count),
            lens_blur_amount: lerp(self.lens_blur_amount, target.lens_blur_amount),
            lens_blur_focus_near: lerp(self.lens_blur_focus_near, target.lens_blur_focus_near),
            lens_blur_focus_far: lerp(self.lens_blur_focus_far, target.lens_blur_focus_far),
            lens_blur_bokeh_boost: lerp(self.lens_blur_bokeh_boost, target.lens_blur_bokeh_boost),
            lens_blur_cat_eye: lerp(self.lens_blur_cat_eye, target.lens_blur_cat_eye),
            texture: lerp(self.texture, target.texture),
            sharpening: blend_sharpening(&self.sharpening, &target.sharpening, t),
            grain: blend_grain(&self.grain, &target.grain, t),
            color_mixer: blend_color_mixer(&self.color_mixer, &target.color_mixer, t),
            color_grading: blend_color_grading(&self.color_grading, &target.color_grading, t),
            defringe: blend_defringe(&self.defringe, &target.defringe, t),
            curves: self.curves.blend(&target.curves, t),
        }
    }
}

fn lerp_field(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn blend_sharpening(a: &Sharpening, b: &Sharpening, t: f32) -> Sharpening {
    Sharpening {
        amount: lerp_field(a.amount, b.amount, t),
        radius: lerp_field(a.radius, b.radius, t),
        detail: lerp_field(a.detail, b.detail, t),
        masking: lerp_field(a.masking, b.masking, t),
    }
}

fn blend_grain(a: &Grain, b: &Grain, t: f32) -> Grain {
    Grain {
        amount: lerp_field(a.amount, b.amount, t),
        size: lerp_field(a.size, b.size, t),
        roughness: lerp_field(a.roughness, b.roughness, t),
    }
}

fn blend_hsl(a: &HslAdjust, b: &HslAdjust, t: f32) -> HslAdjust {
    HslAdjust {
        hue: lerp_field(a.hue, b.hue, t),
        saturation: lerp_field(a.saturation, b.saturation, t),
        luminance: lerp_field(a.luminance, b.luminance, t),
    }
}

fn blend_color_mixer(a: &ColorMixer, b: &ColorMixer, t: f32) -> ColorMixer {
    ColorMixer {
        red: blend_hsl(&a.red, &b.red, t),
        orange: blend_hsl(&a.orange, &b.orange, t),
        yellow: blend_hsl(&a.yellow, &b.yellow, t),
        green: blend_hsl(&a.green, &b.green, t),
        aqua: blend_hsl(&a.aqua, &b.aqua, t),
        blue: blend_hsl(&a.blue, &b.blue, t),
        purple: blend_hsl(&a.purple, &b.purple, t),
        magenta: blend_hsl(&a.magenta, &b.magenta, t),
    }
}

fn blend_wheel(a: &HslWheel, b: &HslWheel, t: f32) -> HslWheel {
    HslWheel {
        hue: lerp_field(a.hue, b.hue, t),
        saturation: lerp_field(a.saturation, b.saturation, t),
        luminance: lerp_field(a.luminance, b.luminance, t),
    }
}

fn blend_color_grading(a: &ColorGrading, b: &ColorGrading, t: f32) -> ColorGrading {
    ColorGrading {
        shadows: blend_wheel(&a.shadows, &b.shadows, t),
        midtones: blend_wheel(&a.midtones, &b.midtones, t),
        highlights: blend_wheel(&a.highlights, &b.highlights, t),
        global: blend_wheel(&a.global, &b.global, t),
        blending: lerp_field(a.blending, b.blending, t),
        balance: lerp_field(a.balance, b.balance, t),
    }
}

fn blend_defringe(a: &Defringe, b: &Defringe, t: f32) -> Defringe {
    Defringe {
        purple_amount: lerp_field(a.purple_amount, b.purple_amount, t),
        purple_hue_range: lerp_field(a.purple_hue_range, b.purple_hue_range, t),
        green_amount: lerp_field(a.green_amount, b.green_amount, t),
        green_hue_range: lerp_field(a.green_hue_range, b.green_hue_range, t),
    }
}

/// Receipt returned by `develop_apply` — the preview output as a base64
/// data URL + the photo id it's for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderReceipt {
    pub photo_id: i64,
    /// `data:image/jpeg;base64,…` so the frontend can drop it straight
    /// into an `<img src>`.
    pub preview_data_url: String,
    pub elapsed_ms: u64,
}

/// Receipt returned by `develop_paste_edits`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedReceipt {
    pub pasted_photo_count: usize,
    pub skipped: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrips_through_json() {
        let ops = Operations::identity();
        let j = serde_json::to_string(&ops).unwrap();
        let back: Operations = serde_json::from_str(&j).unwrap();
        assert!(back.is_identity());
    }

    #[test]
    fn missing_fields_default_to_zero() {
        let ops: Operations = serde_json::from_str("{}").unwrap();
        assert!(ops.is_identity());
    }

    #[test]
    fn blend_at_zero_is_self() {
        let a = Operations {
            exposure: 0.5,
            ..Operations::identity()
        };
        let b = Operations {
            exposure: 1.5,
            ..Operations::identity()
        };
        let c = a.blend(b, 0);
        assert!((c.exposure - 0.5).abs() < 1e-6);
    }

    #[test]
    fn blend_at_hundred_is_target() {
        let a = Operations::identity();
        let b = Operations {
            exposure: 1.0,
            saturation: 40.0,
            ..Operations::identity()
        };
        let c = a.blend(b, 100);
        assert!((c.exposure - 1.0).abs() < 1e-6);
        assert!((c.saturation - 40.0).abs() < 1e-6);
    }

    #[test]
    fn blend_at_fifty_is_half() {
        let a = Operations {
            exposure: 0.0,
            ..Operations::identity()
        };
        let b = Operations {
            exposure: 2.0,
            ..Operations::identity()
        };
        let c = a.blend(b, 50);
        assert!((c.exposure - 1.0).abs() < 1e-6);
    }
}
