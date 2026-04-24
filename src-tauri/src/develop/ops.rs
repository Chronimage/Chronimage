//! The edit operations value object — a flat struct of slider values + a
//! blended curve. Serde-serialises to JSON that lives in
//! `edits.operations_json`. Every field is a plain scalar so interpolation
//! between presets (strength 0 → 100) is a linear lerp per field.
//!
//! Sliders are all in `-100..=100` except:
//! - `exposure` — EV, -4..=4 (UI maps via × 25 scaler)
//! - `temp` — Kelvin delta, -50..=50 (UI cue only; pipeline interprets as
//!   a warm/cool RGB channel shift)

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
}

impl Default for Operations {
    fn default() -> Self {
        Self::identity()
    }
}

impl Operations {
    /// "As imported" — no edits applied. Every field is 0; the pipeline
    /// returns its input untouched.
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
