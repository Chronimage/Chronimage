use image_hasher::{HashAlg, HasherConfig};
use std::path::Path;

/// Compute a perceptual hash for the image at `path`.
///
/// Returns a hex-encoded byte string on success, or `None` if the image
/// cannot be opened. Errors are intentionally swallowed — pHash is
/// best-effort.
///
/// `sha256` is an optional cache hint. When provided AND the import
/// pipeline has already seeded the AI-preview cache, we read that JPEG
/// instead of re-decoding the source — important for HEIC/RAW where
/// the source decoder is slow. When `None`, falls back to `open_any`.
pub fn compute(path: &Path, sha256: Option<&str>) -> Option<String> {
    let img = crate::ai::image_util::open_for_ai(path, sha256).ok()?;
    // Median + DCT preprocessing = pHash equivalent in image_hasher.
    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::Median)
        .preproc_dct()
        .to_hasher();
    let hash = hasher.hash_image(&img);
    Some(hex::encode(hash.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_returns_none() {
        assert!(compute(Path::new("/nonexistent/file.jpg"), None).is_none());
    }
}
