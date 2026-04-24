//! Export preset — format, quality, resize, metadata strip flags.
//!
//! Serialised into `export_jobs.preset_json` so we can reconstruct an
//! in-flight job after an app restart.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Jpeg,
    Tiff,
    /// HEIC is parsed here but the engine returns
    /// `InvalidInput("heic export not yet supported")` — drop in when
    /// `libheif-rs` is added as a dep.
    Heic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorProfile {
    Srgb,
    /// Deferred — engine warns + falls back to sRGB encoding until the
    /// Phase-3 RAW color pipeline lands.
    DisplayP3,
    /// Deferred — same fallback as P3.
    AdobeRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StripMeta {
    #[serde(default = "default_true")]
    pub gps: bool,
    #[serde(default)]
    pub all_exif: bool,
    #[serde(default)]
    pub camera_serial: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPreset {
    pub format: Format,
    pub color: ColorProfile,
    /// 0..=100, JPEG/HEIC only. TIFF ignores.
    pub quality: u8,
    /// Long-edge in pixels. Photos shorter than this are NOT upscaled.
    pub long_edge_px: u32,
    pub strip_meta: StripMeta,
    /// Optional text watermark rendered in the bottom-right of the export.
    /// Phase 4 wires the image picker; Phase 2 ships text only.
    pub watermark_text: Option<String>,
    /// If true, copy the original source_copies file into
    /// `<output_dir>/_originals/` before writing the export.
    #[serde(default)]
    pub archive_originals: bool,
}

impl ExportPreset {
    /// Reasonable defaults for a "quick export for sharing" flow.
    pub fn web_default() -> Self {
        Self {
            format: Format::Jpeg,
            color: ColorProfile::Srgb,
            quality: 88,
            long_edge_px: 2000,
            strip_meta: StripMeta {
                gps: true,
                all_exif: false,
                camera_serial: false,
            },
            watermark_text: None,
            archive_originals: false,
        }
    }
}

impl Default for ExportPreset {
    fn default() -> Self {
        Self::web_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let p = ExportPreset::web_default();
        let j = serde_json::to_string(&p).unwrap();
        let back: ExportPreset = serde_json::from_str(&j).unwrap();
        assert_eq!(p.quality, back.quality);
        assert_eq!(p.long_edge_px, back.long_edge_px);
    }

    #[test]
    fn strip_meta_defaults_gps_on() {
        let s: StripMeta = serde_json::from_str("{}").unwrap();
        assert!(s.gps);
        assert!(!s.all_exif);
    }
}
