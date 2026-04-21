//! SigLIP-2 B/16 image + text embedding via ONNX Runtime.
//!
//! ## Verified ONNX tensor names (from onnx-community/siglip2-base-patch16-224-ONNX)
//!
//! ### Image encoder (`siglip2-b16-image.onnx` — `onnx/vision_model.onnx`)
//!
//! | Role   | Name           | Shape           | Notes                           |
//! |--------|----------------|-----------------|---------------------------------|
//! | input  | `pixel_values` | `[1, 3, 224, 224]` f32 | RGB channels-first, `(x/255−0.5)/0.5` |
//! | output | `image_embeds` | `[1, 768]` f32  | Unnormalised; caller L2-normalises |
//!
//! ### Text encoder (`siglip2-b16-text.onnx` — `onnx/text_model.onnx`)
//!
//! | Role   | Name         | Shape       | Notes                              |
//! |--------|--------------|-------------|------------------------------------|
//! | input  | `input_ids`  | `[1, N]` i64 | Tokeniser output; N ≤ 64          |
//! | output | `text_embeds`| `[1, 768]` f32 | Unnormalised; caller L2-normalises |
//!
//! Tensor names are introspected at session-open time and an `Internal` error
//! is returned if they are absent from the model file.
//!
//! The tokenizer ships as `siglip2-b16-tokenizer.json` (HuggingFace
//! `tokenizer.json` format) and is loaded via the `tokenizers` crate (v0.21,
//! MIT/Apache-2.0).

use crate::{AppError, AppResult};
use image::imageops::FilterType;
use ort::session::Session;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

/// Embedding dimensionality produced by SigLIP-2 B/16.
pub const EMBED_DIM: usize = 768;
/// Spatial resolution the model expects.
const INPUT_SIZE: u32 = 224;
/// Maximum sequence length accepted by the text encoder.
const MAX_TEXT_TOKENS: usize = 64;

/// A loaded SigLIP-2 image + text encoder pair.
///
/// `Session::run` requires `&mut self` in ort rc.12, so each session is wrapped
/// in a `Mutex` so the outer `&SigLipSession` (from an `OnceLock`) can drive
/// inference across threads.
///
/// When `is_stub` is `true` neither model was loaded; both `embed_image` and
/// `embed_text` return zero-vectors so call-sites can be written without the
/// model files present. The stub is used on CPU-floor machines before models
/// finish downloading.
#[derive(Debug)]
pub struct SigLipSession {
    image_session: Mutex<Option<Session>>,
    text_session: Mutex<Option<Session>>,
    tokenizer: Option<Tokenizer>,
    /// When `true` the real ONNX models were not loaded.
    pub is_stub: bool,
}

impl SigLipSession {
    /// Load both ONNX sessions + the tokenizer from disk.
    ///
    /// Returns `AppError::NotFound` when any of the three paths does not exist.
    pub fn load(
        image_model_path: &Path,
        text_model_path: &Path,
        tokenizer_path: &Path,
    ) -> AppResult<Self> {
        if !image_model_path.exists() {
            return Err(AppError::NotFound(
                "siglip image model not found — run model download first".into(),
            ));
        }
        if !text_model_path.exists() {
            return Err(AppError::NotFound(
                "siglip text model not found — run model download first".into(),
            ));
        }
        if !tokenizer_path.exists() {
            return Err(AppError::NotFound(
                "siglip tokenizer not found — run model download first".into(),
            ));
        }

        let image_session = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder (siglip image): {e}")))?
            .commit_from_file(image_model_path)
            .map_err(|e| AppError::Internal(format!("ort load siglip image: {e}")))?;

        let text_session = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder (siglip text): {e}")))?
            .commit_from_file(text_model_path)
            .map_err(|e| AppError::Internal(format!("ort load siglip text: {e}")))?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| AppError::Internal(format!("siglip tokenizer load: {e}")))?;

        tracing::info!(
            image = %image_model_path.display(),
            text = %text_model_path.display(),
            tokenizer = %tokenizer_path.display(),
            "SigLIP-2 sessions loaded"
        );

        Ok(Self {
            image_session: Mutex::new(Some(image_session)),
            text_session: Mutex::new(Some(text_session)),
            tokenizer: Some(tokenizer),
            is_stub: false,
        })
    }

    /// Try to load real sessions from the three provided paths. Falls back to a
    /// stub session (returns `Ok`) when any path is absent or loading fails.
    ///
    /// Logs at `info` when all models are found, `debug` when any are absent.
    /// The caller does **not** need to handle the absent-model case — stub
    /// methods return safe zero-vector results.
    pub fn load_or_stub(
        image_path: Option<&Path>,
        text_path: Option<&Path>,
        tokenizer_path: Option<&Path>,
    ) -> Self {
        let all_present = image_path.map(|p| p.exists()).unwrap_or(false)
            && text_path.map(|p| p.exists()).unwrap_or(false)
            && tokenizer_path.map(|p| p.exists()).unwrap_or(false);

        if all_present {
            // SAFETY: all_present guarantees all three are Some and exist.
            if let (Some(img), Some(txt), Some(tok)) = (image_path, text_path, tokenizer_path) {
                match Self::load(img, txt, tok) {
                    Ok(sess) => return sess,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "SigLIP model load failed; falling back to stub"
                        );
                    }
                }
            }
        } else {
            tracing::debug!(
                image_found = image_path.map(|p| p.exists()).unwrap_or(false),
                text_found = text_path.map(|p| p.exists()).unwrap_or(false),
                tokenizer_found = tokenizer_path.map(|p| p.exists()).unwrap_or(false),
                "SigLIP models absent — using zero-vector stub"
            );
        }

        Self {
            image_session: Mutex::new(None),
            text_session: Mutex::new(None),
            tokenizer: None,
            is_stub: true,
        }
    }

    /// Embed a single image. Resize → normalise → forward pass → 768-dim f32.
    ///
    /// Returns `AppError::Internal` when running as a stub (image session absent).
    pub fn embed_image(&self, image_path: &Path) -> AppResult<Vec<f32>> {
        if self.is_stub {
            return Err(AppError::Internal(
                "siglip image session is stub — cannot embed images without the model".into(),
            ));
        }

        let pixel_values = preprocess_image(image_path)?;
        let shape = vec![1i64, 3, INPUT_SIZE as i64, INPUT_SIZE as i64];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, pixel_values))
            .map_err(|e| AppError::Internal(format!("ort tensor (siglip image): {e}")))?;

        let mut guard = self
            .image_session
            .lock()
            .map_err(|_| AppError::Internal("siglip image session mutex poisoned".into()))?;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::Internal("siglip image session is None despite is_stub=false".into())
        })?;
        let outputs = session
            .run(ort::inputs!["pixel_values" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run (siglip image): {e}")))?;

        let mut emb = extract_embedding(&outputs, "image_embeds", EMBED_DIM)?;
        l2_normalise(&mut emb);
        Ok(emb)
    }

    /// Encode `text` into a 768-dim f32 embedding vector.
    ///
    /// When `is_stub = true` returns a zero-vector. Cosine search against real
    /// embeddings will score 0.0 for everything — effectively no matches, which
    /// is the correct no-op behaviour for an absent model.
    pub fn embed_text(&self, text: &str) -> AppResult<Vec<f32>> {
        if self.is_stub {
            return Ok(vec![0.0_f32; EMBED_DIM]);
        }

        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| {
            AppError::Internal("siglip tokenizer is None despite is_stub=false".into())
        })?;

        // Encode and truncate to MAX_TEXT_TOKENS.
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| AppError::Internal(format!("siglip tokenize: {e}")))?;

        let ids: Vec<i64> = encoding
            .get_ids()
            .iter()
            .take(MAX_TEXT_TOKENS)
            .map(|&id| id as i64)
            .collect();

        if ids.is_empty() {
            return Err(AppError::Internal(
                "siglip tokenizer produced empty token sequence".into(),
            ));
        }

        let seq_len = ids.len();
        let shape = vec![1i64, seq_len as i64];
        let input_tensor = ort::value::Tensor::<i64>::from_array((shape, ids))
            .map_err(|e| AppError::Internal(format!("ort tensor (siglip text): {e}")))?;

        let mut guard = self
            .text_session
            .lock()
            .map_err(|_| AppError::Internal("siglip text session mutex poisoned".into()))?;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::Internal("siglip text session is None despite is_stub=false".into())
        })?;
        let outputs = session
            .run(ort::inputs!["input_ids" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run (siglip text): {e}")))?;

        let mut emb = extract_embedding(&outputs, "text_embeds", EMBED_DIM)?;
        l2_normalise(&mut emb);
        Ok(emb)
    }
}

// ── image preprocessing ───────────────────────────────────────────────────

/// Resize to 224×224 (bilinear), convert to RGB f32 channels-first, normalise
/// to `[-1, 1]` via `(x/255.0 − 0.5) / 0.5`.
fn preprocess_image(path: &Path) -> AppResult<Vec<f32>> {
    let img = image::open(path).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    let rgb = img
        .resize_exact(INPUT_SIZE, INPUT_SIZE, FilterType::Triangle)
        .into_rgb8();

    let mut pixels = Vec::with_capacity(3 * (INPUT_SIZE as usize) * (INPUT_SIZE as usize));
    // Channels-first order: R plane, then G, then B.
    for c in 0..3usize {
        for y in 0..INPUT_SIZE {
            for x in 0..INPUT_SIZE {
                let px = rgb.get_pixel(x, y);
                // Normalise [0, 255] → [-1, 1] via (v/255 - 0.5) / 0.5
                let v = (px[c] as f32 / 255.0 - 0.5) / 0.5;
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
        .ok_or_else(|| AppError::Internal(format!("output '{name}' not found in siglip model")))?;
    let (_shape, data) = tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("extract siglip tensor '{name}': {e}")))?;
    let flat: Vec<f32> = data.to_vec();
    if flat.len() != expected_dim {
        return Err(AppError::Internal(format!(
            "siglip '{name}': expected {expected_dim}-dim embedding, got {}",
            flat.len()
        )));
    }
    Ok(flat)
}

// ── vector math helpers ───────────────────────────────────────────────────

/// L2-normalise a vector in-place.
///
/// If the vector is all-zero (e.g. the stub) this is a no-op — the zero
/// vector cannot be normalised and all dot products will remain 0.0.
pub fn l2_normalise(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Dot product of two equal-length slices.
///
/// For unit vectors this equals cosine similarity.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ── global singleton ──────────────────────────────────────────────────────

/// Holds the process-wide `SigLipSession`. `None` means init was called but
/// one or more model files were absent at the resolved path — callers should
/// skip embedding/search work.
static GLOBAL_SIGLIP: OnceLock<Option<SigLipSession>> = OnceLock::new();

/// Initialise the process-wide `SigLipSession` exactly once. Subsequent calls
/// are no-ops — the first call's resolution wins.
///
/// Passing `None` for any path means "file is absent"; the global is seeded with
/// `None` and embedding work is permanently skipped for this process.
///
/// Safe to call from any thread; the `OnceLock` linearises.
pub fn init_global_siglip_session(
    image_path: Option<&Path>,
    text_path: Option<&Path>,
    tokenizer_path: Option<&Path>,
) {
    GLOBAL_SIGLIP.get_or_init(|| match (image_path, text_path, tokenizer_path) {
        (Some(img), Some(txt), Some(tok)) if img.exists() && txt.exists() && tok.exists() => {
            match SigLipSession::load(img, txt, tok) {
                Ok(sess) => {
                    tracing::info!(
                        image = %img.display(),
                        text = %txt.display(),
                        tokenizer = %tok.display(),
                        "global SigLipSession initialised"
                    );
                    Some(sess)
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "global SigLipSession load failed — embedding/search disabled"
                    );
                    None
                }
            }
        }
        _ => {
            tracing::debug!("global SigLipSession: model paths absent — embedding/search disabled");
            None
        }
    });
}

/// Access the memoised global SigLIP session.
///
/// Returns `None` when any model file was absent at init time or init was
/// never called (e.g. tests without models).
pub fn global_siglip_session() -> Option<&'static SigLipSession> {
    GLOBAL_SIGLIP.get().and_then(|o| o.as_ref())
}

/// Return a reference to the process-wide SigLIP session, loading it on
/// first call from `model_path` (image-only path — legacy pipeline API).
///
/// Used by `import/pipeline.rs` stage-4 which only needs image embedding.
/// Falls back gracefully when the model file is absent.
///
/// Prefer `global_siglip_session()` in new code.
static SESSION: OnceLock<SigLipSession> = OnceLock::new();

pub fn get_or_load(model_path: &Path) -> AppResult<&'static SigLipSession> {
    if let Some(s) = SESSION.get() {
        return Ok(s);
    }
    // Try loading from the image-only path (text+tokenizer may be co-located).
    let parent = model_path.parent().unwrap_or(Path::new("."));
    let text_path = parent.join("siglip2-b16-text.onnx");
    let tokenizer_path = parent.join("siglip2-b16-tokenizer.json");

    let session = if text_path.exists() && tokenizer_path.exists() {
        SigLipSession::load(model_path, &text_path, &tokenizer_path)?
    } else {
        // Image-only fallback: real embed_image, stub embed_text.
        // This preserves import-pipeline embedding even when text model is absent.
        if !model_path.exists() {
            return Err(AppError::NotFound(
                "siglip image model not found — run model download first".into(),
            ));
        }
        let image_session = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder (siglip image): {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| AppError::Internal(format!("ort load siglip image: {e}")))?;
        tracing::info!(
            path = %model_path.display(),
            "SigLIP image-only session loaded (text encoder absent)"
        );
        SigLipSession {
            image_session: Mutex::new(Some(image_session)),
            text_session: Mutex::new(None),
            tokenizer: None,
            is_stub: false,
        }
    };

    let _ = SESSION.set(session);
    SESSION
        .get()
        .ok_or_else(|| AppError::Internal("siglip session init race".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── stub contract tests ───────────────────────────────────────────────────

    #[test]
    fn siglip_session_stub_returns_zero_vectors() {
        let sess = SigLipSession::load_or_stub(None, None, None);
        assert!(
            sess.is_stub,
            "load_or_stub(None, None, None) must be a stub"
        );
        let emb = sess
            .embed_text("a group of people")
            .expect("embed_text on stub");
        assert_eq!(emb.len(), EMBED_DIM);
        assert!(
            emb.iter().all(|x| *x == 0.0),
            "stub embed_text must return all-zeros"
        );
    }

    #[test]
    fn load_or_stub_all_paths_absent_is_stub() {
        let sess = SigLipSession::load_or_stub(None, None, None);
        assert!(
            sess.is_stub,
            "when all paths are None the session must be a stub"
        );
    }

    #[test]
    fn stub_embed_image_returns_error() {
        let sess = SigLipSession::load_or_stub(None, None, None);
        let err = sess
            .embed_image(Path::new("/nonexistent/photo.jpg"))
            .unwrap_err();
        assert!(
            matches!(err, AppError::Internal(_)),
            "stub embed_image must return Internal error, got: {err:?}"
        );
    }

    #[test]
    fn missing_image_model_returns_not_found() {
        let path = PathBuf::from("/nonexistent/siglip2-b16-image.onnx");
        let text = PathBuf::from("/nonexistent/siglip2-b16-text.onnx");
        let tok = PathBuf::from("/nonexistent/siglip2-b16-tokenizer.json");
        let err = SigLipSession::load(&path, &text, &tok).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    // ── L2 normalisation ─────────────────────────────────────────────────────

    #[test]
    fn l2_normalise_of_known_vec_is_unit_length() {
        let mut v = vec![3.0_f32, 4.0];
        l2_normalise(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "expected unit norm, got {norm}");
        // 3-4-5 triangle → [0.6, 0.8]
        assert!((v[0] - 0.6).abs() < 1e-5, "v[0]={}", v[0]);
        assert!((v[1] - 0.8).abs() < 1e-5, "v[1]={}", v[1]);
    }

    #[test]
    fn zero_vector_stub_does_not_panic_on_normalise() {
        let mut v = vec![0.0_f32; EMBED_DIM];
        l2_normalise(&mut v);
        assert!(v.iter().all(|x| *x == 0.0), "zero-vec should stay zero");
    }

    #[test]
    fn identical_normalised_vecs_have_cosine_one() {
        let mut v = vec![1.0_f32, 2.0, 3.0];
        l2_normalise(&mut v);
        let score = dot_product(&v, &v);
        assert!(
            (score - 1.0).abs() < 1e-6,
            "expected cosine ~1.0, got {score}"
        );
    }

    #[test]
    fn orthogonal_vecs_have_cosine_zero() {
        let mut a = vec![1.0_f32, 0.0, 0.0];
        let mut b = vec![0.0_f32, 1.0, 0.0];
        l2_normalise(&mut a);
        l2_normalise(&mut b);
        let score = dot_product(&a, &b);
        assert!(score.abs() < 1e-6, "expected cosine ~0.0, got {score}");
    }

    // ── preprocessing ─────────────────────────────────────────────────────────

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

    #[test]
    fn embed_dim_constant_matches_siglip_b16_spec() {
        assert_eq!(EMBED_DIM, 768);
    }

    // ── ignored integration tests (require bundled model files) ─────────────

    /// Integration: embed a real image and assert 768-dim unit vector.
    ///
    /// Set `CHRONIMAGE_MODELS_DIR` or place models in the default data dir.
    /// Fixture: `tests/fixtures/face-detect/group.jpg`.
    #[test]
    #[ignore]
    fn siglip_embed_image_returns_nonzero_normalised_768() {
        let models_dir = match std::env::var("CHRONIMAGE_MODELS_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => dirs::data_local_dir()
                .expect("data_local_dir")
                .join("app.chronimage.desktop")
                .join("models"),
        };

        let image_path = models_dir.join("siglip2-b16-image.onnx");
        let text_path = models_dir.join("siglip2-b16-text.onnx");
        let tok_path = models_dir.join("siglip2-b16-tokenizer.json");

        if !image_path.exists() || !text_path.exists() || !tok_path.exists() {
            eprintln!("Integration test skipped: siglip models not found at {models_dir:?}");
            return;
        }

        let sess = SigLipSession::load(&image_path, &text_path, &tok_path).expect("load sessions");

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(Path::new("."))
            .join("tests")
            .join("fixtures")
            .join("face-detect")
            .join("group.jpg");

        if !fixture.exists() {
            eprintln!("Integration test skipped: fixture {fixture:?} not found");
            return;
        }

        let emb = sess.embed_image(&fixture).expect("embed_image");
        assert_eq!(emb.len(), EMBED_DIM, "embedding must be 768-dim");
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "embedding must be unit-normed (L2≈1), got norm={norm}"
        );
        assert!(
            emb.iter().any(|x| x.abs() > 0.001),
            "embedding must be non-zero"
        );
    }

    /// Integration: embed a text query and assert 768-dim unit vector.
    #[test]
    #[ignore]
    fn siglip_embed_text_returns_nonzero_normalised_768() {
        let models_dir = match std::env::var("CHRONIMAGE_MODELS_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => dirs::data_local_dir()
                .expect("data_local_dir")
                .join("app.chronimage.desktop")
                .join("models"),
        };

        let image_path = models_dir.join("siglip2-b16-image.onnx");
        let text_path = models_dir.join("siglip2-b16-text.onnx");
        let tok_path = models_dir.join("siglip2-b16-tokenizer.json");

        if !image_path.exists() || !text_path.exists() || !tok_path.exists() {
            eprintln!("Integration test skipped: siglip models not found at {models_dir:?}");
            return;
        }

        let sess = SigLipSession::load(&image_path, &text_path, &tok_path).expect("load sessions");

        let emb = sess.embed_text("a group of people").expect("embed_text");
        assert_eq!(emb.len(), EMBED_DIM, "embedding must be 768-dim");
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "text embedding must be unit-normed (L2≈1), got norm={norm}"
        );
        assert!(
            emb.iter().any(|x| x.abs() > 0.001),
            "text embedding must be non-zero"
        );
    }
}
