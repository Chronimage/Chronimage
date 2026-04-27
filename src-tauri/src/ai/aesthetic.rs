//! NIMA aesthetic scorer via ONNX Runtime.
//!
//! Scores an image 1–10 for aesthetic quality by computing the expected value
//! of a 10-class quality-rating distribution predicted by the NIMA MobileNet model.
//!
//! Model path (relative to model root): `nima.onnx`
//! Input:  `input` — [1, 3, 224, 224] f32, ImageNet-normalised
//! Output: `output` — [1, 10] f32 (softmax distribution over ratings 1–10)

use crate::{ai::providers::session_builder_with_ep, AppError, AppResult};
use image::imageops::FilterType;
use ort::session::Session;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const INPUT_SIZE: u32 = 224;
/// Number of quality-rating bins (1 through 10).
const NUM_CLASSES: usize = 10;

// ImageNet mean/std per channel (R, G, B).
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A loaded NIMA scoring session.
///
/// `Session::run` requires `&mut self` in ort rc.12, so we wrap it in a
/// `Mutex` so the outer `&NimaSession` reference (from `OnceLock`) can
/// still drive inference across threads.
#[derive(Debug)]
pub struct NimaSession {
    session: Mutex<Session>,
}

impl NimaSession {
    /// Load the ONNX session from `model_path`.
    /// Returns `AppError::NotFound` when the file does not exist.
    pub fn load(model_path: &Path) -> AppResult<Self> {
        if !model_path.exists() {
            return Err(AppError::NotFound(
                "nima model not found — run model download first".into(),
            ));
        }
        let session = session_builder_with_ep("nima")
            .map_err(|e| AppError::Internal(format!("ort builder (nima): {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| AppError::Internal(format!("ort load nima: {e}")))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    /// Score a single image.
    ///
    /// Returns the expected aesthetic rating on the range [1.0, 10.0].
    /// `sha256` is an optional cache hint — when set, the AI-preview
    /// cache is consulted first to avoid redecoding HEIC/RAW.
    pub fn score(&self, image_path: &Path, sha256: Option<&str>) -> AppResult<f32> {
        let pixel_values = preprocess_image(image_path, sha256)?;
        // The bundled NIMA ONNX export uses NHWC (`[1, 224, 224, 3]`), not
        // NCHW. Feeding the model NCHW triggers `ort run: Got invalid
        // dimensions for input` at inference time — caught during the
        // 2026-04-23 debug-import investigation.
        let shape = vec![1i64, INPUT_SIZE as i64, INPUT_SIZE as i64, 3];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, pixel_values))
            .map_err(|e| AppError::Internal(format!("ort tensor: {e}")))?;

        // inputs! returns Vec directly (not Result) in ort rc.12.
        // Bind the guard to a local so it lives as long as `outputs`.
        let mut guard = self
            .session
            .lock()
            .map_err(|_| AppError::Internal("nima session mutex poisoned".into()))?;
        let outputs = guard
            .run(ort::inputs!["input" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run: {e}")))?;

        let tensor = outputs
            .get("output")
            .ok_or_else(|| AppError::Internal("output 'output' not found in nima model".into()))?;
        // try_extract_tensor returns (&Shape, &[T]) in ort rc.12.
        let (_shape, data) = tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal(format!("extract tensor: {e}")))?;
        let probs: Vec<f32> = data.to_vec();

        if probs.len() != NUM_CLASSES {
            return Err(AppError::Internal(format!(
                "expected {NUM_CLASSES}-class output, got {}",
                probs.len()
            )));
        }

        Ok(expected_rating(&probs))
    }
}

// ── scoring math ──────────────────────────────────────────────────────────

/// E[rating] = Σ_{i=1}^{10} i · p_i
fn expected_rating(probs: &[f32]) -> f32 {
    probs
        .iter()
        .enumerate()
        .map(|(i, &p)| (i + 1) as f32 * p)
        .sum()
}

// ── image preprocessing ───────────────────────────────────────────────────

/// Resize to 224×224, convert to RGB f32, ImageNet-normalise, NHWC layout.
fn preprocess_image(path: &Path, sha256: Option<&str>) -> AppResult<Vec<f32>> {
    let img = crate::ai::image_util::open_for_ai(path, sha256)
        .map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    let rgb = img
        .resize_exact(INPUT_SIZE, INPUT_SIZE, FilterType::Lanczos3)
        .into_rgb8();

    // NHWC packing: row-major over (y, x, c) to match the model's expected
    // [1, 224, 224, 3] input shape.
    let mut pixels = Vec::with_capacity(3 * (INPUT_SIZE as usize) * (INPUT_SIZE as usize));
    for y in 0..INPUT_SIZE {
        for x in 0..INPUT_SIZE {
            let px = rgb.get_pixel(x, y);
            for c in 0..3usize {
                let v = (px[c] as f32 / 255.0 - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
                pixels.push(v);
            }
        }
    }
    Ok(pixels)
}

// ── global singleton ──────────────────────────────────────────────────────

static SESSION: OnceLock<NimaSession> = OnceLock::new();

/// Return a reference to the process-wide NIMA session, loading it on first
/// call from `model_path`. Subsequent calls return the cached session.
pub fn get_or_load(model_path: &Path) -> AppResult<&'static NimaSession> {
    if let Some(s) = SESSION.get() {
        return Ok(s);
    }
    let session = NimaSession::load(model_path)?;
    let _ = SESSION.set(session);
    SESSION
        .get()
        .ok_or_else(|| AppError::Internal("nima session init race".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_model_returns_not_found() {
        let path = PathBuf::from("/nonexistent/nima.onnx");
        let err = NimaSession::load(&path).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn expected_rating_uniform_distribution_is_5_5() {
        // Uniform over 10 bins → E[X] = (1+2+…+10)/10 = 5.5
        let probs = [0.1_f32; 10];
        let score = expected_rating(&probs);
        assert!((score - 5.5).abs() < 1e-4, "expected ~5.5, got {score}");
    }

    #[test]
    fn expected_rating_all_mass_on_bin_10_is_10() {
        let mut probs = [0.0_f32; 10];
        probs[9] = 1.0;
        let score = expected_rating(&probs);
        assert!((score - 10.0).abs() < 1e-4, "expected 10.0, got {score}");
    }

    #[test]
    fn expected_rating_all_mass_on_bin_1_is_1() {
        let mut probs = [0.0_f32; 10];
        probs[0] = 1.0;
        let score = expected_rating(&probs);
        assert!((score - 1.0).abs() < 1e-4, "expected 1.0, got {score}");
    }

    #[test]
    fn preprocess_image_wrong_path_returns_io_error() {
        let err = preprocess_image(Path::new("/no/such/file.jpg"), None).unwrap_err();
        assert!(
            matches!(err, AppError::Io(_)),
            "expected Io error, got: {err:?}"
        );
    }

    #[test]
    fn preprocess_image_synthetic_256x256() {
        use image::{ImageBuffer, ImageFormat, Rgb};
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(256, 256, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 64u8])
        });
        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("tempfile");
        img.save_with_format(tmp.path(), ImageFormat::Png)
            .expect("save png");

        let pixels = preprocess_image(tmp.path(), None).expect("preprocess");
        assert_eq!(pixels.len(), 3 * 224 * 224);
        // ImageNet normalisation can push values outside [-1,1], but within ~[-3, 3].
        for v in &pixels {
            assert!(v.is_finite(), "pixel must be finite");
        }
    }
}
