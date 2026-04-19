//! SigLIP-B text encoder stub.
//!
//! In Phase 1 the ONNX model is not yet bundled; `embed_text` returns a
//! deterministic zero-vector stub so that the search command compiles and the
//! vector-math path is fully exercised by tests. Phase 1b will swap this for a
//! real `ort::Session` call.
//!
//! EMBED_DIM = 768 matches SigLIP-B/16's text embedding dimension.

use crate::AppResult;

/// Number of dimensions in a SigLIP-B text / image embedding.
pub const EMBED_DIM: usize = 768;

/// A loaded (or stubbed) SigLIP session.
///
/// This type is `Send + Sync` so it can be stored inside `AppState`.
pub struct SigLipSession {
    /// When `true` the real ONNX model is loaded. When `false` we return stubs.
    pub is_stub: bool,
}

impl SigLipSession {
    /// Try to load a real SigLIP model from `model_path`. Falls back to the
    /// stub session (returns `Ok`) if the path doesn't exist or the ort runtime
    /// isn't available yet.
    pub fn load_or_stub(model_path: Option<&std::path::Path>) -> Self {
        // Phase 1 stub: the real ort::Session init comes in Phase 1b once the
        // model-download flow lands. For now we always return the stub.
        let path_exists = model_path.map(|p| p.exists()).unwrap_or(false);

        if path_exists {
            tracing::info!(
                "SigLIP model found at {:?} (stub load — ort not wired yet)",
                model_path
            );
        } else {
            tracing::debug!("SigLIP model absent — using zero-vector stub for text queries");
        }

        Self { is_stub: true }
    }

    /// Encode `text` into a 768-dim f32 vector.
    ///
    /// Returns a zero-vector stub until the real ONNX runtime is wired in
    /// Phase 1b. The caller must L2-normalise before computing cosine scores.
    pub fn embed_text(&self, text: &str) -> AppResult<Vec<f32>> {
        if !self.is_stub {
            // Placeholder for real ort inference — will be filled in Phase 1b.
            return Err(crate::AppError::Internal(
                "real SigLIP inference not yet implemented".into(),
            ));
        }

        // Stub: return a zero-vector. Cosine search against real embeddings
        // will score 0.0 for everything (no matches), which is the correct
        // no-op behaviour for an absent model.
        let _ = text; // suppress unused-variable warning
        Ok(vec![0.0_f32; EMBED_DIM])
    }
}

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
/// For unit vectors this equals cosine similarity. Panics in debug if lengths
/// differ; in release it silently truncates to the shorter slice.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_normalised_vecs_have_cosine_one() {
        let mut v = vec![1.0_f32, 2.0, 3.0];
        l2_normalise(&mut v);
        let mut u = v.clone();
        l2_normalise(&mut u);
        let score = dot_product(&v, &u);
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

    #[test]
    fn zero_vector_stub_does_not_panic_on_normalise() {
        let mut v = vec![0.0_f32; EMBED_DIM];
        l2_normalise(&mut v); // must be a no-op, not NaN or panic
        assert!(v.iter().all(|x| *x == 0.0), "zero-vec should stay zero");
    }

    #[test]
    fn stub_session_returns_zero_vector() {
        let sess = SigLipSession::load_or_stub(None);
        let emb = sess.embed_text("a photo of a dog").expect("embed_text");
        assert_eq!(emb.len(), EMBED_DIM);
        assert!(emb.iter().all(|x| *x == 0.0));
    }
}
