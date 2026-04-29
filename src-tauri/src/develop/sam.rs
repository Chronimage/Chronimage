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
        let decoded = self.decode(&embeddings, &coords, &labels, orig_h, orig_w)?;
        let alpha = threshold_to_alpha(source, &decoded, orig_w, orig_h);
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
    /// Used by `chronimage-mask-debug` to iterate prompt strategies.
    pub fn decode_normalized(
        &self,
        features: &Sam2Features,
        prompts: &[NormalizedPrompt],
        orig_w: u32,
        orig_h: u32,
        invert: bool,
    ) -> AppResult<GeneratedMask> {
        if self.is_stub {
            return Err(AppError::NotFound(
                "SAM2.1 stub -- models not loaded".into(),
            ));
        }
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
        let decoded = self.decode(features, &coords, &labels, orig_h, orig_w)?;
        let source = if invert { "background" } else { "subject" };
        let alpha = threshold_to_alpha(source, &decoded, orig_w, orig_h);
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

    fn decode(
        &self,
        features: &Sam2Features,
        point_coords: &[[f32; 2]],
        point_labels: &[f32],
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
        let mask_input_tensor = ort::value::Tensor::<f32>::from_array((
            vec![1i64, 1, 256, 256],
            vec![0.0_f32; MASK_INPUT_DIM],
        ))
        .map_err(|e| AppError::Internal(format!("ort tensor (sam-mask-input): {e}")))?;
        let has_mask_tensor = ort::value::Tensor::<f32>::from_array((vec![1i64], vec![0.0_f32]))
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
    match source {
        "person" => {
            if face_hints.is_empty() {
                coords.push(to_px(0.5, 0.45));
                labels.push(1.0);
            } else {
                for face in face_hints {
                    let cx = face.x + face.w * 0.5;
                    let cy = face.y + face.h * 0.5;
                    coords.push(to_px(cx, cy));
                    labels.push(1.0);
                    let torso_y = (face.y + face.h * 3.5).clamp(0.0, 1.0);
                    coords.push(to_px(cx, torso_y));
                    labels.push(1.0);
                }
            }
            coords.push(to_px(0.02, 0.02));
            labels.push(0.0);
            coords.push(to_px(0.98, 0.02));
            labels.push(0.0);
        }
        "subject" | "object" => {
            // If the import pipeline detected a face, treat "subject"
            // like "person" — the face + torso region is almost always
            // what the photographer means. Without this, a single
            // center-point positive lands on whatever sits at (0.5, 0.5),
            // which for off-center compositions is rarely the subject.
            if !face_hints.is_empty() {
                for face in face_hints {
                    let cx = face.x + face.w * 0.5;
                    let cy = face.y + face.h * 0.5;
                    coords.push(to_px(cx, cy));
                    labels.push(1.0);
                    let torso_y = (face.y + face.h * 3.5).clamp(0.0, 1.0);
                    coords.push(to_px(cx, torso_y));
                    labels.push(1.0);
                }
            } else {
                coords.push(to_px(0.5, 0.5));
                labels.push(1.0);
            }
            // Dense border negatives — corners + edge midpoints. Two
            // corner points wasn't enough to stop SAM2 from expanding
            // the mask to fill busy backgrounds (e.g. a rangoli or
            // patterned wall surrounding the subject).
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
                labels.push(0.0);
            }
        }
        "sky" => {
            coords.push(to_px(0.5, 0.05));
            labels.push(1.0);
            coords.push(to_px(0.25, 0.05));
            labels.push(1.0);
            coords.push(to_px(0.75, 0.05));
            labels.push(1.0);
            coords.push(to_px(0.5, 0.85));
            labels.push(0.0);
        }
        "foreground" => {
            coords.push(to_px(0.5, 0.88));
            labels.push(1.0);
            coords.push(to_px(0.2, 0.88));
            labels.push(1.0);
            coords.push(to_px(0.8, 0.88));
            labels.push(1.0);
            coords.push(to_px(0.5, 0.12));
            labels.push(0.0);
        }
        _ => {
            coords.push(to_px(0.5, 0.5));
            labels.push(1.0);
            coords.push(to_px(0.02, 0.02));
            labels.push(0.0);
            coords.push(to_px(0.98, 0.98));
            labels.push(0.0);
        }
    }
    (coords, labels)
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
}
