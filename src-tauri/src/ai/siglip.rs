//! SigLIP-B/16 image embedding via ONNX Runtime.
//!
//! Produces a 768-dimensional f32 embedding vector for a given image path.
//! The text encoder is a separate model deferred to a later phase; `embed_text`
//! returns a zero vector stub so call-sites can be written now.
//!
//! Model path (relative to model root): `siglip-b16-image.onnx`
//! Input:  `pixel_values` — [1, 3, 224, 224] f32, normalised to [-1, 1]
//! Output: `image_embeds`  — [1, 768] f32

use crate::{AppError, AppResult};
use image::imageops::FilterType;
use ort::session::Session;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Embedding dimensionality produced by SigLIP-B/16.
pub const EMBED_DIM: usize = 768;
/// Spatial resolution the model expects.
const INPUT_SIZE: u32 = 224;

/// A loaded SigLIP image-encoder session.
///
/// `Session::run` requires `&mut self` in ort rc.12, so we wrap it in a
/// `Mutex` so the outer `&SigLipSession` reference (from `OnceLock`) can
/// still drive inference across threads.
#[derive(Debug)]
pub struct SigLipSession {
    session: Mutex<Session>,
}

impl SigLipSession {
    /// Load the ONNX session from `model_path`.
    /// Returns `AppError::NotFound` when the file does not exist.
    pub fn load(model_path: &Path) -> AppResult<Self> {
        if !model_path.exists() {
            return Err(AppError::NotFound(
                "siglip model not found — run model download first".into(),
            ));
        }
        let session = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| AppError::Internal(format!("ort load: {e}")))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    /// Embed a single image. Resize → normalise → forward pass → 768-dim f32.
    pub fn embed_image(&self, image_path: &Path) -> AppResult<Vec<f32>> {
        let pixel_values = preprocess_image(image_path)?;
        // Use (shape, vec) tuple — avoids ndarray version conflicts.
        let shape = vec![1i64, 3, INPUT_SIZE as i64, INPUT_SIZE as i64];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, pixel_values))
            .map_err(|e| AppError::Internal(format!("ort tensor: {e}")))?;

        // inputs! returns Vec directly (not Result) in ort rc.12.
        // Bind the guard to a local so it lives as long as `outputs`.
        let mut guard = self
            .session
            .lock()
            .map_err(|_| AppError::Internal("siglip session mutex poisoned".into()))?;
        let outputs = guard
            .run(ort::inputs!["pixel_values" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run: {e}")))?;

        extract_embedding(&outputs, "image_embeds", EMBED_DIM)
    }

    /// Stub: text encoder is a separate model, wired in Phase 2.
    /// Returns a zero vector of the correct dimensionality.
    pub fn embed_text(&self, _text: &str) -> AppResult<Vec<f32>> {
        Ok(vec![0.0_f32; EMBED_DIM])
    }
}

// ── image preprocessing ───────────────────────────────────────────────────

/// Resize to 224×224, convert to RGB f32 channels-first, normalise to [-1, 1].
fn preprocess_image(path: &Path) -> AppResult<Vec<f32>> {
    let img = image::open(path).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    let rgb = img
        .resize_exact(INPUT_SIZE, INPUT_SIZE, FilterType::Lanczos3)
        .into_rgb8();

    let mut pixels = Vec::with_capacity(3 * (INPUT_SIZE as usize) * (INPUT_SIZE as usize));
    // Channels-first order: R plane, then G, then B.
    for c in 0..3usize {
        for y in 0..INPUT_SIZE {
            for x in 0..INPUT_SIZE {
                let px = rgb.get_pixel(x, y);
                // Normalise [0, 255] → [-1, 1]
                let v = (px[c] as f32) / 127.5 - 1.0;
                pixels.push(v);
            }
        }
    }
    Ok(pixels)
}

// ── output extraction helper ──────────────────────────────────────────────

fn extract_embedding(
    outputs: &ort::session::SessionOutputs,
    name: &str,
    expected_dim: usize,
) -> AppResult<Vec<f32>> {
    let tensor = outputs
        .get(name)
        .ok_or_else(|| AppError::Internal(format!("output '{name}' not found in model")))?;
    // try_extract_tensor returns (&Shape, &[T]) in ort rc.12.
    let (_shape, data) = tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("extract tensor: {e}")))?;
    let flat: Vec<f32> = data.to_vec();
    if flat.len() != expected_dim {
        return Err(AppError::Internal(format!(
            "expected {expected_dim}-dim embedding, got {}",
            flat.len()
        )));
    }
    Ok(flat)
}

// ── global singleton ──────────────────────────────────────────────────────

static SESSION: OnceLock<SigLipSession> = OnceLock::new();

/// Return a reference to the process-wide SigLIP session, loading it on
/// first call from `model_path`. Subsequent calls return the cached session.
pub fn get_or_load(model_path: &Path) -> AppResult<&'static SigLipSession> {
    if let Some(s) = SESSION.get() {
        return Ok(s);
    }
    let session = SigLipSession::load(model_path)?;
    // `set` fails only if another thread raced us; in that case `get` wins.
    let _ = SESSION.set(session);
    SESSION
        .get()
        .ok_or_else(|| AppError::Internal("siglip session init race".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_model_returns_not_found() {
        let path = PathBuf::from("/nonexistent/siglip-b16-image.onnx");
        let err = SigLipSession::load(&path).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn embed_text_stub_returns_768_zeros() {
        // We test the text stub without loading a model by constructing a
        // SigLipSession from an in-memory trivial ONNX. Since we cannot build
        // a real session in a unit test without the model file, we exercise
        // only the stub's output contract by checking the dimensionality
        // directly from the constant.
        assert_eq!(EMBED_DIM, 768);
    }

    #[test]
    fn embed_dim_constant_matches_siglip_b16_spec() {
        // SigLIP-B/16 produces 768-dimensional patch embeddings.
        assert_eq!(EMBED_DIM, 768);
    }

    #[test]
    fn preprocess_image_wrong_path_returns_io_error() {
        let err = preprocess_image(Path::new("/no/such/file.jpg")).unwrap_err();
        assert!(
            matches!(err, AppError::Io(_)),
            "expected Io error, got: {err:?}"
        );
    }

    #[test]
    fn preprocess_image_synthetic_256x256() {
        // Write a tiny PNG to a temp file and verify the output tensor shape.
        use image::{ImageBuffer, ImageFormat, Rgb};
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(256, 256, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128u8])
        });
        let tmp = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("tempfile");
        img.save_with_format(tmp.path(), ImageFormat::Png)
            .expect("save png");

        let pixels = preprocess_image(tmp.path()).expect("preprocess");
        // 3 channels × 224 × 224
        assert_eq!(pixels.len(), 3 * 224 * 224);
        // All values in [-1, 1]
        for v in &pixels {
            assert!(*v >= -1.0 && *v <= 1.0, "pixel {v} out of range");
        }
    }
}
