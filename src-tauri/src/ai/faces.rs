//! SCRFD-10g face detection + ArcFace W600K R50 embedding via ONNX Runtime.
//!
//! ## Verified ONNX tensor names (inspected 2026-04-20 against locally downloaded models)
//!
//! ### SCRFD-10g (`det_10g.onnx`)
//!
//! | Role    | Name      | Shape         | Notes                              |
//! |---------|-----------|---------------|------------------------------------|
//! | input   | `input.1` | [1, 3, H, W]  | dynamic H/W; we use 640×640        |
//! | score-8 | `448`     | [12800, 1]    | stride-8, 80×80 grid × 2 anchors  |
//! | score-16| `471`     | [3200, 1]     | stride-16, 40×40 grid × 2 anchors |
//! | score-32| `494`     | [800, 1]      | stride-32, 20×20 grid × 2 anchors |
//! | bbox-8  | `451`     | [12800, 4]    | ltrb distances, stride-8           |
//! | bbox-16 | `474`     | [3200, 4]     | ltrb distances, stride-16          |
//! | bbox-32 | `497`     | [800, 4]      | ltrb distances, stride-32          |
//! | kps-8   | `454`     | [12800, 10]   | 5 landmarks × 2 coords, stride-8   |
//! | kps-16  | `477`     | [3200, 10]    | 5 landmarks × 2 coords, stride-16  |
//! | kps-32  | `500`     | [800, 10]     | 5 landmarks × 2 coords, stride-32  |
//!
//! ### ArcFace W600K R50 (`w600k_r50.onnx`)
//!
//! | Role   | Name      | Shape        | Notes                              |
//! |--------|-----------|--------------|-------------------------------------|
//! | input  | `input.1` | [N, 3, 112, 112] | batch dynamic                  |
//! | output | `683`     | [1, 512]     | 512-dim embedding (L2-normalise)   |
//!
//! ## SCRFD decode summary
//!
//! Each stride `s ∈ {8, 16, 32}` produces a flat `[N, 4]` bbox tensor and
//! `[N, 10]` landmark tensor where `N = (640/s)² × 2`. Rows correspond to
//! anchors in raster order (row-major, two anchors per cell). Decode:
//!
//! ```text
//! cx = (col + 0.5) * s,  cy = (row + 0.5) * s
//! x1 = cx - bbox[0]*s,   y1 = cy - bbox[1]*s
//! x2 = cx + bbox[2]*s,   y2 = cy + bbox[3]*s
//! kp_x = cx + kps[2k]*s, kp_y = cy + kps[2k+1]*s   for k in 0..5
//! ```
//!
//! ## ArcFace alignment
//!
//! The 5-point affine align uses a manual 3-point least-squares solve (no
//! nalgebra/ndarray available in the dependency set). We pick the left-eye,
//! right-eye, and nose correspondence to solve for the 6 affine parameters
//! `[a, b, tx, c, d, ty]` that map landmark coords → the standard 112×112
//! target points. A bilinear warp then fills the chip.
//!
//! Both models ship inside `buffalo_l.zip` from InsightFace (MIT licence) and
//! are extracted by `ai::download` on first run.

use crate::{ai::providers::session_builder_with_ep, AppError, AppResult};
use image::imageops::FilterType;
use ort::session::Session;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// ArcFace R50 embedding dimensionality.
pub const FACE_EMBED_DIM: usize = 512;

/// Input resolution for SCRFD-10g.
const SCRFD_SIZE: u32 = 640;
/// Padding value used for letterboxing (gray).
const LETTERBOX_GRAY: f32 = 0.0; // (128 - 127.5) / 128.0

/// ArcFace input chip size.
const ARCFACE_SIZE: u32 = 112;

/// Standard 5-point ArcFace destination landmarks for a 112×112 chip.
/// Order: left-eye, right-eye, nose, left-mouth, right-mouth.
const ARCFACE_DST: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

// ── public parameter types ────────────────────────────────────────────────

/// Tuneable detection parameters.
#[derive(Debug, Clone)]
pub struct DetectParams {
    /// Confidence threshold (pre-NMS). Default: 0.5.
    pub detect_threshold: f32,
    /// IoU threshold for NMS. Default: 0.45.
    pub nms_iou: f32,
}

impl Default for DetectParams {
    fn default() -> Self {
        Self {
            detect_threshold: 0.5,
            nms_iou: 0.45,
        }
    }
}

// ── FaceBox ───────────────────────────────────────────────────────────────

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

// ── FacesSession ─────────────────────────────────────────────────────────

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

        let retina = session_builder_with_ep("scrfd")
            .map_err(|e| AppError::Internal(format!("ort builder (retina): {e}")))?
            .commit_from_file(retina_path)
            .map_err(|e| AppError::Internal(format!("ort load retina: {e}")))?;

        let arcface = session_builder_with_ep("arcface")
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
            // Attempt a real load; on failure fall through to stub with a warning.
            if let (Some(rp), Some(ap)) = (retina_path, arcface_path) {
                match Self::load(rp, ap) {
                    Ok(sess) => {
                        tracing::info!("Face models loaded (scrfd={:?}, arcface={:?})", rp, ap,);
                        return sess;
                    }
                    Err(e) => {
                        tracing::warn!("Face model load failed ({}); falling back to stub", e);
                    }
                }
            }
        } else {
            tracing::debug!(
                "Face models absent (scrfd_found={retina_exists}, arcface_found={arcface_exists}) — using stub"
            );
        }

        Self {
            retina: Mutex::new(None),
            arcface: Mutex::new(None),
            is_stub: true,
        }
    }
}

// ── Global memoised session (shared across pipeline runs) ──────────────────

/// Holds the process-wide `FacesSession`. `None` variant means a successful
/// resolve-and-init ran but either model file is absent at the resolved path
/// — callers should skip face work. `Some(sess)` may still be a stub if
/// inference init failed after file-existence checks passed (rare).
static GLOBAL_FACES: OnceLock<Option<FacesSession>> = OnceLock::new();

/// Initialise the process-wide `FacesSession` exactly once. Subsequent calls
/// are no-ops — the first call's resolution wins.
///
/// Passing `None` for either path means "we already know the file is absent";
/// the global is seeded with `None` and face work is permanently skipped for
/// this process. Re-init after a bundled-dir re-resolution requires a restart.
///
/// Safe to call from any thread; the `OnceLock` linearises.
pub fn init_global_faces_session(retina_path: Option<&Path>, arcface_path: Option<&Path>) {
    GLOBAL_FACES.get_or_init(|| match (retina_path, arcface_path) {
        (Some(r), Some(a)) if r.exists() && a.exists() => match FacesSession::load(r, a) {
            Ok(sess) => {
                tracing::info!(
                    scrfd = %r.display(),
                    arcface = %a.display(),
                    "global FacesSession initialised"
                );
                Some(sess)
            }
            Err(e) => {
                tracing::warn!(error = %e, "global FacesSession load failed — face work disabled");
                None
            }
        },
        _ => {
            tracing::debug!("global FacesSession: model paths absent — face work disabled");
            None
        }
    });
}

/// Access the memoised global session. Returns `None` when either model file
/// was absent at init time (or init was never called — e.g. tests).
///
/// Pipeline stage-5 + any future face-related command should prefer this over
/// `FacesSession::load` to avoid re-init (~2s per call, ~190MB ONNX commit).
pub fn global_faces_session() -> Option<&'static FacesSession> {
    GLOBAL_FACES.get().and_then(|o| o.as_ref())
}

impl FacesSession {
    // ── public API ────────────────────────────────────────────────────────

    /// Detect faces in `image_path` using default detection parameters.
    ///
    /// Returns `Ok(vec![])` when running as a stub (model absent).
    pub fn detect_faces(&self, image_path: &Path) -> AppResult<Vec<FaceBox>> {
        self.detect_faces_with(image_path, &DetectParams::default())
    }

    /// Detect faces with explicit tuning parameters.
    ///
    /// Returns `Ok(vec![])` when running as a stub.
    pub fn detect_faces_with(
        &self,
        image_path: &Path,
        params: &DetectParams,
    ) -> AppResult<Vec<FaceBox>> {
        if self.is_stub {
            let _ = image_path;
            let _ = params;
            return Ok(vec![]);
        }

        let img = image::open(image_path)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
        let orig_w = img.width() as f32;
        let orig_h = img.height() as f32;

        // Letterbox to 640×640.
        let (input_data, scale, pad_left, pad_top) = letterbox_image(&img)?;

        let shape = vec![1i64, 3, SCRFD_SIZE as i64, SCRFD_SIZE as i64];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, input_data))
            .map_err(|e| AppError::Internal(format!("ort tensor (scrfd): {e}")))?;

        let mut guard = self
            .retina
            .lock()
            .map_err(|_| AppError::Internal("scrfd session mutex poisoned".into()))?;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::Internal("scrfd session is None despite is_stub=false".into())
        })?;

        let outputs = session
            .run(ort::inputs!["input.1" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run (scrfd): {e}")))?;

        // Decode all three stride levels.
        let mut candidates: Vec<FaceBox> = Vec::new();
        for (stride, score_key, bbox_key, kps_key, grid_side) in [
            (8u32, "448", "451", "454", 80usize),
            (16u32, "471", "474", "477", 40usize),
            (32u32, "494", "497", "500", 20usize),
        ] {
            let decoded = decode_scrfd_stride(
                &outputs,
                stride,
                score_key,
                bbox_key,
                kps_key,
                grid_side,
                params.detect_threshold,
            )?;
            candidates.extend(decoded);
        }

        tracing::debug!(
            "scrfd: {} candidates before NMS (scale={:.4}, pad=({pad_left},{pad_top}))",
            candidates.len(),
            scale
        );

        // NMS.
        let mut kept = nms_greedy(candidates, params.nms_iou);

        // Un-letterbox: remove pad, divide by scale, clamp.
        for fb in &mut kept {
            fb.x = ((fb.x - pad_left) / scale).clamp(0.0, orig_w);
            fb.y = ((fb.y - pad_top) / scale).clamp(0.0, orig_h);
            fb.w = (fb.w / scale).min(orig_w - fb.x);
            fb.h = (fb.h / scale).min(orig_h - fb.y);
            for lm in &mut fb.landmarks {
                lm[0] = ((lm[0] - pad_left) / scale).clamp(0.0, orig_w);
                lm[1] = ((lm[1] - pad_top) / scale).clamp(0.0, orig_h);
            }
        }

        tracing::debug!("scrfd: {} faces after NMS + unletterbox", kept.len());
        Ok(kept)
    }

    /// Crop and embed a face chip defined by `bbox` from `image_path`.
    ///
    /// Returns `Ok(vec![0.0; FACE_EMBED_DIM])` when running as a stub.
    /// When the real model is loaded, the crop is aligned via the 5-point
    /// landmarks and passed through ArcFace; the output is L2-normalised.
    pub fn embed_face(&self, image_path: &Path, bbox: &FaceBox) -> AppResult<Vec<f32>> {
        if self.is_stub {
            let _ = image_path;
            let _ = bbox;
            return Ok(vec![0.0_f32; FACE_EMBED_DIM]);
        }

        let img = image::open(image_path)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;

        // Build the affine transform from bbox landmarks → ArcFace target points.
        // We use a 3-point solve (left-eye, right-eye, nose) to determine the
        // 6 affine parameters; no nalgebra/ndarray are available in this dep set.
        let m = affine_from_3pts(&bbox.landmarks)?;

        // Warp to 112×112 chip using bilinear sampling.
        let chip_data = warp_affine_bilinear(&img, &m)?;

        let shape = vec![1i64, 3, ARCFACE_SIZE as i64, ARCFACE_SIZE as i64];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, chip_data))
            .map_err(|e| AppError::Internal(format!("ort tensor (arcface): {e}")))?;

        let mut guard = self
            .arcface
            .lock()
            .map_err(|_| AppError::Internal("arcface session mutex poisoned".into()))?;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::Internal("arcface session is None despite is_stub=false".into())
        })?;

        let outputs = session
            .run(ort::inputs!["input.1" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run (arcface): {e}")))?;

        let tensor = outputs
            .get("683")
            .ok_or_else(|| AppError::Internal("output '683' not found in arcface model".into()))?;
        let (_shape, data) = tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal(format!("extract arcface tensor: {e}")))?;
        let mut embedding: Vec<f32> = data.to_vec();

        if embedding.len() != FACE_EMBED_DIM {
            return Err(AppError::Internal(format!(
                "arcface output dim mismatch: expected {FACE_EMBED_DIM}, got {}",
                embedding.len()
            )));
        }

        l2_normalise(&mut embedding);
        Ok(embedding)
    }

    /// Embed a pre-cropped face image directly (no SCRFD detection, no 5-point
    /// landmark alignment). Resize to 112×112 + normalise to [-1, 1] + run
    /// ArcFace. Intended for test fixtures like LFW where photos are already
    /// tightly-cropped canonical-pose faces and SCRFD would reject them
    /// because they're out of its training distribution.
    ///
    /// NOT intended for production — real photos need SCRFD landmarks for
    /// alignment to hit ArcFace's quoted accuracy.
    pub fn embed_prealigned_face(&self, image_path: &Path) -> AppResult<Vec<f32>> {
        if self.is_stub {
            let _ = image_path;
            return Ok(vec![0.0_f32; FACE_EMBED_DIM]);
        }

        let img = image::open(image_path)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
        // Resize straight to 112×112 (ArcFace input). Triangle filter is a
        // reasonable default for downscale/upscale alike.
        let chip = img
            .resize_exact(
                ARCFACE_SIZE,
                ARCFACE_SIZE,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();

        // Channels-first f32 [1, 3, 112, 112], normalise (x - 127.5) / 127.5.
        let n = (ARCFACE_SIZE * ARCFACE_SIZE) as usize;
        let mut chw: Vec<f32> = vec![0.0; 3 * n];
        for (i, pixel) in chip.pixels().enumerate() {
            let y = i / ARCFACE_SIZE as usize;
            let x = i % ARCFACE_SIZE as usize;
            let idx = y * ARCFACE_SIZE as usize + x;
            chw[idx] = (pixel.0[0] as f32 - 127.5) / 127.5;
            chw[n + idx] = (pixel.0[1] as f32 - 127.5) / 127.5;
            chw[2 * n + idx] = (pixel.0[2] as f32 - 127.5) / 127.5;
        }

        let shape = vec![1i64, 3, ARCFACE_SIZE as i64, ARCFACE_SIZE as i64];
        let input_tensor = ort::value::Tensor::<f32>::from_array((shape, chw))
            .map_err(|e| AppError::Internal(format!("ort tensor (arcface prealigned): {e}")))?;

        let mut guard = self
            .arcface
            .lock()
            .map_err(|_| AppError::Internal("arcface session mutex poisoned".into()))?;
        let session = guard.as_mut().ok_or_else(|| {
            AppError::Internal("arcface session is None despite is_stub=false".into())
        })?;

        let outputs = session
            .run(ort::inputs!["input.1" => input_tensor])
            .map_err(|e| AppError::Internal(format!("ort run (arcface prealigned): {e}")))?;

        let tensor = outputs
            .get("683")
            .ok_or_else(|| AppError::Internal("output '683' not found in arcface model".into()))?;
        let (_shape, data) = tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal(format!("extract arcface tensor: {e}")))?;
        let mut embedding: Vec<f32> = data.to_vec();

        if embedding.len() != FACE_EMBED_DIM {
            return Err(AppError::Internal(format!(
                "arcface output dim mismatch: expected {FACE_EMBED_DIM}, got {}",
                embedding.len()
            )));
        }

        l2_normalise(&mut embedding);
        Ok(embedding)
    }
}

// ── image preprocessing helpers ───────────────────────────────────────────

/// Letterbox `img` to `SCRFD_SIZE × SCRFD_SIZE`, preserving aspect ratio.
/// Pads with the SCRFD gray value `(128-127.5)/128.0 ≈ 0.0039`.
///
/// Returns `(flat_chw_f32, scale, pad_left, pad_top)`.
fn letterbox_image(img: &image::DynamicImage) -> AppResult<(Vec<f32>, f32, f32, f32)> {
    let (orig_w, orig_h) = (img.width() as f32, img.height() as f32);
    let s = SCRFD_SIZE as f32;
    let scale = (s / orig_w).min(s / orig_h);
    let new_w = (orig_w * scale).round() as u32;
    let new_h = (orig_h * scale).round() as u32;

    let resized = img
        .resize_exact(new_w, new_h, FilterType::Lanczos3)
        .into_rgb8();

    let pad_left = ((s - new_w as f32) / 2.0).floor();
    let pad_top = ((s - new_h as f32) / 2.0).floor();
    let size = SCRFD_SIZE as usize;

    // Fill with gray (128 normalised).
    let mut pixels = vec![LETTERBOX_GRAY; 3 * size * size];

    for y in 0..new_h as usize {
        for x in 0..new_w as usize {
            let px = resized.get_pixel(x as u32, y as u32);
            let dx = x + pad_left as usize;
            let dy = y + pad_top as usize;
            if dx < size && dy < size {
                for c in 0..3usize {
                    let norm = (px[c] as f32 - 127.5) / 128.0;
                    pixels[c * size * size + dy * size + dx] = norm;
                }
            }
        }
    }

    Ok((pixels, scale, pad_left, pad_top))
}

// ── SCRFD decode helpers ───────────────────────────────────────────────────

/// Decode one SCRFD stride-level into `FaceBox` candidates above `threshold`.
fn decode_scrfd_stride(
    outputs: &ort::session::SessionOutputs,
    stride: u32,
    score_key: &str,
    bbox_key: &str,
    kps_key: &str,
    grid_side: usize,
    threshold: f32,
) -> AppResult<Vec<FaceBox>> {
    // Extract raw tensors.
    let score_t = outputs
        .get(score_key)
        .ok_or_else(|| AppError::Internal(format!("scrfd output '{score_key}' not found")))?;
    let bbox_t = outputs
        .get(bbox_key)
        .ok_or_else(|| AppError::Internal(format!("scrfd output '{bbox_key}' not found")))?;
    let kps_t = outputs
        .get(kps_key)
        .ok_or_else(|| AppError::Internal(format!("scrfd output '{kps_key}' not found")))?;

    let (_, score_raw) = score_t
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("extract score tensor: {e}")))?;
    let (_, bbox_raw) = bbox_t
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("extract bbox tensor: {e}")))?;
    let (_, kps_raw) = kps_t
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("extract kps tensor: {e}")))?;

    let scores: Vec<f32> = score_raw.to_vec();
    let bboxes: Vec<f32> = bbox_raw.to_vec();
    let kps: Vec<f32> = kps_raw.to_vec();

    // 2 anchors per grid cell.
    const ANCHORS_PER_CELL: usize = 2;
    let n = grid_side * grid_side * ANCHORS_PER_CELL;
    debug_assert_eq!(scores.len(), n);

    let mut faces = Vec::new();
    let stride_f = stride as f32;

    for idx in 0..n {
        let conf = scores[idx];
        if conf < threshold {
            continue;
        }

        // Anchor center: two anchors per cell share the same center.
        let cell = idx / ANCHORS_PER_CELL;
        let row = cell / grid_side;
        let col = cell % grid_side;
        let cx = (col as f32 + 0.5) * stride_f;
        let cy = (row as f32 + 0.5) * stride_f;

        // bbox decode: distances from center (left, top, right, bottom) × stride.
        let bl = bboxes[idx * 4];
        let bt = bboxes[idx * 4 + 1];
        let br = bboxes[idx * 4 + 2];
        let bb = bboxes[idx * 4 + 3];

        let x1 = cx - bl * stride_f;
        let y1 = cy - bt * stride_f;
        let x2 = cx + br * stride_f;
        let y2 = cy + bb * stride_f;

        // Landmark decode: 5 × (dx, dy) offsets from center × stride.
        let mut landmarks = [[0.0f32; 2]; 5];
        for k in 0..5usize {
            landmarks[k][0] = cx + kps[idx * 10 + k * 2] * stride_f;
            landmarks[k][1] = cy + kps[idx * 10 + k * 2 + 1] * stride_f;
        }

        faces.push(FaceBox {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
            score: conf,
            landmarks,
        });
    }

    Ok(faces)
}

// ── NMS ───────────────────────────────────────────────────────────────────

/// Greedy NMS: sort by descending score, suppress boxes with IoU > threshold.
fn nms_greedy(mut boxes: Vec<FaceBox>, iou_threshold: f32) -> Vec<FaceBox> {
    // Sort highest score first.
    boxes.sort_unstable_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<FaceBox> = Vec::with_capacity(boxes.len());
    'outer: for candidate in boxes {
        for kept_box in &kept {
            if iou(&candidate, kept_box) > iou_threshold {
                continue 'outer;
            }
        }
        kept.push(candidate);
    }
    kept
}

/// Intersection-over-union of two `FaceBox` values.
fn iou(a: &FaceBox, b: &FaceBox) -> f32 {
    let ax2 = a.x + a.w;
    let ay2 = a.y + a.h;
    let bx2 = b.x + b.w;
    let by2 = b.y + b.h;

    let ix1 = a.x.max(b.x);
    let iy1 = a.y.max(b.y);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);

    let inter_w = (ix2 - ix1).max(0.0);
    let inter_h = (iy2 - iy1).max(0.0);
    let inter = inter_w * inter_h;
    if inter <= 0.0 {
        return 0.0;
    }

    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 {
        return 0.0;
    }
    inter / union
}

// ── affine alignment helpers ──────────────────────────────────────────────

/// 2×3 affine matrix stored as `[a, b, tx, c, d, ty]` mapping
/// source `(x, y)` → `(a*x + b*y + tx, c*x + d*y + ty)`.
type Affine2x3 = [f32; 6];

/// Compute a 2×3 affine transform from landmark src → ARCFACE_DST.
///
/// We use the first 3 point pairs (left-eye, right-eye, nose) to solve the
/// 6-unknown linear system exactly. Using exactly 3 pairs gives an exact
/// solution (no over-determined least-squares needed); the remaining 2 pairs
/// are left as residuals — acceptable for faces with reasonable pose.
///
/// This avoids requiring `nalgebra` or `ndarray` in the dependency set.
fn affine_from_3pts(src: &[[f32; 2]; 5]) -> AppResult<Affine2x3> {
    // Solve: for each of 3 source points sᵢ = (xᵢ, yᵢ) and target dᵢ = (uᵢ, vᵢ):
    //   u = a*x + b*y + tx
    //   v = c*x + d*y + ty
    //
    // This splits into two independent 3×3 linear systems:
    //   [[x0 y0 1], [x1 y1 1], [x2 y2 1]] * [a, b, tx]^T = [u0, u1, u2]^T
    //   [[x0 y0 1], [x1 y1 1], [x2 y2 1]] * [c, d, ty]^T = [v0, v1, v2]^T

    let (x0, y0) = (src[0][0], src[0][1]);
    let (x1, y1) = (src[1][0], src[1][1]);
    let (x2, y2) = (src[2][0], src[2][1]);
    let (u0, v0) = (ARCFACE_DST[0][0], ARCFACE_DST[0][1]);
    let (u1, v1) = (ARCFACE_DST[1][0], ARCFACE_DST[1][1]);
    let (u2, v2) = (ARCFACE_DST[2][0], ARCFACE_DST[2][1]);

    // Determinant of the 3×3 system matrix A.
    let det = x0 * (y1 - y2) - x1 * (y0 - y2) + x2 * (y0 - y1);
    if det.abs() < 1e-6 {
        return Err(AppError::Internal(
            "affine solve failed: degenerate landmark configuration (det ~ 0)".into(),
        ));
    }
    let inv_det = 1.0 / det;

    // Cofactor rows of A⁻¹ (Cramer's rule for the first two columns of Aᵀ).
    let c00 = (y1 - y2) * inv_det;
    let c01 = (x2 - x1) * inv_det;
    let c02 = (x1 * y2 - x2 * y1) * inv_det;
    let c10 = (y2 - y0) * inv_det;
    let c11 = (x0 - x2) * inv_det;
    let c12 = (x2 * y0 - x0 * y2) * inv_det;
    let c20 = (y0 - y1) * inv_det;
    let c21 = (x1 - x0) * inv_det;
    let c22 = (x0 * y1 - x1 * y0) * inv_det;

    // [a, b, tx] = A⁻¹ * [u0, u1, u2]^T
    let a = c00 * u0 + c10 * u1 + c20 * u2;
    let b = c01 * u0 + c11 * u1 + c21 * u2;
    let tx = c02 * u0 + c12 * u1 + c22 * u2;

    // [c, d, ty] = A⁻¹ * [v0, v1, v2]^T
    let c = c00 * v0 + c10 * v1 + c20 * v2;
    let d = c01 * v0 + c11 * v1 + c21 * v2;
    let ty = c02 * v0 + c12 * v1 + c22 * v2;

    Ok([a, b, tx, c, d, ty])
}

/// Warp `src` image to a 112×112 chip using inverse affine + bilinear sampling.
///
/// The forward affine maps source coords → destination coords. For sampling
/// we invert it to map each destination pixel back to a source coordinate.
///
/// Returns a channels-first `[3 × 112 × 112]` f32 buffer normalised to `[-1, 1]`.
fn warp_affine_bilinear(src: &image::DynamicImage, fwd: &Affine2x3) -> AppResult<Vec<f32>> {
    // Invert the 2×3 affine.
    // fwd = [a, b, tx, c, d, ty]  means  dst = A * src + t
    // inv: src = A⁻¹ * (dst - t)
    let a = fwd[0];
    let b = fwd[1];
    let tx = fwd[2];
    let c = fwd[3];
    let d = fwd[4];
    let ty = fwd[5];

    let det = a * d - b * c;
    if det.abs() < 1e-9 {
        return Err(AppError::Internal(
            "affine warp failed: singular matrix".into(),
        ));
    }
    let inv_det = 1.0 / det;
    let ia = d * inv_det;
    let ib = -b * inv_det;
    let ic = -c * inv_det;
    let id = a * inv_det;
    let itx = (b * ty - d * tx) * inv_det;
    let ity = (c * tx - a * ty) * inv_det;

    let rgb = src.to_rgb8();
    let (sw, sh) = (rgb.width() as f32, rgb.height() as f32);
    let out_size = ARCFACE_SIZE as usize;
    let mut chip = vec![0.0f32; 3 * out_size * out_size];

    for dy in 0..out_size {
        for dx in 0..out_size {
            // Map destination pixel → source coordinates.
            let dfx = dx as f32 + 0.5;
            let dfy = dy as f32 + 0.5;
            let sx = ia * dfx + ib * dfy + itx;
            let sy = ic * dfx + id * dfy + ity;

            // Bilinear interpolation.
            let x0 = sx.floor();
            let y0 = sy.floor();
            let x_frac = sx - x0;
            let y_frac = sy - y0;
            let x0i = x0 as i32;
            let y0i = y0 as i32;

            let sample = |xi: i32, yi: i32| -> [f32; 3] {
                let xi = xi.clamp(0, (sw as i32) - 1) as u32;
                let yi = yi.clamp(0, (sh as i32) - 1) as u32;
                let px = rgb.get_pixel(xi, yi);
                [px[0] as f32, px[1] as f32, px[2] as f32]
            };

            let p00 = sample(x0i, y0i);
            let p10 = sample(x0i + 1, y0i);
            let p01 = sample(x0i, y0i + 1);
            let p11 = sample(x0i + 1, y0i + 1);

            for c_idx in 0..3usize {
                let top = p00[c_idx] * (1.0 - x_frac) + p10[c_idx] * x_frac;
                let bot = p01[c_idx] * (1.0 - x_frac) + p11[c_idx] * x_frac;
                let val = top * (1.0 - y_frac) + bot * y_frac;
                // Normalise to [-1, 1].
                let norm = (val - 127.5) / 127.5;
                chip[c_idx * out_size * out_size + dy * out_size + dx] = norm;
            }
        }
    }

    Ok(chip)
}

// ── vector math ───────────────────────────────────────────────────────────

/// L2-normalise a vector in-place. No-op if norm < 1e-10.
fn l2_normalise(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── existing contract tests (kept unchanged) ──────────────────────────

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
        let scrfd = PathBuf::from("/nonexistent/det_10g.onnx");
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

    // ── new unit tests ────────────────────────────────────────────────────

    /// When detect_faces is called with a stub session, detect_faces_with
    /// also returns empty regardless of params.
    #[test]
    fn stub_detect_faces_with_custom_params_returns_empty() {
        let sess = FacesSession::load_or_stub(None, None);
        let params = DetectParams {
            detect_threshold: 0.1,
            nms_iou: 0.3,
        };
        let faces = sess
            .detect_faces_with(Path::new("/nonexistent/photo.jpg"), &params)
            .expect("stub should not error");
        assert!(faces.is_empty());
    }

    /// IoU of a box with itself must be 1.0.
    #[test]
    fn iou_self_is_one() {
        let fb = FaceBox {
            x: 10.0,
            y: 10.0,
            w: 50.0,
            h: 80.0,
            score: 0.9,
            landmarks: [[0.0; 2]; 5],
        };
        let v = iou(&fb, &fb);
        assert!((v - 1.0).abs() < 1e-5, "iou(self, self) = {v}");
    }

    /// IoU of non-overlapping boxes must be 0.0.
    #[test]
    fn iou_non_overlapping_is_zero() {
        let a = FaceBox {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            score: 0.9,
            landmarks: [[0.0; 2]; 5],
        };
        let b = FaceBox {
            x: 20.0,
            y: 20.0,
            w: 10.0,
            h: 10.0,
            score: 0.8,
            landmarks: [[0.0; 2]; 5],
        };
        assert_eq!(iou(&a, &b), 0.0);
    }

    /// L2 norm of a normalised vector must be ~1.
    #[test]
    fn l2_normalise_produces_unit_vector() {
        let mut v = vec![3.0_f32, 4.0];
        l2_normalise(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6, "norm = {norm}");
    }

    /// L2 norm of a zero vector must not panic and must stay zero.
    #[test]
    fn l2_normalise_zero_vector_is_noop() {
        let mut v = vec![0.0_f32; FACE_EMBED_DIM];
        l2_normalise(&mut v);
        assert!(v.iter().all(|&x| x == 0.0));
    }

    /// When the five source landmarks equal the five ArcFace destination points,
    /// the affine matrix should map them back to within floating-point tolerance.
    ///
    /// We solve from 3 src=dst point pairs so the remaining 2 are residuals;
    /// since all 5 share the same identity-like mapping here, those residuals
    /// should also be near-zero.
    #[test]
    fn arcface_alignment_matrix_round_trips_identity_landmarks() {
        // src == dst: the affine that maps ARCFACE_DST[0..3] → ARCFACE_DST[0..3]
        // is the identity (a=1, b=0, tx=0, c=0, d=1, ty=0).
        let m = affine_from_3pts(&ARCFACE_DST).expect("affine_from_3pts");

        // Apply forward map to each source point and compare with destination.
        for (i, src) in ARCFACE_DST.iter().enumerate() {
            let mapped_x = m[0] * src[0] + m[1] * src[1] + m[2];
            let mapped_y = m[3] * src[0] + m[4] * src[1] + m[5];
            let dst = ARCFACE_DST[i];
            assert!(
                (mapped_x - dst[0]).abs() < 1.0,
                "point {i}: mapped_x={mapped_x:.3} dst_x={:.3}",
                dst[0]
            );
            assert!(
                (mapped_y - dst[1]).abs() < 1.0,
                "point {i}: mapped_y={mapped_y:.3} dst_y={:.3}",
                dst[1]
            );
        }
    }

    /// Degenerate landmarks (all at the same point) must return an error, not panic.
    #[test]
    fn affine_degenerate_landmarks_returns_error() {
        let degenerate = [[30.0f32, 40.0]; 5];
        let result = affine_from_3pts(&degenerate);
        assert!(
            result.is_err(),
            "expected Err for degenerate landmarks, got Ok"
        );
    }

    /// NMS with a single box must keep that box.
    #[test]
    fn nms_single_box_is_kept() {
        let fb = FaceBox {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            score: 0.9,
            landmarks: [[0.0; 2]; 5],
        };
        let kept = nms_greedy(vec![fb.clone()], 0.45);
        assert_eq!(kept.len(), 1);
    }

    /// NMS must suppress a highly overlapping lower-score box.
    #[test]
    fn nms_suppresses_overlapping_lower_score() {
        let high = FaceBox {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            score: 0.95,
            landmarks: [[0.0; 2]; 5],
        };
        // Almost identical box with lower score — IoU ≈ 0.81.
        let low = FaceBox {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
            score: 0.7,
            landmarks: [[0.0; 2]; 5],
        };
        let kept = nms_greedy(vec![high, low], 0.45);
        assert_eq!(kept.len(), 1);
        assert!((kept[0].score - 0.95).abs() < 1e-5);
    }

    /// Letterbox of a square image should have zero padding.
    #[test]
    fn letterbox_square_has_zero_padding() {
        use image::{ImageBuffer, Rgb};
        let img = image::DynamicImage::ImageRgb8(ImageBuffer::from_fn(320, 320, |_, _| {
            Rgb([100u8, 150, 200])
        }));
        let (pixels, scale, pad_left, pad_top) = letterbox_image(&img).expect("letterbox");
        assert_eq!(pixels.len(), 3 * 640 * 640);
        assert!((scale - 2.0).abs() < 1e-4, "scale={scale}");
        assert_eq!(pad_left as u32, 0);
        assert_eq!(pad_top as u32, 0);
    }

    /// Letterbox of a wide image should have non-zero vertical padding.
    #[test]
    fn letterbox_wide_image_has_vertical_padding() {
        use image::{ImageBuffer, Rgb};
        let img = image::DynamicImage::ImageRgb8(ImageBuffer::from_fn(640, 320, |_, _| {
            Rgb([100u8, 150, 200])
        }));
        let (_pixels, _scale, pad_left, pad_top) = letterbox_image(&img).expect("letterbox");
        assert_eq!(pad_left as u32, 0, "wide image: no horizontal pad");
        assert!(
            pad_top > 0.0,
            "wide image: expect top padding, got {pad_top}"
        );
    }

    // ── ignored integration tests (require real ONNX models) ─────────────

    /// Integration test: drive the real SCRFD-10g model against a known image.
    ///
    /// Skipped unless CHRONIMAGE_MODELS_DIR is set and the models exist.
    /// Place a Creative-Commons group photo at
    /// `tests/fixtures/face-detect/group.jpg` for a definitive assertion.
    #[test]
    #[ignore]
    fn scrfd_detects_faces_in_real_group_photo() {
        // Respect the test-isolation override used in CI and fast-test mode.
        let models_dir = match std::env::var("CHRONIMAGE_MODELS_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => dirs::data_local_dir()
                .expect("data_local_dir")
                .join("app.chronimage.desktop")
                .join("models"),
        };

        let scrfd_path = models_dir.join("det_10g.onnx");
        let arcface_path = models_dir.join("w600k_r50.onnx");

        if !scrfd_path.exists() || !arcface_path.exists() {
            eprintln!("Integration test skipped: models not found at {models_dir:?}");
            return;
        }

        let sess = FacesSession::load(&scrfd_path, &arcface_path).expect("load real sessions");

        // Find or skip the fixture photo.
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

        let faces = sess
            .detect_faces(&fixture)
            .expect("detect_faces on real photo");

        assert!(
            faces.len() >= 3,
            "expected ≥ 3 faces in group photo, got {} — check fixture",
            faces.len()
        );

        // Also smoke-test embedding on the first detected face.
        let emb = sess
            .embed_face(&fixture, &faces[0])
            .expect("embed_face on real photo");
        assert_eq!(emb.len(), FACE_EMBED_DIM);
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "embedding not unit-normed: norm={norm}"
        );
    }
}
