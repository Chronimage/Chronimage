use crate::{
    ai::providers::session_builder_with_ep,
    develop::segmentation::{FaceHint, GeneratedMask},
    AppError, AppResult,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{codecs::png::PngEncoder, imageops::FilterType, ImageEncoder, RgbImage};
use ort::session::Session;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const ENCODER_SIZE: u32 = 1024;
#[allow(dead_code)]
const EMBED_DIM: usize = 256 * 64 * 64;
const MASK_INPUT_DIM: usize = 256 * 256;
const IMAGENET_MEAN: [f32; 3] = [123.675, 116.28, 103.53];
const IMAGENET_STD: [f32; 3] = [58.395, 57.12, 57.375];

#[derive(Debug, Clone)]
struct TensorData {
    shape: Vec<i64>,
    data: Vec<f32>,
}

/// Encoder output reused across many decoder calls. `pub` so the
/// `chronimage-mask-debug` bin can encode an image once and iterate
/// prompt strategies through `decode_normalized` without re-running
/// the (expensive) image encoder.
#[derive(Debug, Clone)]
pub struct Sam2Features {
    image_embed: TensorData,
    high_res_feats_0: TensorData,
    high_res_feats_1: TensorData,
}

/// Single SAM2.1 point prompt expressed in normalised image coords.
/// `(x, y)` are in `[0, 1]`; `label` is `1.0` for foreground, `0.0` for background.
pub type NormalizedPrompt = (f32, f32, f32);

#[derive(Debug)]
pub struct SamSession {
    encoder: Mutex<Option<Session>>,
    decoder: Mutex<Option<Session>>,
    pub is_stub: bool,
}

impl SamSession {
    pub fn load(encoder_path: &Path, decoder_path: &Path) -> AppResult<Self> {
        if !encoder_path.exists() {
            return Err(AppError::NotFound("SAM2.1 encoder not found".into()));
        }
        if !decoder_path.exists() {
            return Err(AppError::NotFound("SAM2.1 decoder not found".into()));
        }
        let encoder = session_builder_with_ep("sam2.1-encoder")
            .map_err(|e| AppError::Internal(format!("ort builder (sam2.1-encoder): {e}")))?
            .commit_from_file(encoder_path)
            .map_err(|e| AppError::Internal(format!("ort load sam2.1-encoder: {e}")))?;
        let decoder = session_builder_with_ep("sam2.1-decoder")
            .map_err(|e| AppError::Internal(format!("ort builder (sam2.1-decoder): {e}")))?
            .commit_from_file(decoder_path)
            .map_err(|e| AppError::Internal(format!("ort load sam2.1-decoder: {e}")))?;
        tracing::info!(
            encoder = %encoder_path.display(),
            decoder = %decoder_path.display(),
            "SAM2.1 sessions loaded"
        );
        Ok(Self {
            encoder: Mutex::new(Some(encoder)),
            decoder: Mutex::new(Some(decoder)),
            is_stub: false,
        })
    }

    pub fn load_or_stub(enc: Option<&Path>, dec: Option<&Path>) -> Self {
        let enc_ok = enc.map(|p| p.exists()).unwrap_or(false);
        let dec_ok = dec.map(|p| p.exists()).unwrap_or(false);
        if enc_ok && dec_ok {
            if let (Some(e), Some(d)) = (enc, dec) {
                match Self::load(e, d) {
                    Ok(s) => return s,
                    Err(err) => tracing::warn!(error = %err, "SAM2.1 load failed"),
                }
            }
        } else {
            tracing::debug!(
                encoder_found = enc_ok,
                decoder_found = dec_ok,
                "SAM2.1 models absent"
            );
        }
        Self {
            encoder: Mutex::new(None),
            decoder: Mutex::new(None),
            is_stub: true,
        }
    }

    pub fn generate_bitmap_mask(
        &self,
        img: &RgbImage,
        source: &str,
        face_hints: &[FaceHint],
    ) -> AppResult<GeneratedMask> {
        if self.is_stub {
            return Err(AppError::NotFound(
                "SAM2.1 stub -- models not loaded".into(),
            ));
        }
        let (orig_w, orig_h) = img.dimensions();
        if orig_w == 0 || orig_h == 0 {
            return Err(AppError::InvalidInput(
                "cannot generate SAM mask for empty image".into(),
            ));
        }
        let embeddings = self.encode(img)?;
        let (coords, labels) = build_prompts(source, face_hints, orig_w, orig_h);
        let decoded = self.decode_refined(&embeddings, &coords, &labels, orig_h, orig_w)?;
        let mut alpha = threshold_to_alpha(source, &decoded, orig_w, orig_h);
        refine_alpha_with_guided_filter(
            &mut alpha,
            img,
            guided_radius_for(orig_w, orig_h),
            GUIDED_EPS,
        );
        let confidence = mask_confidence(&alpha);
        let data_b64 = encode_luma_png(orig_w, orig_h, &alpha)?;
        Ok(GeneratedMask {
            width: orig_w,
            height: orig_h,
            data_b64,
            confidence,
            model: "sam2.1-hiera-large",
        })
    }

    /// Encode an image into reusable SAM2.1 features. Public so debug
    /// tooling can run the encoder once and decode many prompt
    /// strategies against the cached features.
    pub fn encode_features(&self, img: &RgbImage) -> AppResult<Sam2Features> {
        if self.is_stub {
            return Err(AppError::NotFound(
                "SAM2.1 stub -- models not loaded".into(),
            ));
        }
        self.encode(img)
    }

    /// Decode a mask from cached features using normalised `(x, y, label)`
    /// prompts. `invert` produces a background-style alpha (1 - sigmoid).
    /// `source_img` is the original RGB image used as guidance for the
    /// edge-aware refinement pass; pass the same image you encoded.
    /// Used by `chronimage-mask-debug` to iterate prompt strategies.
    pub fn decode_normalized(
        &self,
        features: &Sam2Features,
        prompts: &[NormalizedPrompt],
        source_img: &RgbImage,
        invert: bool,
    ) -> AppResult<GeneratedMask> {
        if self.is_stub {
            return Err(AppError::NotFound(
                "SAM2.1 stub -- models not loaded".into(),
            ));
        }
        let (orig_w, orig_h) = source_img.dimensions();
        if orig_w == 0 || orig_h == 0 {
            return Err(AppError::InvalidInput(
                "cannot decode SAM mask for empty image".into(),
            ));
        }
        let scale = ENCODER_SIZE as f32 / orig_w.max(orig_h) as f32;
        let sw = orig_w as f32 * scale;
        let sh = orig_h as f32 * scale;
        let coords: Vec<[f32; 2]> = prompts
            .iter()
            .map(|(xn, yn, _)| [xn * sw, yn * sh])
            .collect();
        let labels: Vec<f32> = prompts.iter().map(|(_, _, l)| *l).collect();
        let decoded = self.decode_refined(features, &coords, &labels, orig_h, orig_w)?;
        let source = if invert { "background" } else { "subject" };
        let mut alpha = threshold_to_alpha(source, &decoded, orig_w, orig_h);
        refine_alpha_with_guided_filter(
            &mut alpha,
            source_img,
            guided_radius_for(orig_w, orig_h),
            GUIDED_EPS,
        );
        let confidence = mask_confidence(&alpha);
        let data_b64 = encode_luma_png(orig_w, orig_h, &alpha)?;
        Ok(GeneratedMask {
            width: orig_w,
            height: orig_h,
            data_b64,
            confidence,
            model: "sam2.1-hiera-large",
        })
    }

    fn encode(&self, img: &RgbImage) -> AppResult<Sam2Features> {
        let pixel_values = preprocess_encoder(img)?;
        let shape = vec![1i64, 3, ENCODER_SIZE as i64, ENCODER_SIZE as i64];
        let tensor = ort::value::Tensor::<f32>::from_array((shape, pixel_values))
            .map_err(|e| AppError::Internal(format!("ort tensor (sam-encoder): {e}")))?;
        let mut guard = self
            .encoder
            .lock()
            .map_err(|_| AppError::Internal("sam encoder mutex poisoned".into()))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AppError::Internal("sam encoder session None".into()))?;
        let outputs = session
            .run(ort::inputs!["image" => tensor])
            .map_err(|e| AppError::Internal(format!("ort run (sam-encoder): {e}")))?;
        Ok(Sam2Features {
            image_embed: extract_tensor(&outputs, "image_embed")
                .or_else(|_| extract_tensor(&outputs, "image_embedding"))?,
            high_res_feats_0: extract_tensor(&outputs, "high_res_feats_0")?,
            high_res_feats_1: extract_tensor(&outputs, "high_res_feats_1")?,
        })
    }

    /// Run the SAM2 decoder twice: first pass with no mask hint, then a
    /// refinement pass that feeds the best logit grid from pass 1 back as
    /// `mask_input`. The model was trained on this iterative-refinement
    /// regime — pass 2 tightens edges where pass 1 was uncertain (the
    /// dithered halftone you see on soft cloth like an orange dhoti).
    /// Cost: one extra decoder run (~600 ms on DirectML for SAM2-large).
    fn decode_refined(
        &self,
        features: &Sam2Features,
        point_coords: &[[f32; 2]],
        point_labels: &[f32],
        orig_h: u32,
        orig_w: u32,
    ) -> AppResult<DecodedMask> {
        let pass1 = self.decode(features, point_coords, point_labels, None, orig_h, orig_w)?;
        // SAM2 expects `mask_input` at exactly 256×256 (MASK_INPUT_DIM).
        // The vietanhdev export already returns 256×256 logits so the
        // round-trip is identity; if a future export changes that we
        // resample bilinearly to fit.
        let mask_hint = if pass1.mask_w as usize * pass1.mask_h as usize == MASK_INPUT_DIM {
            pass1.logits.clone()
        } else {
            resample_logits_to_mask_input(&pass1.logits, pass1.mask_w, pass1.mask_h)
        };
        self.decode(
            features,
            point_coords,
            point_labels,
            Some(&mask_hint),
            orig_h,
            orig_w,
        )
    }

    fn decode(
        &self,
        features: &Sam2Features,
        point_coords: &[[f32; 2]],
        point_labels: &[f32],
        mask_input: Option<&[f32]>,
        _orig_h: u32,
        _orig_w: u32,
    ) -> AppResult<DecodedMask> {
        let n = point_coords.len() as i64;
        let image_embed_tensor = tensor_from_data("image_embed", &features.image_embed)?;
        let high_res_0_tensor = tensor_from_data("high_res_feats_0", &features.high_res_feats_0)?;
        let high_res_1_tensor = tensor_from_data("high_res_feats_1", &features.high_res_feats_1)?;
        let coords_flat: Vec<f32> = point_coords
            .iter()
            .flat_map(|xy| xy.iter().copied())
            .collect();
        let coords_tensor = ort::value::Tensor::<f32>::from_array((vec![1i64, n, 2], coords_flat))
            .map_err(|e| AppError::Internal(format!("ort tensor (sam-coords): {e}")))?;
        let labels_tensor =
            ort::value::Tensor::<f32>::from_array((vec![1i64, n], point_labels.to_vec()))
                .map_err(|e| AppError::Internal(format!("ort tensor (sam-labels): {e}")))?;
        let (mask_input_data, has_mask_value) = match mask_input {
            Some(m) if m.len() == MASK_INPUT_DIM => (m.to_vec(), 1.0_f32),
            _ => (vec![0.0_f32; MASK_INPUT_DIM], 0.0_f32),
        };
        let mask_input_tensor =
            ort::value::Tensor::<f32>::from_array((vec![1i64, 1, 256, 256], mask_input_data))
                .map_err(|e| AppError::Internal(format!("ort tensor (sam-mask-input): {e}")))?;
        let has_mask_tensor =
            ort::value::Tensor::<f32>::from_array((vec![1i64], vec![has_mask_value]))
                .map_err(|e| AppError::Internal(format!("ort tensor (sam-has-mask): {e}")))?;
        let mut guard = self
            .decoder
            .lock()
            .map_err(|_| AppError::Internal("sam decoder mutex poisoned".into()))?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AppError::Internal("sam decoder session None".into()))?;
        let outputs = session
            .run(ort::inputs![
                "image_embed"      => image_embed_tensor,
                "high_res_feats_0" => high_res_0_tensor,
                "high_res_feats_1" => high_res_1_tensor,
                "point_coords"     => coords_tensor,
                "point_labels"     => labels_tensor,
                "mask_input"       => mask_input_tensor,
                "has_mask_input"   => has_mask_tensor
            ])
            .map_err(|e| AppError::Internal(format!("ort run (sam-decoder): {e}")))?;
        let (mask_shape, masks_data) = outputs["masks"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal(format!("sam-decoder extract masks: {e}")))?;
        let masks_flat = masks_data.to_vec();
        let (_is, iou_data) = outputs["iou_predictions"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal(format!("sam-decoder extract iou: {e}")))?;
        let iou_flat = iou_data.to_vec();
        tracing::debug!(
            "sam-decoder: masks shape={:?}, iou len={} (min={:.3} max={:.3})",
            mask_shape,
            iou_flat.len(),
            iou_flat.iter().cloned().fold(f32::INFINITY, f32::min),
            iou_flat.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        );
        let num_masks = iou_flat.len().clamp(1, 3);
        if masks_flat.is_empty() {
            return Err(AppError::Internal("sam-decoder: empty masks tensor".into()));
        }
        let pixels_per_mask = masks_flat.len() / num_masks;
        if pixels_per_mask == 0 {
            return Err(AppError::Internal(
                "sam-decoder: cannot determine pixels_per_mask".into(),
            ));
        }
        // Mask shape is `[batch, num_masks, mask_h, mask_w]`. The
        // vietanhdev SAM2 ONNX export keeps masks at the native 256×256
        // decoder resolution; older / custom exports may emit 1024×1024.
        // We read the actual dims out of the shape so `threshold_to_alpha`
        // can upsample correctly in either case.
        let (mask_h, mask_w) = if mask_shape.len() >= 4 {
            (mask_shape[2] as u32, mask_shape[3] as u32)
        } else {
            let side = (pixels_per_mask as f64).sqrt().round() as u32;
            (side, side)
        };
        let best_idx = (0..num_masks)
            .max_by(|&a, &b| {
                iou_flat
                    .get(a)
                    .and_then(|va| iou_flat.get(b).map(|vb| va.partial_cmp(vb)))
                    .flatten()
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        let offset = best_idx * pixels_per_mask;
        Ok(DecodedMask {
            logits: masks_flat[offset..offset + pixels_per_mask].to_vec(),
            mask_w,
            mask_h,
        })
    }
}

/// Raw decoder output: SAM2's mask logits at their native grid resolution
/// (typically 256×256), still in letterboxed encoder coordinate space.
/// `threshold_to_alpha` is responsible for unletterboxing + upsampling
/// back to original image dimensions.
struct DecodedMask {
    logits: Vec<f32>,
    mask_w: u32,
    mask_h: u32,
}

/// Bilinearly resample a logit grid to the canonical 256×256 mask_input
/// shape SAM2's decoder expects on its second pass. Only fires when an
/// exotic ONNX export emits something other than 256×256 — for the
/// shipped vietanhdev export this is unreachable in practice.
fn resample_logits_to_mask_input(logits: &[f32], src_w: u32, src_h: u32) -> Vec<f32> {
    let dst = 256_usize;
    let sw = src_w as usize;
    let sh = src_h as usize;
    if sw == 0 || sh == 0 || logits.len() < sw * sh {
        return vec![0.0_f32; MASK_INPUT_DIM];
    }
    let mut out = vec![0.0_f32; dst * dst];
    let sx_per_dx = (sw - 1) as f32 / (dst - 1).max(1) as f32;
    let sy_per_dy = (sh - 1) as f32 / (dst - 1).max(1) as f32;
    for dy in 0..dst {
        let sy = dy as f32 * sy_per_dy;
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let fy = sy - y0 as f32;
        for dx in 0..dst {
            let sx = dx as f32 * sx_per_dx;
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let fx = sx - x0 as f32;
            let l00 = logits[y0 * sw + x0];
            let l01 = logits[y0 * sw + x1];
            let l10 = logits[y1 * sw + x0];
            let l11 = logits[y1 * sw + x1];
            let l0 = l00 * (1.0 - fx) + l01 * fx;
            let l1 = l10 * (1.0 - fx) + l11 * fx;
            out[dy * dst + dx] = l0 * (1.0 - fy) + l1 * fy;
        }
    }
    out
}

fn extract_tensor(outputs: &ort::session::SessionOutputs, name: &str) -> AppResult<TensorData> {
    let value = outputs
        .get(name)
        .ok_or_else(|| AppError::Internal(format!("sam-encoder output {name} missing")))?;
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Internal(format!("sam-encoder extract {name}: {e}")))?;
    Ok(TensorData {
        shape: shape.to_vec(),
        data: data.to_vec(),
    })
}

fn tensor_from_data(name: &str, tensor: &TensorData) -> AppResult<ort::value::Tensor<f32>> {
    ort::value::Tensor::<f32>::from_array((tensor.shape.clone(), tensor.data.clone()))
        .map_err(|e| AppError::Internal(format!("ort tensor ({name}): {e}")))
}

/// SAM2 prompt label conventions (from the official SAM2 ONNX export):
/// `1.0` = foreground point, `0.0` = background point, `2.0` = box
/// top-left, `3.0` = box bottom-right. Box prompts are the canonical
/// way to ask SAM2 "mask the object inside this rectangle" and they're
/// noticeably more stable than free-floating point clusters for
/// object-level masks (the SAM2 paper reports ~5–10% mIoU bump vs.
/// equivalent point prompts on COCO).
const LABEL_FG: f32 = 1.0;
const LABEL_BG: f32 = 0.0;
const LABEL_BOX_TL: f32 = 2.0;
const LABEL_BOX_BR: f32 = 3.0;

fn build_prompts(
    source: &str,
    face_hints: &[FaceHint],
    orig_w: u32,
    orig_h: u32,
) -> (Vec<[f32; 2]>, Vec<f32>) {
    let scale = ENCODER_SIZE as f32 / orig_w.max(orig_h) as f32;
    let sw = orig_w as f32 * scale;
    let sh = orig_h as f32 * scale;
    let to_px = |xn: f32, yn: f32| -> [f32; 2] { [xn * sw, yn * sh] };
    let mut coords: Vec<[f32; 2]> = Vec::new();
    let mut labels: Vec<f32> = Vec::new();

    let push_box =
        |coords: &mut Vec<[f32; 2]>, labels: &mut Vec<f32>, x0: f32, y0: f32, x1: f32, y1: f32| {
            let x0 = x0.clamp(0.005, 0.995);
            let y0 = y0.clamp(0.005, 0.995);
            let x1 = x1.clamp(0.005, 0.995).max(x0 + 0.01);
            let y1 = y1.clamp(0.005, 0.995).max(y0 + 0.01);
            coords.push(to_px(x0, y0));
            labels.push(LABEL_BOX_TL);
            coords.push(to_px(x1, y1));
            labels.push(LABEL_BOX_BR);
        };

    match source {
        "person" => {
            if face_hints.is_empty() {
                coords.push(to_px(0.5, 0.45));
                labels.push(LABEL_FG);
                push_dense_border_negatives(&mut coords, &mut labels, &to_px);
            } else {
                let face = primary_face_hint(face_hints);
                let cx = face.x + face.w * 0.5;
                let cy = face.y + face.h * 0.5;
                // Generous body box: 1.6× face width either side, 6.5×
                // face height down — captures full standing/seated body.
                push_box(
                    &mut coords,
                    &mut labels,
                    face.x - face.w * 1.6,
                    face.y - face.h * 0.6,
                    face.x + face.w + face.w * 1.6,
                    face.y + face.h * 6.5,
                );
                // Anchor positive at face center so SAM2 picks the
                // person inside the box, not a competing object behind.
                coords.push(to_px(cx, cy));
                labels.push(LABEL_FG);
                push_dense_border_negatives(&mut coords, &mut labels, &to_px);
            }
        }
        // Background / landscape masks are computed as the *inverse* of the
        // subject mask: SAM2 produces a subject-shaped logit grid, then
        // `threshold_to_alpha` flips the sigmoid for these source kinds.
        // That means the prompts MUST be the same as "subject" — sending
        // generic image-center prompts here gives SAM2 garbage to invert
        // and leaks the resulting "background" alpha onto the actual
        // subject (e.g. green tint covering the baby).
        "subject" | "object" | "background" | "landscape" => {
            if !face_hints.is_empty() {
                let face = primary_face_hint(face_hints);
                let cx = face.x + face.w * 0.5;
                let cy = face.y + face.h * 0.5;
                // Subject box: tighter than "person" — the user wants
                // *whatever the camera is pointed at*, not necessarily a
                // full standing body. 1.4× face padding sideways, 5.5×
                // height below the face.
                push_box(
                    &mut coords,
                    &mut labels,
                    face.x - face.w * 1.4,
                    face.y - face.h * 0.5,
                    face.x + face.w + face.w * 1.4,
                    face.y + face.h * 5.5,
                );
                coords.push(to_px(cx, cy));
                labels.push(LABEL_FG);
            } else {
                // No face hint — fall back to a centered positive point.
                // Box prompts without a real anchor would just guess at
                // image-center, which is rarely the subject for off-center
                // compositions.
                coords.push(to_px(0.5, 0.5));
                labels.push(LABEL_FG);
            }
            push_dense_border_negatives(&mut coords, &mut labels, &to_px);
        }
        "sky" => {
            // Top band as a box prompt — gives SAM2 a clean rectangle to
            // fill rather than three guess-at-the-horizon positives.
            push_box(&mut coords, &mut labels, 0.05, 0.02, 0.95, 0.35);
            coords.push(to_px(0.5, 0.85));
            labels.push(LABEL_BG);
        }
        "foreground" => {
            push_box(&mut coords, &mut labels, 0.05, 0.65, 0.95, 0.98);
            coords.push(to_px(0.5, 0.12));
            labels.push(LABEL_BG);
        }
        _ => {
            coords.push(to_px(0.5, 0.5));
            labels.push(LABEL_FG);
            coords.push(to_px(0.02, 0.02));
            labels.push(LABEL_BG);
            coords.push(to_px(0.98, 0.98));
            labels.push(LABEL_BG);
        }
    }
    (coords, labels)
}

/// Pick the largest face hint (by area) — used as the anchor for box
/// prompts. Multi-face photos still get a single tight subject mask;
/// the user can layer additional masks via the UI's Add/Subtract modes.
fn primary_face_hint(face_hints: &[FaceHint]) -> &FaceHint {
    face_hints
        .iter()
        .max_by(|a, b| {
            (a.w * a.h)
                .partial_cmp(&(b.w * b.h))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(&face_hints[0])
}

fn push_dense_border_negatives<F>(coords: &mut Vec<[f32; 2]>, labels: &mut Vec<f32>, to_px: &F)
where
    F: Fn(f32, f32) -> [f32; 2],
{
    for &(nx, ny) in &[
        (0.02, 0.02),
        (0.98, 0.02),
        (0.02, 0.98),
        (0.98, 0.98),
        (0.5, 0.02),
        (0.5, 0.98),
        (0.02, 0.5),
        (0.98, 0.5),
    ] {
        coords.push(to_px(nx, ny));
        labels.push(LABEL_BG);
    }
}

fn threshold_to_alpha(source: &str, decoded: &DecodedMask, w: u32, h: u32) -> Vec<u8> {
    let pixels = (w as usize) * (h as usize);
    let invert = matches!(source, "background" | "landscape");
    let to_alpha = |l: f32| {
        let sig = 1.0 / (1.0 + (-l).exp());
        let a = if invert { 1.0 - sig } else { sig };
        (a * 255.0).round().clamp(0.0, 255.0) as u8
    };

    let logits = &decoded.logits;
    let mw = decoded.mask_w as usize;
    let mh = decoded.mask_h as usize;
    if mw == 0 || mh == 0 || logits.len() < mw * mh {
        return vec![0_u8; pixels];
    }

    // The encoder letterboxes the original image into a 1024×1024 square:
    // longest side scales to ENCODER_SIZE; the shorter side is padded with
    // black to fill the square. The decoder then emits a `mw × mh` logit
    // grid (typically 256×256) that covers the *full letterbox*, padding
    // included. To map a pixel `(x, y)` in the original image into mask
    // grid coordinates we therefore go original → letterbox → mask grid:
    //   lx = x * scale,  ly = y * scale     (letterbox space, 0..1024)
    //   mx = lx * mw / 1024, my = ly * mh / 1024
    // The padding region is outside `(orig_w*scale, orig_h*scale)` so we
    // never sample logits from it for any visible original pixel — i.e.
    // the alpha matte we produce is automatically pad-free.
    let scale = ENCODER_SIZE as f32 / w.max(h) as f32;
    let mx_per_x = scale * mw as f32 / ENCODER_SIZE as f32;
    let my_per_y = scale * mh as f32 / ENCODER_SIZE as f32;
    // Image content occupies the first `image_mw × image_mh` cells of
    // the logit grid; the remaining cells are over the letterbox pad.
    // Clamp bilinear sampling to that region so the right-/bottom-edge
    // pixels don't pull in pad logits during interpolation.
    let image_mw_f = (w as f32 * mx_per_x).min(mw as f32);
    let image_mh_f = (h as f32 * my_per_y).min(mh as f32);
    let mx_max = (image_mw_f - 1.0).max(0.0);
    let my_max = (image_mh_f - 1.0).max(0.0);

    let mut alpha = vec![0_u8; pixels];
    for y in 0..h as usize {
        let my = (y as f32 * my_per_y).clamp(0.0, my_max);
        let y0 = my.floor() as usize;
        let y1 = (y0 + 1).min(my_max as usize);
        let fy = my - y0 as f32;
        for x in 0..w as usize {
            let mx = (x as f32 * mx_per_x).clamp(0.0, mx_max);
            let x0 = mx.floor() as usize;
            let x1 = (x0 + 1).min(mx_max as usize);
            let fx = mx - x0 as f32;
            let l00 = logits[y0 * mw + x0];
            let l01 = logits[y0 * mw + x1];
            let l10 = logits[y1 * mw + x0];
            let l11 = logits[y1 * mw + x1];
            let l0 = l00 * (1.0 - fx) + l01 * fx;
            let l1 = l10 * (1.0 - fx) + l11 * fx;
            let l = l0 * (1.0 - fy) + l1 * fy;
            alpha[y * w as usize + x] = to_alpha(l);
        }
    }
    alpha
}

/// Guided-filter epsilon. Smaller = sharper edges, larger = smoother.
/// `1e-3` on `[0, 1]`-normalised inputs is the He et al. default for
/// matting refinement and matches what most reference implementations
/// (matlab, opencv ximgproc) use.
const GUIDED_EPS: f32 = 1.0e-3;

/// Pick a guided-filter radius that scales with image resolution. SAM2's
/// 256→source upsample puts the dithered logit boundary at ~`max(w,h)/256`
/// source pixels — we want a kernel a few times that wide so the filter
/// can pull boundary mid-alphas onto a real image edge. Kept small at
/// thumbnail sizes (≤16) so the matte doesn't bleed across thin features
/// like fingers.
fn guided_radius_for(w: u32, h: u32) -> u32 {
    let edge = w.min(h) as f32;
    ((edge / 160.0).round() as u32).clamp(4, 24)
}

/// Edge-aware refinement of a SAM2 alpha matte using the source RGB
/// image as the guidance signal. Implements He, Sun & Tang's guided
/// filter (CVPR 2010, §3.1) in scalar luminance form: a single linear
/// model `q = a·I + b` per local window, with a / b solved by minimising
/// `(q − p)² + ε·a²` over each window then averaged across overlapping
/// windows. SAM2's 256×256 logit grid bilinearly upsamples into a
/// halftone-looking boundary on soft cloth (e.g. an orange dhoti); the
/// guided filter snaps those mid-alphas onto actual image edges.
///
/// Cost: six O(N) box filters via summed-area tables, no FFT, no
/// per-pixel kernel sweep. Roughly 50 ms on a 1280×853 thumbnail in
/// debug builds. No-ops on size mismatch / radius 0.
fn refine_alpha_with_guided_filter(alpha: &mut [u8], source: &RgbImage, radius: u32, eps: f32) {
    let (sw, sh) = source.dimensions();
    let n = (sw as usize) * (sh as usize);
    if alpha.len() != n || radius == 0 || sw < 2 || sh < 2 {
        return;
    }

    // Guidance: rec.709 luminance, normalised to [0, 1].
    let mut i_buf = vec![0.0_f32; n];
    for (idx, p) in source.pixels().enumerate() {
        i_buf[idx] =
            (0.2126 * p.0[0] as f32 + 0.7152 * p.0[1] as f32 + 0.0722 * p.0[2] as f32) / 255.0;
    }
    // Mask: alpha to [0, 1].
    let p_buf: Vec<f32> = alpha.iter().map(|&a| a as f32 / 255.0).collect();

    let w = sw as usize;
    let h = sh as usize;
    let r = radius as usize;

    let mean_i = box_filter_mean(&i_buf, w, h, r);
    let mean_p = box_filter_mean(&p_buf, w, h, r);
    let ii: Vec<f32> = i_buf.iter().map(|x| x * x).collect();
    let ip: Vec<f32> = i_buf.iter().zip(&p_buf).map(|(i, p)| i * p).collect();
    let mean_ii = box_filter_mean(&ii, w, h, r);
    let mean_ip = box_filter_mean(&ip, w, h, r);

    let mut a = vec![0.0_f32; n];
    let mut b = vec![0.0_f32; n];
    for k in 0..n {
        let var_i = (mean_ii[k] - mean_i[k] * mean_i[k]).max(0.0);
        let cov_ip = mean_ip[k] - mean_i[k] * mean_p[k];
        let ak = cov_ip / (var_i + eps);
        a[k] = ak;
        b[k] = mean_p[k] - ak * mean_i[k];
    }
    let mean_a = box_filter_mean(&a, w, h, r);
    let mean_b = box_filter_mean(&b, w, h, r);

    for k in 0..n {
        let q = mean_a[k] * i_buf[k] + mean_b[k];
        alpha[k] = (q.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// Mean over a square `(2r+1)×(2r+1)` box, computed in O(N) via a
/// summed-area table (a.k.a. integral image). The SAT itself is O(N) to
/// build and queries the mean of any rectangle in O(1). Edge windows are
/// clipped to the image so the divisor matches the actual sampled area
/// — no replicate/reflect padding step needed.
fn box_filter_mean(input: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if w == 0 || h == 0 || input.len() < w * h {
        return Vec::new();
    }
    let stride = w + 1;
    let mut sat = vec![0.0_f64; stride * (h + 1)];
    for y in 0..h {
        let mut row_sum = 0.0_f64;
        for x in 0..w {
            row_sum += input[y * w + x] as f64;
            sat[(y + 1) * stride + (x + 1)] = sat[y * stride + (x + 1)] + row_sum;
        }
    }
    let mut out = vec![0.0_f32; w * h];
    for y in 0..h {
        let y0 = y.saturating_sub(r);
        let y1 = (y + r + 1).min(h);
        for x in 0..w {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r + 1).min(w);
            let area = ((y1 - y0) * (x1 - x0)) as f64;
            let s = sat[y1 * stride + x1] - sat[y1 * stride + x0] - sat[y0 * stride + x1]
                + sat[y0 * stride + x0];
            out[y * w + x] = (s / area.max(1.0)) as f32;
        }
    }
    out
}

fn preprocess_encoder(img: &RgbImage) -> AppResult<Vec<f32>> {
    let (orig_w, orig_h) = img.dimensions();
    let scale = ENCODER_SIZE as f32 / orig_w.max(orig_h) as f32;
    let new_w = ((orig_w as f32 * scale).round() as u32).min(ENCODER_SIZE);
    let new_h = ((orig_h as f32 * scale).round() as u32).min(ENCODER_SIZE);
    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Triangle);
    let cs = ENCODER_SIZE as usize;
    let mut pixels = vec![0.0_f32; 3 * cs * cs];
    for c in 0..3usize {
        let black = (0.0 - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
        pixels[c * cs * cs..(c + 1) * cs * cs].fill(black);
    }
    for y in 0..new_h as usize {
        for x in 0..new_w as usize {
            let p = resized.get_pixel(x as u32, y as u32).0;
            for c in 0..3usize {
                pixels[c * cs * cs + y * cs + x] =
                    (p[c] as f32 - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
            }
        }
    }
    Ok(pixels)
}

fn mask_confidence(alpha: &[u8]) -> f64 {
    if alpha.is_empty() {
        return 0.0;
    }
    let covered = alpha.iter().filter(|&&a| a > 32).count() as f64 / alpha.len() as f64;
    (0.40 + covered.clamp(0.0, 0.52)).clamp(0.0, 0.93)
}

fn encode_luma_png(width: u32, height: u32, alpha: &[u8]) -> AppResult<String> {
    let expected = (width as usize).saturating_mul(height as usize);
    if alpha.len() != expected {
        return Err(AppError::Internal(
            "sam mask alpha buffer has wrong size".into(),
        ));
    }
    // Encoded as LumaA8 (greyscale + alpha) where the alpha channel carries
    // the mask coverage and the luma channel is constant white. Storing the
    // mask in the *alpha* channel — not luminance — is what makes the
    // frontend's `mask-image: url(...)` overlay work reliably in WebView2:
    // CSS `mask-mode: match-source` defaults to `alpha` for raster sources,
    // so a luminance-only L8 PNG (with no alpha channel) gets a default
    // alpha of 255 across the whole image and the mask covers the entire
    // frame regardless of where the subject is. Encoding the matte into
    // the alpha channel makes the contract unambiguous for every browser.
    let mut interleaved = Vec::with_capacity(expected * 2);
    for &a in alpha {
        interleaved.push(255_u8);
        interleaved.push(a);
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(&interleaved, width, height, image::ExtendedColorType::La8)
        .map_err(|e| AppError::Internal(format!("encode sam mask png: {e}")))?;
    Ok(B64.encode(out))
}

static GLOBAL_SAM: OnceLock<Option<SamSession>> = OnceLock::new();

pub fn init_global_sam_session(encoder_path: Option<&Path>, decoder_path: Option<&Path>) {
    GLOBAL_SAM.get_or_init(|| match (encoder_path, decoder_path) {
        (Some(e), Some(d)) if e.exists() && d.exists() => match SamSession::load(e, d) {
            Ok(sess) => {
                tracing::info!(encoder = %e.display(), decoder = %d.display(), "global SamSession initialised");
                Some(sess)
            }
            Err(err) => {
                tracing::warn!(error = %err, "global SamSession load failed");
                None
            }
        },
        _ => {
            tracing::debug!("global SamSession: model paths absent");
            None
        }
    });
}

pub fn global_sam_session() -> Option<&'static SamSession> {
    GLOBAL_SAM.get().and_then(|o| o.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn stub_session_is_stub() {
        let s = SamSession::load_or_stub(None, None);
        assert!(s.is_stub);
    }

    #[test]
    fn stub_generate_bitmap_mask_returns_not_found() {
        let s = SamSession::load_or_stub(None, None);
        let img = RgbImage::from_pixel(64, 64, Rgb([100, 100, 100]));
        let err = s.generate_bitmap_mask(&img, "subject", &[]).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn invalid_image_size_errors() {
        let s = SamSession {
            encoder: Mutex::new(None),
            decoder: Mutex::new(None),
            is_stub: false,
        };
        let img = RgbImage::new(0, 0);
        let err = s.generate_bitmap_mask(&img, "subject", &[]).unwrap_err();
        assert!(
            matches!(err, AppError::InvalidInput(_)),
            "expected InvalidInput, got: {err:?}"
        );
    }

    #[test]
    fn preprocess_encoder_correct_shape() {
        let img = RgbImage::from_pixel(320, 240, Rgb([128, 64, 32]));
        let pixels = preprocess_encoder(&img).expect("preprocess");
        assert_eq!(
            pixels.len(),
            3 * ENCODER_SIZE as usize * ENCODER_SIZE as usize
        );
    }

    #[test]
    fn preprocess_encoder_values_in_range() {
        let img = RgbImage::from_pixel(64, 64, Rgb([255, 0, 128]));
        let pixels = preprocess_encoder(&img).expect("preprocess");
        for v in &pixels {
            assert!(*v > -3.0 && *v < 3.0, "value {v} out of [-3,3]");
        }
    }

    #[test]
    fn build_prompts_person_with_faces() {
        let hints = [FaceHint {
            x: 0.4,
            y: 0.1,
            w: 0.2,
            h: 0.15,
        }];
        let (coords, labels) = build_prompts("person", &hints, 640, 480);
        assert!(!coords.is_empty());
        assert_eq!(coords.len(), labels.len());
        assert!(labels.contains(&1.0));
        assert!(labels.contains(&0.0));
    }

    #[test]
    fn build_prompts_subject_with_face_anchors_on_face() {
        // Face in the right half — geometric center (0.5, 0.5) is *not*
        // on the subject. Without face-hint anchoring, SAM lands on
        // whatever's at center (rangoli, patterned wall, etc.). With
        // hints the positive prompts move onto the face + torso.
        let hints = [FaceHint {
            x: 0.7,
            y: 0.3,
            w: 0.1,
            h: 0.12,
        }];
        let (coords, labels) = build_prompts("subject", &hints, 1000, 1000);
        let positives: Vec<[f32; 2]> = coords
            .iter()
            .zip(labels.iter())
            .filter(|(_, &l)| l > 0.5)
            .map(|(c, _)| *c)
            .collect();
        assert!(
            !positives.is_empty(),
            "expected at least one positive prompt"
        );
        for p in &positives {
            // Encoder size = 1024; the face is in the right half, so
            // every positive prompt should land in the right half (>512).
            assert!(
                p[0] > 512.0,
                "positive prompt at x={} not in face region",
                p[0]
            );
        }
        // Eight border negatives.
        let neg_count = labels.iter().filter(|&&l| l < 0.5).count();
        assert_eq!(neg_count, 8, "expected 8 border negatives, got {neg_count}");
    }

    #[test]
    fn build_prompts_subject_no_face_uses_center_with_dense_negatives() {
        let (coords, labels) = build_prompts("subject", &[], 1000, 1000);
        let positives = labels.iter().filter(|&&l| l > 0.5).count();
        let negatives = labels.iter().filter(|&&l| l < 0.5).count();
        assert_eq!(positives, 1, "expected single center positive");
        assert_eq!(negatives, 8, "expected 8 border negatives");
        assert_eq!(coords.len(), labels.len());
    }

    #[test]
    fn build_prompts_all_sources_nonempty() {
        for src in [
            "person",
            "subject",
            "sky",
            "foreground",
            "background",
            "landscape",
            "object",
        ] {
            let (c, l) = build_prompts(src, &[], 640, 480);
            assert!(!c.is_empty(), "source={src}");
            assert_eq!(c.len(), l.len(), "source={src}");
        }
    }

    #[test]
    fn threshold_inverts_for_background() {
        // 2×1 mask grid (mw=2, mh=1). Image is square so the encoder
        // letterbox is 1:1 and the mask grid covers the full image.
        let decoded = DecodedMask {
            logits: vec![4.0_f32, -4.0],
            mask_w: 2,
            mask_h: 1,
        };
        let fg = threshold_to_alpha("subject", &decoded, 2, 1);
        let bg = threshold_to_alpha("background", &decoded, 2, 1);
        assert!(fg[0] > 200 && fg[1] < 55);
        assert!(bg[0] < 55 && bg[1] > 200);
    }

    #[test]
    fn threshold_upsamples_small_grid_to_full_image() {
        // 4×4 logit grid covers a 1024×1024 letterboxed encoder space.
        // Original image is 800×800 (square → fills letterbox cleanly):
        // expect a coarse but non-zero alpha across the whole image.
        let mut logits = vec![-10.0_f32; 16];
        for i in 0..4 {
            logits[i * 4 + i] = 10.0;
        }
        let decoded = DecodedMask {
            logits,
            mask_w: 4,
            mask_h: 4,
        };
        let alpha = threshold_to_alpha("subject", &decoded, 800, 800);
        assert_eq!(alpha.len(), 800 * 800);
        let hot = alpha.iter().filter(|&&a| a > 200).count();
        assert!(hot > 0, "expected some hot diagonal pixels");
        let cold = alpha.iter().filter(|&&a| a < 32).count();
        assert!(cold > 0, "expected some cold off-diagonal pixels");
    }

    #[test]
    fn threshold_skips_letterbox_padding_for_landscape_image() {
        // Wide image: 200×100. After letterboxing (longest side → 1024),
        // pixels live in y ∈ [0, 512] of the 1024-tall letterbox; the
        // bottom half is black padding. We should NEVER sample logits
        // from the padded region for any visible image pixel.
        let mut logits = vec![-10.0_f32; 16 * 16];
        // Mark the bottom half of the logit grid hot (the padded region).
        for y in 8..16 {
            for x in 0..16 {
                logits[y * 16 + x] = 10.0;
            }
        }
        let decoded = DecodedMask {
            logits,
            mask_w: 16,
            mask_h: 16,
        };
        let alpha = threshold_to_alpha("subject", &decoded, 200, 100);
        // Image only maps to the top 1024×512 of the letterbox → top
        // half of the logit grid → all logits read should be cold.
        let hot = alpha.iter().filter(|&&a| a > 200).count();
        assert_eq!(
            hot, 0,
            "no visible pixel should sample from the padded region of the logit grid"
        );
    }

    #[test]
    fn encode_luma_png_carries_mask_in_alpha_channel() {
        // The mask matte must live in the *alpha* channel — not luminance —
        // so the frontend's CSS `mask-image` overlay reads it correctly in
        // WebView2. If we slipped back to L8 the mask would cover the whole
        // frame regardless of subject position.
        let alpha: Vec<u8> = (0..16_u8).collect();
        let b64 = encode_luma_png(4, 4, &alpha).expect("encode");
        let bytes = B64.decode(&b64).expect("b64 decode");
        let img = image::load_from_memory(&bytes).expect("png decode");
        let color = img.color();
        assert!(
            color.has_alpha(),
            "encoded mask PNG must have an alpha channel, got {color:?}"
        );
        let rgba = img.to_rgba8();
        assert_eq!(rgba.dimensions(), (4, 4));
        for (i, p) in rgba.pixels().enumerate() {
            assert_eq!(
                p.0[3], alpha[i],
                "alpha channel at index {i} must equal source mask",
            );
        }
    }

    #[test]
    fn build_prompts_subject_with_face_emits_box_labels() {
        // Box prompts (label 2.0 = TL, 3.0 = BR) are the canonical SAM2
        // "mask the object inside this rectangle" hint and the most
        // accurate way to anchor the subject mask. Regression: ensure
        // the subject path doesn't slip back to point-only prompts.
        let hints = [FaceHint {
            x: 0.6,
            y: 0.4,
            w: 0.08,
            h: 0.10,
        }];
        let (_coords, labels) = build_prompts("subject", &hints, 1280, 853);
        let tl_count = labels.iter().filter(|&&l| (l - 2.0).abs() < 1e-6).count();
        let br_count = labels.iter().filter(|&&l| (l - 3.0).abs() < 1e-6).count();
        assert_eq!(
            tl_count, 1,
            "subject+face must emit exactly one box top-left, got {tl_count}"
        );
        assert_eq!(
            br_count, 1,
            "subject+face must emit exactly one box bottom-right, got {br_count}"
        );
    }

    #[test]
    fn background_prompts_match_subject_prompts() {
        // Background / landscape masks invert the subject-mask logits;
        // they MUST therefore use the same prompt set so the model
        // produces a clean subject blob to invert. If background falls
        // back to generic image-center prompts the inversion leaks onto
        // the actual subject (the symptom: green tint covering the baby
        // in addition to the background).
        let hints = [FaceHint {
            x: 0.55,
            y: 0.40,
            w: 0.10,
            h: 0.13,
        }];
        let (subj_c, subj_l) = build_prompts("subject", &hints, 1280, 853);
        for source in &["background", "landscape"] {
            let (c, l) = build_prompts(source, &hints, 1280, 853);
            assert_eq!(
                c, subj_c,
                "{source} coords must match subject coords (logit inversion happens in threshold_to_alpha, not here)"
            );
            assert_eq!(l, subj_l, "{source} labels must match subject labels");
        }
    }

    #[test]
    fn build_prompts_subject_no_face_emits_no_box() {
        // Without a face hint we have no anchor for a sensible box,
        // so fall back to a single centered positive + dense negatives.
        let (_coords, labels) = build_prompts("subject", &[], 1280, 853);
        assert!(
            !labels.iter().any(|&l| l == 2.0 || l == 3.0),
            "no-face fallback must not emit box labels"
        );
    }

    #[test]
    fn primary_face_hint_picks_largest_by_area() {
        // The smallest face is sometimes a false positive (random
        // pareidolia on a textured background) — anchoring on it would
        // mask the wrong region. Always anchor on the largest detected
        // face so the box prompt covers the actual subject.
        let hints = [
            FaceHint {
                x: 0.05,
                y: 0.05,
                w: 0.04,
                h: 0.05,
            }, // tiny pareidolia
            FaceHint {
                x: 0.55,
                y: 0.40,
                w: 0.20,
                h: 0.25,
            }, // real subject
        ];
        let f = primary_face_hint(&hints);
        assert!(
            (f.x - 0.55).abs() < 1e-6,
            "expected the larger face to be picked, got x={}",
            f.x
        );
    }

    #[test]
    fn box_filter_mean_of_constant_input_is_constant() {
        // SAT regression: a uniform input must come out uniform after
        // box filtering, regardless of radius or window-clipping at the
        // image border.
        let w = 32;
        let h = 24;
        let input = vec![0.42_f32; w * h];
        for &r in &[1_usize, 4, 12] {
            let out = box_filter_mean(&input, w, h, r);
            assert_eq!(out.len(), w * h);
            for v in &out {
                assert!(
                    (v - 0.42).abs() < 1e-5,
                    "box filter changed a constant: got {v} at r={r}"
                );
            }
        }
    }

    #[test]
    fn guided_filter_preserves_alpha_inside_uniform_image() {
        // Guidance with no edges → guided filter falls back to a plain
        // box average of the input mask. Mid-mask pixels stay close to
        // their pre-filter value; only edges shift.
        let img = RgbImage::from_pixel(64, 64, Rgb([120, 120, 120]));
        let mut alpha = vec![255_u8; 64 * 64];
        // Carve out a 16×16 black hole in the middle so there's
        // something for the filter to shift around.
        for y in 24..40 {
            for x in 24..40 {
                alpha[y * 64 + x] = 0;
            }
        }
        let before_center = alpha[32 * 64 + 32];
        let before_far_corner = alpha[2 * 64 + 2];
        refine_alpha_with_guided_filter(&mut alpha, &img, 4, GUIDED_EPS);
        // Far corner well outside the kernel of the hole should remain ~255.
        assert!(
            alpha[2 * 64 + 2] > 240,
            "far-corner alpha {} drifted from {before_far_corner}",
            alpha[2 * 64 + 2]
        );
        // Center of the hole should remain ~0 (a few pixels deep into
        // the hole, the kernel sees only black input).
        assert!(
            alpha[32 * 64 + 32] < 64,
            "hole-center alpha {} drifted from {before_center}",
            alpha[32 * 64 + 32]
        );
    }

    #[test]
    fn guided_filter_snaps_alpha_to_image_edge() {
        // Half-black / half-white image with a fuzzy mid-alpha gradient
        // straddling the colour boundary. The guided filter should
        // sharpen the alpha back onto the image edge — black-side pixels
        // pulled to ~0, white-side pulled to ~255.
        let w = 64;
        let h = 32;
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 20 } else { 230 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let mut alpha = vec![0_u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                // Linear ramp 0..255 across the whole width — alpha is
                // intentionally misaligned with the image edge at x=32.
                let v = ((x as f32) / (w as f32 - 1.0) * 255.0) as u8;
                alpha[(y * w + x) as usize] = v;
            }
        }
        refine_alpha_with_guided_filter(&mut alpha, &img, 4, GUIDED_EPS);
        // Sample a few rows on each side, well away from the boundary.
        let mid_y = (h / 2) as usize;
        let left = alpha[mid_y * w as usize + 4];
        let right = alpha[mid_y * w as usize + (w as usize - 5)];
        assert!(
            left < 96,
            "expected left side to be pulled toward black, got {left}"
        );
        assert!(
            right > 160,
            "expected right side to be pulled toward white, got {right}"
        );
    }

    #[test]
    fn guided_radius_for_clamps_to_sane_bounds() {
        // Tiny thumbnail → clamp to minimum 4 to avoid degenerate
        // single-pixel boxes; full-res RAW → clamp to max 24 so the
        // matte doesn't bleed across thin features (fingers, hair).
        assert_eq!(guided_radius_for(64, 64), 4);
        assert_eq!(guided_radius_for(7008, 4672), 24);
    }

    #[test]
    fn resample_logits_is_identity_for_256_grid() {
        // The shipped vietanhdev export already returns 256×256 logits,
        // so the resampler should be the identity on that input —
        // anything else would corrupt the second-pass mask hint.
        let mut logits = vec![0.0_f32; 256 * 256];
        for (i, v) in logits.iter_mut().enumerate() {
            *v = (i as f32) * 0.001;
        }
        let out = resample_logits_to_mask_input(&logits, 256, 256);
        assert_eq!(out.len(), MASK_INPUT_DIM);
        for (i, (o, l)) in out.iter().zip(logits.iter()).enumerate() {
            assert!((o - l).abs() < 1e-4, "resample drifted at {i}: {o} vs {l}",);
        }
    }

    #[test]
    fn resample_logits_handles_non_256_grids() {
        // Defensive: if a future ONNX export emits 1024×1024 (the
        // pre-upsampled mask), the second pass must still receive
        // exactly MASK_INPUT_DIM values without panicking.
        let logits = vec![3.0_f32; 1024 * 1024];
        let out = resample_logits_to_mask_input(&logits, 1024, 1024);
        assert_eq!(out.len(), MASK_INPUT_DIM);
        for v in &out {
            assert!(
                (v - 3.0).abs() < 1e-4,
                "uniform input must produce uniform output, got {v}"
            );
        }
    }
}
