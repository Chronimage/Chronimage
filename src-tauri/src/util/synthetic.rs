//! Synthetic photo fixture generation — dev / e2e-test only.
//!
//! Debug-only because shipping a "generate 100k fake photos" code path in a
//! signed release binary would be a liability. `cfg(debug_assertions)` in
//! `util/mod.rs` + matching cfg on the Tauri command ensures this compiles
//! away completely in release builds.
//!
//! The byte pattern mirrors `src-tauri/tests/phase_1_catalog_size.rs` exactly
//! — a 512×512 JPEG with per-pixel LCG noise, ~50 KB encoded. Deterministic
//! per `index` so re-runs produce byte-identical bytes.

use image::{ImageBuffer, Rgb};

/// Edge length of every synthetic fixture JPEG.
pub const FIXTURE_SIZE: u32 = 512;

/// JPEG quality for the synthetic fixture. Matches the catalog-size test's
/// quality setting so ratio math stays comparable.
pub const FIXTURE_QUALITY: u8 = 80;

/// Produce a deterministic, JPEG-encoded, RGB8 512×512 photo for `index`.
///
/// Uses a Knuth-style 64-bit LCG seeded from `index` to fill every pixel
/// with noise — incompressible, so every photo is unique and the compressed
/// output lands in the ~40-80 KB band that matches realistic phone / compact
/// camera output.
pub fn synthesize_jpeg(index: usize) -> Vec<u8> {
    let mut buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(FIXTURE_SIZE, FIXTURE_SIZE);
    let mut state = (index as u64)
        .wrapping_mul(0x5851_F42D_4C95_7F2D)
        .wrapping_add(1);
    for pixel in buf.pixels_mut() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let r = (state >> 40) as u8;
        let g = (state >> 32) as u8;
        let b = (state >> 24) as u8;
        *pixel = Rgb([r, g, b]);
    }
    let mut out = Vec::with_capacity(80_000);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, FIXTURE_QUALITY)
        .encode_image(&buf)
        .unwrap_or_else(|e| {
            // Encoder failure on a 512×512 RGB8 buffer is a programming
            // error, not a runtime condition. Leaving an empty Vec is a
            // safer compromise than panicking in a cfg'd-out module.
            tracing::error!(error = %e, "synthetic jpeg encode failed");
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_nonempty_jpeg() {
        let bytes = synthesize_jpeg(0);
        assert!(
            bytes.len() > 10_000,
            "synthetic JPEG unexpectedly small: {} bytes",
            bytes.len()
        );
        // JPEG SOI marker
        assert_eq!(&bytes[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn different_indices_produce_different_bytes() {
        let a = synthesize_jpeg(0);
        let b = synthesize_jpeg(1);
        assert_ne!(a, b, "different indices produced identical bytes");
    }

    #[test]
    fn same_index_is_deterministic() {
        let a = synthesize_jpeg(42);
        let b = synthesize_jpeg(42);
        assert_eq!(
            a, b,
            "same index produced different bytes — LCG not deterministic"
        );
    }
}
