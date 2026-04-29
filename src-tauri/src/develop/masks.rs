//! Persistent local-adjustment mask layers.
//!
//! Mask rows are metadata plus a JSON payload. Raster brush masks should store
//! sidecar file references in `mask_payload` (`payload_storage = "file"`), while
//! gradients and prompt masks can stay inline until they become large.

use super::ops::Operations;
use crate::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{imageops, RgbImage};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

const VALID_SOURCES: &[&str] = &[
    "brush",
    "linear_gradient",
    "radial_gradient",
    "prompt",
    "subject",
    "sky",
    "background",
    "foreground",
    "object",
    "person",
    "landscape",
    "color_range",
    "luminance_range",
    "depth_range",
];

const VALID_MODES: &[&str] = &["normal", "add", "subtract", "intersect"];
const VALID_STORAGE: &[&str] = &["inline", "file"];

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DevelopMask {
    pub id: i64,
    pub photo_id: i64,
    pub edit_id: Option<i64>,
    pub name: String,
    pub source: String,
    pub mode: String,
    pub visible: bool,
    pub order_index: i64,
    pub payload_storage: String,
    pub mask_payload: String,
    pub operations_json: String,
    pub confidence: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

impl DevelopMask {
    pub fn operations(&self) -> AppResult<Operations> {
        serde_json::from_str(&self.operations_json).map_err(AppError::from)
    }

    pub fn payload(&self) -> AppResult<serde_json::Value> {
        serde_json::from_str(&self.mask_payload).map_err(AppError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopMaskCreateRequest {
    pub photo_id: i64,
    pub edit_id: Option<i64>,
    pub name: Option<String>,
    pub source: String,
    pub mode: Option<String>,
    pub visible: Option<bool>,
    pub order_index: Option<i64>,
    pub payload_storage: Option<String>,
    pub mask_payload: serde_json::Value,
    pub operations: Operations,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopMaskUpdateRequest {
    pub mask_id: i64,
    pub name: Option<String>,
    pub source: Option<String>,
    pub mode: Option<String>,
    pub visible: Option<bool>,
    pub order_index: Option<i64>,
    pub payload_storage: Option<String>,
    pub mask_payload: Option<serde_json::Value>,
    pub operations: Option<Operations>,
    pub confidence: Option<f64>,
}

pub async fn list(pool: &SqlitePool, photo_id: i64) -> AppResult<Vec<DevelopMask>> {
    sqlx::query_as::<_, DevelopMask>(
        "SELECT id, photo_id, edit_id, name, source, mode, visible, order_index, \
         payload_storage, mask_payload, operations_json, confidence, created_at, updated_at \
         FROM develop_masks WHERE photo_id = ?1 ORDER BY order_index ASC, id ASC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn list_visible(pool: &SqlitePool, photo_id: i64) -> AppResult<Vec<DevelopMask>> {
    sqlx::query_as::<_, DevelopMask>(
        "SELECT id, photo_id, edit_id, name, source, mode, visible, order_index, \
         payload_storage, mask_payload, operations_json, confidence, created_at, updated_at \
         FROM develop_masks \
         WHERE photo_id = ?1 AND visible = 1 ORDER BY order_index ASC, id ASC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn create(pool: &SqlitePool, req: DevelopMaskCreateRequest) -> AppResult<i64> {
    validate_photo(pool, req.photo_id).await?;
    if let Some(edit_id) = req.edit_id {
        validate_edit_belongs_to_photo(pool, edit_id, req.photo_id).await?;
    }

    let source = validate_member("source", &req.source, VALID_SOURCES)?;
    let mode = validate_member("mode", req.mode.as_deref().unwrap_or("normal"), VALID_MODES)?;
    let payload_storage = validate_member(
        "payload_storage",
        req.payload_storage.as_deref().unwrap_or("inline"),
        VALID_STORAGE,
    )?;
    let confidence = validate_confidence(req.confidence)?;
    let order_index = match req.order_index {
        Some(i) => i,
        None => next_order_index(pool, req.photo_id).await?,
    };
    let name = clean_name(req.name.as_deref(), source);
    let mask_payload = serde_json::to_string(&req.mask_payload)?;
    let operations_json = serde_json::to_string(&req.operations)?;
    let now = chrono::Utc::now().to_rfc3339();

    let id = sqlx::query_scalar(
        "INSERT INTO develop_masks \
         (photo_id, edit_id, name, source, mode, visible, order_index, payload_storage, \
          mask_payload, operations_json, confidence, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12) \
         RETURNING id",
    )
    .bind(req.photo_id)
    .bind(req.edit_id)
    .bind(name)
    .bind(source)
    .bind(mode)
    .bind(req.visible.unwrap_or(true))
    .bind(order_index)
    .bind(payload_storage)
    .bind(mask_payload)
    .bind(operations_json)
    .bind(confidence)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn update(pool: &SqlitePool, req: DevelopMaskUpdateRequest) -> AppResult<DevelopMask> {
    let current = get(pool, req.mask_id).await?;
    let source = match req.source {
        Some(ref s) => validate_member("source", s, VALID_SOURCES)?.to_string(),
        None => current.source.clone(),
    };
    let mode = match req.mode {
        Some(ref s) => validate_member("mode", s, VALID_MODES)?.to_string(),
        None => current.mode.clone(),
    };
    let payload_storage = match req.payload_storage {
        Some(ref s) => validate_member("payload_storage", s, VALID_STORAGE)?.to_string(),
        None => current.payload_storage.clone(),
    };
    let confidence = validate_confidence(req.confidence.or(current.confidence))?;
    let mask_payload = match req.mask_payload {
        Some(ref payload) => serde_json::to_string(payload)?,
        None => current.mask_payload.clone(),
    };
    let operations_json = match req.operations {
        Some(ref ops) => serde_json::to_string(ops)?,
        None => current.operations_json.clone(),
    };
    let name = match req.name {
        Some(ref name) => clean_name(Some(name), &source),
        None => current.name.clone(),
    };
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE develop_masks SET \
           name = ?1, source = ?2, mode = ?3, visible = ?4, order_index = ?5, \
           payload_storage = ?6, mask_payload = ?7, operations_json = ?8, \
           confidence = ?9, updated_at = ?10 \
         WHERE id = ?11",
    )
    .bind(name)
    .bind(source)
    .bind(mode)
    .bind(req.visible.unwrap_or(current.visible))
    .bind(req.order_index.unwrap_or(current.order_index))
    .bind(payload_storage)
    .bind(mask_payload)
    .bind(operations_json)
    .bind(confidence)
    .bind(now)
    .bind(req.mask_id)
    .execute(pool)
    .await?;

    get(pool, req.mask_id).await
}

pub async fn delete(pool: &SqlitePool, mask_id: i64) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM develop_masks WHERE id = ?1")
        .bind(mask_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub fn apply_mask_layers(
    img: &RgbImage,
    global_ops: &Operations,
    masks: &[DevelopMask],
) -> AppResult<RgbImage> {
    if masks.is_empty() {
        return Ok(crate::develop::pipeline::apply(img, global_ops));
    }

    let (w, h) = img.dimensions();
    let mut base = crate::develop::pipeline::rgb_to_unit_buf(img);
    crate::develop::pipeline::apply_to_unit_buf(&mut base, w as usize, h as usize, global_ops);

    let mut layers: Vec<(Operations, Vec<f32>)> = Vec::new();
    let mut coverage = vec![0.0_f32; (w as usize).saturating_mul(h as usize)];

    for mask in masks {
        if !mask.visible {
            continue;
        }
        let ops = mask.operations()?;
        if ops.is_identity() {
            continue;
        }
        let alpha = rasterize_mask(mask, Some(img), w as usize, h as usize)?;
        compose_mask_layer(&mut layers, &mut coverage, ops, alpha, &mask.mode);
    }

    for (ops, alpha) in layers {
        if alpha.iter().all(|a| *a <= 0.0) {
            continue;
        }
        let mut adjusted = base.clone();
        crate::develop::pipeline::apply_to_unit_buf(&mut adjusted, w as usize, h as usize, &ops);
        if ops.lens_blur_amount > 0.0 {
            let adjusted_rgb = crate::develop::pipeline::unit_buf_to_rgb(w, h, &adjusted, img);
            let adjusted_spatial =
                crate::develop::pipeline::apply_local_spatial(adjusted_rgb, &ops);
            adjusted = crate::develop::pipeline::rgb_to_unit_buf(&adjusted_spatial);
        }
        blend_masked(&mut base, &adjusted, &alpha);
    }

    let out = crate::develop::pipeline::unit_buf_to_rgb(w, h, &base, img);
    Ok(crate::develop::pipeline::apply_spatial(out, global_ops))
}

/// Rasterise a `DevelopMask` to a per-pixel `[0, 1]` alpha matte at
/// `(w, h)`.
///
/// The optional `source_img` is the photo the mask is being applied
/// to — passing it lets pixel-driven mask kinds (`color_range`,
/// `luminance_range`) sample the actual image data. `None` is
/// supported for callers that only have geometric masks (gradients,
/// bitmaps); pixel-driven kinds fall back to a fully-transparent
/// matte in that case so missing context doesn't surface as an error.
pub fn rasterize_mask(
    mask: &DevelopMask,
    source_img: Option<&RgbImage>,
    w: usize,
    h: usize,
) -> AppResult<Vec<f32>> {
    let payload = mask.payload()?;
    rasterize_payload(&payload, source_img, w, h)
}

fn rasterize_payload(
    payload: &serde_json::Value,
    source_img: Option<&RgbImage>,
    w: usize,
    h: usize,
) -> AppResult<Vec<f32>> {
    if w == 0 || h == 0 {
        return Ok(Vec::new());
    }
    let kind = payload
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("full");
    let mut alpha = vec![0.0_f32; w * h];
    match kind {
        "bitmap" => return rasterize_bitmap_payload(payload, w, h),
        "color_range" => {
            rasterize_color_range_payload(payload, source_img, w, h, &mut alpha);
        }
        "luminance_range" => {
            rasterize_luminance_range_payload(payload, source_img, w, h, &mut alpha);
        }
        "full" | "all" | "prompt" => {
            alpha.fill(1.0);
        }
        "sky" => {
            for y in 0..h {
                let yn = y as f32 / (h.saturating_sub(1).max(1) as f32);
                let a = 1.0 - smoothstep(0.18, 0.62, yn);
                for x in 0..w {
                    alpha[y * w + x] = a;
                }
            }
        }
        "subject" | "person" | "object" => {
            let radius_x = if kind == "person" { 0.23 } else { 0.32 };
            let radius_y = if kind == "person" { 0.42 } else { 0.36 };
            rasterize_ellipse(&mut alpha, w, h, (0.5, 0.55), (radius_x, radius_y), 0.28);
        }
        "foreground" => {
            for y in 0..h {
                let yn = y as f32 / (h.saturating_sub(1).max(1) as f32);
                let a = smoothstep(0.34, 0.92, yn);
                for x in 0..w {
                    alpha[y * w + x] = a;
                }
            }
        }
        "background" | "landscape" => {
            let mut subject = vec![0.0_f32; w * h];
            rasterize_ellipse(&mut subject, w, h, (0.5, 0.55), (0.32, 0.36), 0.28);
            for (dst, subj) in alpha.iter_mut().zip(subject) {
                *dst = (1.0 - subj).clamp(0.0, 1.0);
            }
        }
        "linear_gradient" | "gradient" => {
            let top = json_f32(payload, "top").unwrap_or(0.0);
            let bottom = json_f32(payload, "bottom").unwrap_or(1.0);
            let lo = top.min(bottom);
            let hi = top.max(bottom).max(lo + 1e-6);
            for y in 0..h {
                let yn = y as f32 / (h.saturating_sub(1).max(1) as f32);
                let mut a = ((yn - lo) / (hi - lo)).clamp(0.0, 1.0);
                if bottom < top {
                    a = 1.0 - a;
                }
                for x in 0..w {
                    alpha[y * w + x] = a;
                }
            }
        }
        "radial_gradient" | "radial" => {
            let cx = json_f32(payload, "cx").unwrap_or(0.5);
            let cy = json_f32(payload, "cy").unwrap_or(0.5);
            let radius = json_f32(payload, "radius").unwrap_or(0.35).max(1e-4);
            let feather = json_f32(payload, "feather").unwrap_or(0.25).clamp(0.0, 1.0);
            rasterize_ellipse(&mut alpha, w, h, (cx, cy), (radius, radius), feather);
        }
        "brush" => {
            let cx = json_f32(payload, "cx").unwrap_or(0.5);
            let cy = json_f32(payload, "cy").unwrap_or(0.5);
            let radius = json_f32(payload, "radius").unwrap_or(0.18).max(1e-4);
            let feather = json_f32(payload, "feather").unwrap_or(0.45).clamp(0.0, 1.0);
            let density = json_f32(payload, "density").unwrap_or(1.0).clamp(0.0, 1.0);
            rasterize_ellipse(&mut alpha, w, h, (cx, cy), (radius, radius), feather);
            for a in &mut alpha {
                *a *= density;
            }
        }
        other => {
            return Err(AppError::InvalidInput(format!(
                "unsupported mask payload kind {other}"
            )));
        }
    }
    Ok(alpha)
}

fn rasterize_bitmap_payload(
    payload: &serde_json::Value,
    w: usize,
    h: usize,
) -> AppResult<Vec<f32>> {
    let data_b64 = payload
        .get("data_b64")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::InvalidInput("bitmap mask payload requires data_b64".into()))?;
    let bytes = B64
        .decode(data_b64)
        .map_err(|e| AppError::InvalidInput(format!("invalid bitmap mask base64: {e}")))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| AppError::InvalidInput(format!("invalid bitmap mask image: {e}")))?;
    // Mask matte lives in the alpha channel for LumaA8 / RGBA encodings —
    // see `develop::sam::encode_luma_png`. Older legacy payloads (or any
    // hand-crafted L8 / RGB PNGs) have no alpha; for those we read luma.
    // ColorType-based dispatch keeps both encodings working.
    let color = decoded.color();
    let has_alpha = color.has_alpha();
    let (decoded_w, decoded_h) = (decoded.width(), decoded.height());
    let raw: Vec<u8> = if has_alpha {
        decoded.to_rgba8().pixels().map(|p| p.0[3]).collect()
    } else {
        decoded.to_luma8().into_raw()
    };
    let buf = image::GrayImage::from_raw(decoded_w, decoded_h, raw).ok_or_else(|| {
        AppError::Internal("bitmap mask: gray buffer length mismatch after channel pick".into())
    })?;
    let resized = if buf.width() as usize == w && buf.height() as usize == h {
        buf
    } else {
        imageops::resize(&buf, w as u32, h as u32, imageops::FilterType::Triangle)
    };
    Ok(resized.pixels().map(|p| p.0[0] as f32 / 255.0).collect())
}

/// Rasterise a `color_range` mask: select pixels whose colour distance
/// to a sampled `target_rgb` is within `tolerance`, with a soft
/// `feather` band on the outside.
///
/// Distance is RGB Euclidean in `[0, 1]` linear-ish space — close
/// enough to perceptual ΔE for "select all the orange flowers" without
/// pulling in a full Lab conversion crate. The smoothstep edges are
/// `tolerance·(1 − feather)` (fully selected) and `tolerance·(1 + feather)`
/// (no longer selected) so increasing feather widens the soft band
/// symmetrically without changing the threshold.
///
/// Payload schema:
/// ```json
/// {
///   "kind": "color_range",
///   "target_rgb": [180, 100, 50],   // 0–255 sample from the eyedropper
///   "tolerance": 0.25,              // 0–1, sphere radius in unit RGB
///   "feather":   0.40               // 0–1, soft band as a fraction
/// }
/// ```
fn rasterize_color_range_payload(
    payload: &serde_json::Value,
    source_img: Option<&RgbImage>,
    w: usize,
    h: usize,
    alpha: &mut [f32],
) {
    let Some(src) = source_img else {
        // No source image → mask covers nothing. Caller's choice
        // whether to surface that as an error; we keep it transparent
        // so a missing-context bug looks like "the slider does
        // nothing" rather than "the rasteriser panicked".
        alpha.fill(0.0);
        return;
    };
    let target = json_rgb_array(payload, "target_rgb").unwrap_or([128, 128, 128]);
    let tolerance = json_f32(payload, "tolerance")
        .unwrap_or(0.25)
        .clamp(0.005, 1.732);
    let feather = json_f32(payload, "feather").unwrap_or(0.4).clamp(0.0, 1.0);
    let edge0 = tolerance * (1.0 - feather);
    let edge1 = (tolerance * (1.0 + feather)).max(edge0 + 1e-4);
    let tr = target[0] as f32 / 255.0;
    let tg = target[1] as f32 / 255.0;
    let tb = target[2] as f32 / 255.0;

    let resized;
    let pixels: &[u8] = if src.width() as usize == w && src.height() as usize == h {
        src.as_raw()
    } else {
        // The mask is rasterised at preview size, not source size, so
        // the eyedropper pixel chosen at full resolution still lines up
        // visually after the resize. Triangle filtering is fine here —
        // no need for the better-quality Lanczos because we're going
        // to threshold the result with a smoothstep anyway.
        resized = imageops::resize(src, w as u32, h as u32, imageops::FilterType::Triangle);
        resized.as_raw()
    };
    for k in 0..(w * h) {
        let r = pixels[k * 3] as f32 / 255.0;
        let g = pixels[k * 3 + 1] as f32 / 255.0;
        let b = pixels[k * 3 + 2] as f32 / 255.0;
        let dr = r - tr;
        let dg = g - tg;
        let db = b - tb;
        let d = (dr * dr + dg * dg + db * db).sqrt();
        // 1 inside tolerance, smooth falloff to 0 across the feather band.
        alpha[k] = (1.0 - smoothstep(edge0, edge1, d)).clamp(0.0, 1.0);
    }
}

/// Rasterise a `luminance_range` mask: select pixels whose rec.709
/// luma falls inside `[lo, hi]`, with a `feather` band on each edge.
///
/// Useful for "darken everything below 30% luma" and similar
/// tone-targeted edits without a full curves panel.
///
/// Payload schema:
/// ```json
/// {
///   "kind": "luminance_range",
///   "lo":      0.30,    // 0–1, lower bound (start of full selection)
///   "hi":      0.70,    // 0–1, upper bound (end of full selection)
///   "feather": 0.10     // 0–1, soft band added on each side of [lo, hi]
/// }
/// ```
fn rasterize_luminance_range_payload(
    payload: &serde_json::Value,
    source_img: Option<&RgbImage>,
    w: usize,
    h: usize,
    alpha: &mut [f32],
) {
    let Some(src) = source_img else {
        alpha.fill(0.0);
        return;
    };
    let raw_lo = json_f32(payload, "lo").unwrap_or(0.3).clamp(0.0, 1.0);
    let raw_hi = json_f32(payload, "hi").unwrap_or(0.7).clamp(0.0, 1.0);
    let lo = raw_lo.min(raw_hi);
    let hi = raw_lo.max(raw_hi).max(lo + 1e-4);
    let feather = json_f32(payload, "feather").unwrap_or(0.1).clamp(0.0, 0.5);

    let resized;
    let pixels: &[u8] = if src.width() as usize == w && src.height() as usize == h {
        src.as_raw()
    } else {
        resized = imageops::resize(src, w as u32, h as u32, imageops::FilterType::Triangle);
        resized.as_raw()
    };
    let edge_lo_outer = (lo - feather).max(0.0);
    let edge_hi_outer = (hi + feather).min(1.0);
    for k in 0..(w * h) {
        let r = pixels[k * 3] as f32 / 255.0;
        let g = pixels[k * 3 + 1] as f32 / 255.0;
        let b = pixels[k * 3 + 2] as f32 / 255.0;
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        // Two smoothsteps form a soft window: ramp up from
        // `edge_lo_outer` to `lo`, then back down from `hi` to
        // `edge_hi_outer`. Multiplied so we get 1 only inside
        // `[lo, hi]` and 0 outside the feathered band.
        let rise = smoothstep(edge_lo_outer, lo, luma);
        let fall = 1.0 - smoothstep(hi, edge_hi_outer, luma);
        alpha[k] = (rise * fall).clamp(0.0, 1.0);
    }
}

fn json_rgb_array(payload: &serde_json::Value, key: &str) -> Option<[u8; 3]> {
    let arr = payload.get(key)?.as_array()?;
    if arr.len() != 3 {
        return None;
    }
    let r = arr[0].as_f64()?.clamp(0.0, 255.0) as u8;
    let g = arr[1].as_f64()?.clamp(0.0, 255.0) as u8;
    let b = arr[2].as_f64()?.clamp(0.0, 255.0) as u8;
    Some([r, g, b])
}

fn compose_mask_layer(
    layers: &mut Vec<(Operations, Vec<f32>)>,
    coverage: &mut [f32],
    ops: Operations,
    alpha: Vec<f32>,
    mode: &str,
) {
    match mode {
        "subtract" => {
            for layer in layers.iter_mut() {
                for (dst, a) in layer.1.iter_mut().zip(&alpha) {
                    *dst = (*dst * (1.0 - *a)).clamp(0.0, 1.0);
                }
            }
            for (dst, a) in coverage.iter_mut().zip(&alpha) {
                *dst = (*dst * (1.0 - *a)).clamp(0.0, 1.0);
            }
        }
        "intersect" => {
            for layer in layers.iter_mut() {
                for (dst, a) in layer.1.iter_mut().zip(&alpha) {
                    *dst = (*dst * *a).clamp(0.0, 1.0);
                }
            }
            for (dst, a) in coverage.iter_mut().zip(&alpha) {
                *dst = (*dst * *a).clamp(0.0, 1.0);
            }
        }
        "add" => {
            let mut effective = alpha;
            for (dst, covered) in effective.iter_mut().zip(coverage.iter()) {
                *dst = (*dst * (1.0 - *covered)).clamp(0.0, 1.0);
            }
            for (covered, a) in coverage.iter_mut().zip(&effective) {
                *covered = (*covered + *a).clamp(0.0, 1.0);
            }
            layers.push((ops, effective));
        }
        _ => {
            for (covered, a) in coverage.iter_mut().zip(&alpha) {
                *covered = covered.max(*a).clamp(0.0, 1.0);
            }
            layers.push((ops, alpha));
        }
    }
}

fn rasterize_ellipse(
    alpha: &mut [f32],
    w: usize,
    h: usize,
    center: (f32, f32),
    radii: (f32, f32),
    feather: f32,
) {
    let (cx, cy) = center;
    let (radius_x, radius_y) = radii;
    let inner = (1.0 - feather.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let rx = radius_x.max(1e-4);
    let ry = radius_y.max(1e-4);
    for y in 0..h {
        let yn = y as f32 / (h.saturating_sub(1).max(1) as f32);
        for x in 0..w {
            let xn = x as f32 / (w.saturating_sub(1).max(1) as f32);
            let d = (((xn - cx) / rx).powi(2) + ((yn - cy) / ry).powi(2)).sqrt();
            alpha[y * w + x] = if d <= inner {
                1.0
            } else {
                (1.0 - ((d - inner) / (1.0 - inner).max(1e-6))).clamp(0.0, 1.0)
            };
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn blend_masked(base: &mut [[f32; 3]], adjusted: &[[f32; 3]], alpha: &[f32]) {
    for ((dst, src), a) in base.iter_mut().zip(adjusted).zip(alpha) {
        let a = (*a).clamp(0.0, 1.0);
        dst[0] = dst[0] + (src[0] - dst[0]) * a;
        dst[1] = dst[1] + (src[1] - dst[1]) * a;
        dst[2] = dst[2] + (src[2] - dst[2]) * a;
    }
}

fn json_f32(payload: &serde_json::Value, key: &str) -> Option<f32> {
    payload.get(key)?.as_f64().map(|v| v as f32)
}

pub async fn get(pool: &SqlitePool, mask_id: i64) -> AppResult<DevelopMask> {
    sqlx::query_as::<_, DevelopMask>(
        "SELECT id, photo_id, edit_id, name, source, mode, visible, order_index, \
         payload_storage, mask_payload, operations_json, confidence, created_at, updated_at \
         FROM develop_masks WHERE id = ?1",
    )
    .bind(mask_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("develop mask {mask_id}")))
}

async fn next_order_index(pool: &SqlitePool, photo_id: i64) -> AppResult<i64> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT MAX(order_index) FROM develop_masks WHERE photo_id = ?1")
            .bind(photo_id)
            .fetch_one(pool)
            .await?;
    Ok(current.unwrap_or(-1) + 1)
}

async fn validate_photo(pool: &SqlitePool, photo_id: i64) -> AppResult<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM photos WHERE id = ?1")
        .bind(photo_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("photo {photo_id}")))
    }
}

async fn validate_edit_belongs_to_photo(
    pool: &SqlitePool,
    edit_id: i64,
    photo_id: i64,
) -> AppResult<()> {
    let owner: Option<i64> = sqlx::query_scalar("SELECT photo_id FROM edits WHERE id = ?1")
        .bind(edit_id)
        .fetch_optional(pool)
        .await?;
    match owner {
        Some(owner_photo_id) if owner_photo_id == photo_id => Ok(()),
        Some(_) => Err(AppError::InvalidInput(format!(
            "edit {edit_id} does not belong to photo {photo_id}"
        ))),
        None => Err(AppError::NotFound(format!("edit {edit_id}"))),
    }
}

fn validate_member<'a>(field: &str, value: &'a str, allowed: &[&str]) -> AppResult<&'a str> {
    let trimmed = value.trim();
    if allowed.contains(&trimmed) {
        Ok(trimmed)
    } else {
        Err(AppError::InvalidInput(format!(
            "{field} must be one of {}",
            allowed.join(", ")
        )))
    }
}

fn validate_confidence(confidence: Option<f64>) -> AppResult<Option<f64>> {
    match confidence {
        Some(v) if !(0.0..=1.0).contains(&v) => Err(AppError::InvalidInput(
            "confidence must be between 0 and 1".into(),
        )),
        other => Ok(other),
    }
}

fn clean_name(name: Option<&str>, fallback_source: &str) -> String {
    let trimmed = name.unwrap_or("").trim();
    if trimmed.is_empty() {
        fallback_source.replace('_', " ")
    } else {
        trimmed.chars().take(96).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn seeded_pool() -> SqlitePool {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, 'mask111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 100, 100, '2026-12-02T00:00:00Z', 0)",
        )
        .await
        .expect("seed photo");
        pool
    }

    fn create_req() -> DevelopMaskCreateRequest {
        DevelopMaskCreateRequest {
            photo_id: 1,
            edit_id: None,
            name: Some("Sky".into()),
            source: "sky".into(),
            mode: None,
            visible: None,
            order_index: None,
            payload_storage: None,
            mask_payload: serde_json::json!({ "kind": "gradient", "top": 0.0, "bottom": 0.5 }),
            operations: Operations {
                exposure: 0.5,
                ..Operations::identity()
            },
            confidence: Some(0.9),
        }
    }

    #[tokio::test]
    async fn create_and_list_mask() {
        let pool = seeded_pool().await;
        let id = create(&pool, create_req()).await.expect("create");
        assert!(id > 0);

        let rows = list(&pool, 1).await.expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Sky");
        assert_eq!(rows[0].source, "sky");
        assert!(rows[0].visible);
        assert_eq!(rows[0].operations().expect("ops").exposure, 0.5);
    }

    #[tokio::test]
    async fn update_mask_changes_payload_and_visibility() {
        let pool = seeded_pool().await;
        let id = create(&pool, create_req()).await.expect("create");
        let updated = update(
            &pool,
            DevelopMaskUpdateRequest {
                mask_id: id,
                name: Some("Darken sky".into()),
                source: None,
                mode: Some("normal".into()),
                visible: Some(false),
                order_index: Some(3),
                payload_storage: None,
                mask_payload: Some(serde_json::json!({ "kind": "linear_gradient" })),
                operations: None,
                confidence: None,
            },
        )
        .await
        .expect("update");

        assert_eq!(updated.name, "Darken sky");
        assert!(!updated.visible);
        assert_eq!(updated.order_index, 3);
        assert_eq!(
            updated.payload().expect("payload")["kind"],
            serde_json::json!("linear_gradient")
        );
    }

    #[tokio::test]
    async fn invalid_source_is_rejected() {
        let pool = seeded_pool().await;
        let mut req = create_req();
        req.source = "telepathy".into();
        let err = create(&pool, req).await.expect_err("invalid source");
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn delete_mask_removes_row() {
        let pool = seeded_pool().await;
        let id = create(&pool, create_req()).await.expect("create");
        let deleted = delete(&pool, id).await.expect("delete");
        assert_eq!(deleted, 1);
        assert!(list(&pool, 1).await.expect("list").is_empty());
    }

    #[test]
    fn bitmap_payload_rasterizes_to_alpha() {
        let png_b64 = crate::develop::segmentation::generate_bitmap_mask(
            &RgbImage::from_pixel(4, 4, image::Rgb([40, 40, 40])),
            "subject",
            &[],
        )
        .expect("generated")
        .data_b64;
        let payload = serde_json::json!({
            "kind": "bitmap",
            "width": 4,
            "height": 4,
            "format": "png-luma8",
            "data_b64": png_b64,
        });
        let alpha = rasterize_payload(&payload, None, 4, 4).expect("rasterized");
        assert_eq!(alpha.len(), 16);
        assert!(alpha.iter().all(|a| (0.0..=1.0).contains(a)));
    }

    #[test]
    fn color_range_rasterizer_selects_pixels_close_to_target() {
        // 8×4 image: left half pure red, right half pure blue. A
        // color_range mask targeting red should pick the left half
        // and exclude the right half regardless of feather.
        let mut img = RgbImage::new(8, 4);
        for y in 0..4 {
            for x in 0..8 {
                let c = if x < 4 {
                    image::Rgb([220, 30, 30])
                } else {
                    image::Rgb([30, 30, 220])
                };
                img.put_pixel(x, y, c);
            }
        }
        let payload = serde_json::json!({
            "kind": "color_range",
            "target_rgb": [220, 30, 30],
            "tolerance": 0.2,
            "feather": 0.3,
        });
        let alpha = rasterize_payload(&payload, Some(&img), 8, 4).expect("rasterized");
        assert_eq!(alpha.len(), 32);
        // Left half: high alpha (close to target). Right half: zero.
        for y in 0..4 {
            for x in 0..4 {
                let a = alpha[y * 8 + x];
                assert!(a > 0.95, "left-half pixel ({x},{y}) alpha {a} not solid");
            }
            for x in 4..8 {
                let a = alpha[y * 8 + x];
                assert!(
                    a < 0.05,
                    "right-half pixel ({x},{y}) alpha {a} should be zero"
                );
            }
        }
    }

    #[test]
    fn color_range_rasterizer_returns_zero_alpha_without_source() {
        // Defensive contract: pixel-driven kinds need source pixels.
        // Without them we MUST NOT panic and MUST NOT return a fully
        // selected mask — both surface as user-visible bugs.
        let payload = serde_json::json!({
            "kind": "color_range",
            "target_rgb": [128, 128, 128],
            "tolerance": 0.5,
            "feather": 0.5,
        });
        let alpha = rasterize_payload(&payload, None, 4, 4).expect("rasterized");
        assert_eq!(alpha.len(), 16);
        assert!(alpha.iter().all(|a| *a == 0.0));
    }

    #[test]
    fn luminance_range_rasterizer_picks_band_and_feathers_outside() {
        // Luma ramp 0 → 1 across width. A range mask with [0.3, 0.7]
        // should be ~1 inside that band and ~0 well outside it, with
        // a soft feathered edge in between.
        let w = 100;
        let h = 1;
        let mut img = RgbImage::new(w, h);
        for x in 0..w {
            let v = ((x as f32 / (w - 1) as f32) * 255.0).round() as u8;
            img.put_pixel(x, 0, image::Rgb([v, v, v]));
        }
        let payload = serde_json::json!({
            "kind": "luminance_range",
            "lo": 0.3,
            "hi": 0.7,
            "feather": 0.05,
        });
        let alpha =
            rasterize_payload(&payload, Some(&img), w as usize, h as usize).expect("rasterized");
        // Inside the band: solidly selected.
        for (x, a) in alpha.iter().enumerate().take(60).skip(40) {
            assert!(*a > 0.95, "luma ~{x}/100 alpha {a} not solid");
        }
        // Far below `lo`: not selected.
        for (x, a) in alpha.iter().enumerate().take(15) {
            assert!(*a < 0.05, "luma ~{x}/100 alpha {a} should be zero");
        }
        // Far above `hi`: not selected.
        for (x, a) in alpha.iter().enumerate().take(w as usize).skip(85) {
            assert!(*a < 0.05, "luma ~{x}/100 alpha {a} should be zero");
        }
    }

    #[test]
    fn luminance_range_rasterizer_swaps_inverted_lo_hi() {
        // Defensive: if a UI bug passes lo > hi, the mask must still
        // pick the intended luma band rather than producing nothing.
        let w = 100;
        let h = 1;
        let mut img = RgbImage::new(w, h);
        for x in 0..w {
            let v = ((x as f32 / (w - 1) as f32) * 255.0).round() as u8;
            img.put_pixel(x, 0, image::Rgb([v, v, v]));
        }
        let payload = serde_json::json!({
            "kind": "luminance_range",
            "lo": 0.7,
            "hi": 0.3,
            "feather": 0.0,
        });
        let alpha =
            rasterize_payload(&payload, Some(&img), w as usize, h as usize).expect("rasterized");
        let middle_alpha: f32 = alpha[40..60].iter().sum::<f32>() / 20.0;
        assert!(
            middle_alpha > 0.9,
            "swapped lo/hi must still select the [0.3, 0.7] band, got mean {middle_alpha}"
        );
    }

    #[test]
    fn subtract_and_intersect_modify_existing_layers() {
        let ops = Operations {
            exposure: 0.5,
            ..Operations::identity()
        };
        let mut layers = Vec::new();
        let mut coverage = vec![0.0; 4];
        compose_mask_layer(
            &mut layers,
            &mut coverage,
            ops.clone(),
            vec![1.0, 1.0, 0.0, 0.0],
            "normal",
        );
        compose_mask_layer(
            &mut layers,
            &mut coverage,
            ops.clone(),
            vec![0.0, 1.0, 0.0, 0.0],
            "subtract",
        );
        assert_eq!(layers[0].1, vec![1.0, 0.0, 0.0, 0.0]);

        compose_mask_layer(
            &mut layers,
            &mut coverage,
            ops,
            vec![1.0, 0.0, 1.0, 0.0],
            "intersect",
        );
        assert_eq!(layers[0].1, vec![1.0, 0.0, 0.0, 0.0]);
    }
}
