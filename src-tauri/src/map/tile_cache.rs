//! Phase 4 §5 — local OSM tile cache.
//!
//! Leaflet renders tiles by setting `<img src="…">`. Without caching
//! every map zoom hammers `tile.openstreetmap.org`, which violates OSMF
//! fair-use policy. This module gives us a Rust-side tile fetcher that
//! reads from `{thumbnails_dir}/../tiles/{z}/{x}/{y}.png`, falling
//! through to HTTPS on miss. The frontend calls via the `map_tile`
//! command and feeds the returned bytes into a custom Leaflet tile
//! loader (blob URL).

use crate::{AppError, AppResult};
use std::time::Duration;

/// Matches Leaflet's slippy-map scheme — (z, x, y) are TMS tile coords.
/// z ≤ 19 is the OSM standard zoom cap.
fn validate(z: u32, x: u32, y: u32) -> AppResult<()> {
    if z > 19 {
        return Err(AppError::InvalidInput(format!(
            "tile zoom {z} out of range"
        )));
    }
    let limit = 1u32 << z;
    if x >= limit || y >= limit {
        return Err(AppError::InvalidInput(format!(
            "tile ({z},{x},{y}) out of range"
        )));
    }
    Ok(())
}

fn tile_path(z: u32, x: u32, y: u32) -> AppResult<std::path::PathBuf> {
    let data = crate::util::paths::thumbnails_dir()?;
    // Sibling directory of the thumbnails cache so we don't pollute
    // the photo-thumbs folder.
    let mut p = data.parent().map(|p| p.to_path_buf()).unwrap_or(data);
    p.push("tiles");
    p.push(z.to_string());
    p.push(x.to_string());
    p.push(format!("{y}.png"));
    Ok(p)
}

/// Read bytes for a single tile — cache first, network fallback. New
/// tiles are written to disk with best-effort semantics (write errors
/// only log).
///
/// Network fetch is user-initiated in the sense that the user opened
/// the map; gated behind this function only to keep non-map surfaces
/// from accidentally triggering OSM requests.
pub async fn user_initiated_fetch_tile(z: u32, x: u32, y: u32) -> AppResult<Vec<u8>> {
    validate(z, x, y)?;
    let path = tile_path(z, x, y)?;
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }

    let url = format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        // OSM fair-use: every request must carry a UA identifying the app.
        .user_agent("Chronimage/0.1 (https://github.com/Chronimage/Chronimage)")
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("tile fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "OSM returned {} for tile ({z},{x},{y})",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("tile body read failed: {e}")))?
        .to_vec();

    // Best-effort cache write.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, &bytes) {
        tracing::warn!(error = %e, "tile cache write failed");
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_out_of_range_tiles() {
        assert!(validate(2, 4, 0).is_err(), "x=4 at zoom 2 exceeds 2^2=4");
        assert!(validate(2, 3, 0).is_ok());
        assert!(validate(20, 0, 0).is_err());
        assert!(validate(19, 0, 0).is_ok());
    }
}
