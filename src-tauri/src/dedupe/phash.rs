use image_hasher::{HashAlg, HasherConfig};
use std::path::Path;

/// Compute a perceptual hash for the image at `path`.
///
/// Returns a hex-encoded byte string on success, or `None` if the image
/// cannot be opened (RAW files, corrupt data, unsupported format, etc.).
/// Errors are intentionally swallowed — pHash is best-effort.
pub fn compute(path: &Path) -> Option<String> {
    let img = image::open(path).ok()?;
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
        assert!(compute(Path::new("/nonexistent/file.jpg")).is_none());
    }
}
