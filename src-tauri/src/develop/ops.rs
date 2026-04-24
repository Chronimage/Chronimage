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
//! Curves are fixed at 5 control points per channel (blacks / shadows /
//! mids / highlights / whites). Each point is `[x, y]` in `[0, 1]`.
//! Identity = the diagonal `y = x`; the pipeline runs a Catmull-Rom
//! spline through the 5 points to build a 256-entry LUT. Five channels
//! are stored — `rgb` (the composite master curve) + `r`/`g`/`b` per-
//! channel + `l` (luma).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    /// Tone curves — master RGB + per-channel R/G/B + luma. Default is
    /// identity on every channel (no-op). See module doc.
    #[serde(default)]
    pub curves: Curves,
}

/// Fixed-size curve for one channel: 5 control points in `[0,1]^2`.
/// Order is left-to-right on the x axis. Serialised as a `[[f32; 2]; 5]`
/// for compact storage + obvious JSON shape.
pub type Curve = [[f32; 2]; 5];

/// Identity curve — the diagonal `y = x` at five evenly spaced x points.
pub const fn identity_curve() -> Curve {
    [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ]
}

/// Per-channel tone curves. Each field is a 5-point curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    pub const fn identity() -> Self {
        Self {
            rgb: identity_curve(),
            r: identity_curve(),
            g: identity_curve(),
            b: identity_curve(),
            l: identity_curve(),
        }
    }

    /// True iff every channel is the identity (y = x) curve. The pipeline
    /// skips the whole curves stage when this is true.
    pub fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    fn blend_channel(a: &Curve, b: &Curve, t: f32) -> Curve {
        let mut out = identity_curve();
        for i in 0..5 {
            out[i][0] = a[i][0] + (b[i][0] - a[i][0]) * t;
            out[i][1] = a[i][1] + (b[i][1] - a[i][1]) * t;
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

impl Default for Operations {
    fn default() -> Self {
        Self::identity()
    }
}

impl Operations {
    /// "As imported" — no edits applied. Every field is 0; curves are
    /// the diagonal; the pipeline returns its input untouched.
    pub const fn identity() -> Self {
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
            curves: self.curves.blend(&target.curves, t),
        }
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
