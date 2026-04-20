//! SCRFD-10g face detection + ArcFace W600K R50 embedding via ONNX Runtime.
//!
//! Phase 1 scaffold: sessions are stubbed (return empty / zero results) when
//! model files are absent. Real inference wiring lands in Phase 1b once the
//! model-download flow is complete.
//!
//! Both models ship inside `buffalo_l.zip` from InsightFace (MIT licence) and
//! are extracted by `ai::download` on first run.
//!
//! ## Phase-1b plan
//!
//! 1. SCRFD-10g preprocess: resize image to 640×640, convert to RGB f32
//!    channels-first, then normalise each pixel as `(x - 127.5) / 128.0`.
//!    Input shape `[1, 3, 640, 640]`. (**Different from the old RetinaFace
//!    BGR mean-subtract — do not reuse that normalisation.**)
//! 2. Parse the stride-8 / stride-16 / stride-32 output head triplets
//!    (`score`, `bbox`, `kps`), decode anchors, run NMS.
//! 3. For each surviving box crop + align a 112×112 face chip via the 5-point
//!    landmarks using an affine transform (standard ArcFace alignment matrix).
//! 4. ArcFace preprocess: RGB f32 channels-first, normalise to `[-1, 1]`.
//! 5. L2-normalise the 512-dim output embedding before returning.
//!
//! ## Model files (extracted from buffalo_l.zip on first run)
//!
//! - `scrfd_10g_bnkps.onnx`  — SCRFD-10g with keypoints (InsightFace MIT)
//! - `w600k_r50.onnx`        — ArcFace W600K R50 (InsightFace MIT)
//!
//! Note: the `FacesSession` struct retains the field names `retina` and
//! `arcface` internally to minimise churn — they map to SCRFD and ArcFace
//! W600K respectively.
//!
//! ## Input/output shapes
//!
//! | Model         | Input name | Shape            | dtype | Normalisation              |
//! |---------------|------------|------------------|-------|----------------------------|
//! | SCRFD-10g     | `input.1`  | [1, 3, 640, 640] | f32   | RGB, (x-127.5)/128.0       |
//! | ArcFace W600K | `input.1`  | [1, 3, 112, 112] | f32   | RGB, [-1, 1]               |
//!
//! Output: ArcFace `683` — [1, 512] f32 (L2-normalise before cosine compare).

use crate::{AppError, AppResult};
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

/// ArcFace R50 embedding dimensionality.
pub const FACE_EMBED_DIM: usize = 512;

/// A bounding box + 5-point landmarks produced by RetinaFace.
///
/// Coordinates are in pixel space relative to the original (unscaled) image.
/// Landmarks order: left eye, right eye, nose, left mouth corner, right mouth corner.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    /// 5-point landmarks: \[\[x, y\]; 5\] in original image pixel coordinates.
    /// Order: left-eye, right-eye, nose, left-mouth, right-mouth.
    pub landmarks: [[f32; 2]; 5],
}

/// Loaded RetinaFace + ArcFace ONNX sessions.
///
/// `Session::run` requires `&mut self` in ort rc.12, so each session is wrapped
/// in a `Mutex` so the outer `&FacesSession` (from an `OnceLock`) can drive
/// inference across threads.
///
/// When `is_stub` is `true` neither model was loaded; detect/embed return
/// empty / zero results so callers can be written before model files land.
#[derive(Debug)]
pub struct FacesSession {
    retina: Mutex<Option<Session>>,
    arcface: Mutex<Option<Session>>,
    /// `true` when one or both models are absent (Phase 1 stub).
    pub is_stub: bool,
}

impl FacesSession {
    /// Load both ONNX sessions from disk. Returns `AppError::NotFound` if
    /// either path does not exist.
    pub fn load(retina_path: &Path, arcface_path: &Path) -> AppResult<Self> {
        if !retina_path.exists() {
            return Err(AppError::NotFound(
                "scrfd detector model not found — run model download first".into(),
            ));
        }
        if !arcface_path.exists() {
            return Err(AppError::NotFound(
                "arcface model not found — run model download first".into(),
            ));
        }

        let retina = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder (retina): {e}")))?
            .commit_from_file(retina_path)
            .map_err(|e| AppError::Internal(format!("ort load retina: {e}")))?;

        let arcface = Session::builder()
            .map_err(|e| AppError::Internal(format!("ort builder (arcface): {e}")))?
            .commit_from_file(arcface_path)
            .map_err(|e| AppError::Internal(format!("ort load arcface: {e}")))?;

        Ok(Self {
            retina: Mutex::new(Some(retina)),
            arcface: Mutex::new(Some(arcface)),
            is_stub: false,
        })
    }

    /// Try to load real sessions; fall back to stub if either path is absent.
    ///
    /// Logs at `info` when models are found, `debug` when absent.
    /// The caller does **not** need to handle the absent-model case — stub
    /// methods return safe empty/zero results.
    pub fn load_or_stub(retina_path: Option<&Path>, arcface_path: Option<&Path>) -> Self {
        let retina_exists = retina_path.map(|p| p.exists()).unwrap_or(false);
        let arcface_exists = arcface_path.map(|p| p.exists()).unwrap_or(false);

        if retina_exists && arcface_exists {
            tracing::info!(
                "Face models found (scrfd={:?}, arcface={:?}) — stub only in Phase 1, wiring in Phase 1b",
                retina_path,
                arcface_path,
            );
        } else {
            tracing::debug!(
                "Face models absent (scrfd_found={retina_exists}, arcface_found={arcface_exists}) — using stub"
            );
        }

        // Phase 1 stub: sessions are not loaded even when files exist.
        // Real ort wiring deferred to Phase 1b alongside model-download flow.
        Self {
            retina: Mutex::new(None),
            arcface: Mutex::new(None),
            is_stub: true,
        }
    }

    /// Detect faces in `image_path` and return bounding boxes with landmarks.
    ///
    /// Returns `Ok(vec![])` when running as a stub (model absent).
    /// When the real model is loaded, preprocessing and NMS are applied.
    pub fn detect_faces(&self, image_path: &Path) -> AppResult<Vec<FaceBox>> {
        if self.is_stub {
            let _ = image_path; // suppress unused-variable warning
            return Ok(vec![]);
        }

        // Acquire the RetinaFace session for real inference.
        let _guard = self
            .retina
            .lock()
            .map_err(|_| AppError::Internal("retinaface session mutex poisoned".into()))?;

        // TODO(cc): SCRFD-10g inference — preprocess image to [1,3,640,640]
        // RGB f32 normalised as (x-127.5)/128.0, run session, decode SCRFD
        // anchor boxes across stride-8/16/32 heads, apply NMS, map back to
        // original image coordinates. Tracked in PRD §5 item 2 (phase-1b).
        Err(AppError::Internal(
            "scrfd detector inference not yet wired (phase-1b)".into(),
        ))
    }

    /// Crop and embed a face chip defined by `bbox` from `image_path`.
    ///
    /// Returns `Ok(vec![0.0; FACE_EMBED_DIM])` when running as a stub.
    /// When the real model is loaded, the crop is aligned via the 5-point
    /// landmarks and passed through ArcFace; the output is L2-normalised.
    pub fn embed_face(&self, image_path: &Path, bbox: &FaceBox) -> AppResult<Vec<f32>> {
        if self.is_stub {
            let _ = image_path; // suppress unused-variable warning
            let _ = bbox;
            return Ok(vec![0.0_f32; FACE_EMBED_DIM]);
        }

        // Acquire the ArcFace session for real inference.
        let _guard = self
            .arcface
            .lock()
            .map_err(|_| AppError::Internal("arcface session mutex poisoned".into()))?;

        // TODO(cc): arcface inference — align face chip to 112×112 using
        // `bbox.landmarks` (standard ArcFace alignment matrix), run ArcFace
        // session, then call l2_normalise on the 512-dim output. Tracked in
        // PRD §5 item 2 (phase-1b).
        Err(AppError::Internal(
            "arcface inference not yet wired (phase-1b)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stub_returns_empty_detections() {
        let sess = FacesSession::load_or_stub(None, None);
        assert!(sess.is_stub, "load_or_stub(None, None) must be a stub");
        let faces = sess
            .detect_faces(Path::new("/nonexistent/photo.jpg"))
            .expect("stub detect_faces should not error");
        assert!(
            faces.is_empty(),
            "stub detect_faces must return empty vec, got {faces:?}"
        );
    }

    #[test]
    fn stub_returns_zero_embedding() {
        let sess = FacesSession::load_or_stub(None, None);
        let bbox = FaceBox {
            x: 0.0,
            y: 0.0,
            w: 64.0,
            h: 64.0,
            score: 0.99,
            landmarks: [
                [10.0, 20.0],
                [50.0, 20.0],
                [30.0, 40.0],
                [15.0, 55.0],
                [45.0, 55.0],
            ],
        };
        let emb = sess
            .embed_face(Path::new("/nonexistent/photo.jpg"), &bbox)
            .expect("stub embed_face should not error");
        assert_eq!(
            emb.len(),
            FACE_EMBED_DIM,
            "stub embedding must be {FACE_EMBED_DIM}-dim"
        );
        assert!(
            emb.iter().all(|&v| v == 0.0),
            "stub embedding must be all zeros"
        );
    }

    #[test]
    fn missing_model_returns_not_found() {
        let scrfd = PathBuf::from("/nonexistent/scrfd_10g_bnkps.onnx");
        let arcface = PathBuf::from("/nonexistent/w600k_r50.onnx");
        let err = FacesSession::load(&scrfd, &arcface).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn face_embed_dim_constant_matches_arcface_w600k_r50_spec() {
        // ArcFace W600K R50 always produces 512-dimensional embeddings.
        assert_eq!(FACE_EMBED_DIM, 512);
    }
}
