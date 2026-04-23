//! Small image-processing helpers shared across the pipeline.
//!
//! - [`apply_exif_orientation`] — rotate/flip a `DynamicImage` according to
//!   the TIFF/EXIF Orientation tag (values 1–8 per the spec at
//!   <https://exiftool.org/TagNames/EXIF.html>). The `image` crate does *not*
//!   auto-apply this at decode time, so thumbnails end up in raw sensor
//!   orientation (portrait camera shots display landscape) unless we do it
//!   ourselves. See `import::exif::ExifData::orientation` for the source.
//! - [`laplacian_variance`] — classic focus-detection metric. Higher values
//!   = sharper image. Used in Stage 2.6 of the import pipeline to populate
//!   `photos.sharpness_score`.

use image::{DynamicImage, GrayImage};

/// Apply the EXIF Orientation transform to an image.
///
/// Orientation tag values (TIFF 6.0 spec § 4.2.5):
/// | Value | Transform |
/// |-------|-----------|
/// | 1     | identity (no rotation) |
/// | 2     | flip horizontal |
/// | 3     | rotate 180° |
/// | 4     | flip vertical |
/// | 5     | transpose (flip horizontal + rotate 270° CW) |
/// | 6     | rotate 90° CW (portrait shot on a camera held vertically) |
/// | 7     | transverse (flip horizontal + rotate 90° CW) |
/// | 8     | rotate 270° CW (= 90° CCW) |
///
/// Unknown / missing orientation = treated as 1 (return unchanged).
///
/// Rotations happen in-memory via the `image` crate's `rotate90`/`rotate180`/
/// `rotate270` helpers, which re-encode the pixel buffer with the new
/// dimensions. Cost on a 320 × 320 thumbnail: ~2 ms.
#[must_use]
pub fn apply_exif_orientation(img: DynamicImage, orientation: Option<u32>) -> DynamicImage {
    match orientation.unwrap_or(1) {
        1 => img,
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate270(),
        6 => img.rotate90(),
        7 => img.fliph().rotate90(),
        8 => img.rotate270(),
        _ => img, // unknown value = leave unchanged
    }
}

/// Laplacian variance — classic sharpness / focus metric.
///
/// Convolve the grayscale image with the 3×3 Laplacian kernel
/// `[0 1 0; 1 -4 1; 0 1 0]` and return the variance of the response. Sharper
/// images have stronger edge responses → higher variance. A threshold around
/// 100 (on 8-bit input) reliably separates in-focus from out-of-focus shots
/// on typical consumer photography.
///
/// Input: an RGB image (we convert to luminance internally via `into_luma8`).
/// Cost: ~5–15 ms on a 320 × 320 thumbnail.
#[must_use]
pub fn laplacian_variance(img: &DynamicImage) -> f32 {
    let gray: GrayImage = img.to_luma8();
    let (w, h) = (gray.width() as i32, gray.height() as i32);
    if w < 3 || h < 3 {
        return 0.0;
    }

    // Single-pass Laplacian over interior pixels. Skip the 1-pixel border
    // (standard convention — kernel doesn't fit there).
    let stride = w as usize;
    let pixels = gray.as_raw();
    let interior = ((w - 2) * (h - 2)) as usize;
    if interior == 0 {
        return 0.0;
    }

    let mut sum: f64 = 0.0;
    let mut sum_sq: f64 = 0.0;
    for y in 1..(h - 1) {
        let row = (y as usize) * stride;
        for x in 1..(w - 1) {
            let i = row + x as usize;
            let centre = pixels[i] as f64;
            let up = pixels[i - stride] as f64;
            let down = pixels[i + stride] as f64;
            let left = pixels[i - 1] as f64;
            let right = pixels[i + 1] as f64;
            // Laplacian kernel response.
            let l = up + down + left + right - 4.0 * centre;
            sum += l;
            sum_sq += l * l;
        }
    }

    let n = interior as f64;
    let mean = sum / n;
    let var = (sum_sq / n) - (mean * mean);
    if var.is_finite() && var > 0.0 {
        var as f32
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> DynamicImage {
        let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |_, _| Rgb(rgb));
        DynamicImage::ImageRgb8(buf)
    }

    /// A test image with a distinct corner so rotations are detectable.
    /// Top-left pixel is red, everything else is black.
    fn corner_marker(w: u32, h: u32) -> DynamicImage {
        let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
            if x == 0 && y == 0 {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 0, 0])
            }
        });
        DynamicImage::ImageRgb8(buf)
    }

    #[test]
    fn orientation_1_is_identity() {
        let img = corner_marker(10, 6);
        let out = apply_exif_orientation(img.clone(), Some(1));
        assert_eq!(out.to_rgb8().get_pixel(0, 0), img.to_rgb8().get_pixel(0, 0));
        assert_eq!((out.width(), out.height()), (10, 6));
    }

    #[test]
    fn orientation_none_is_identity() {
        let img = corner_marker(4, 3);
        let out = apply_exif_orientation(img, None);
        assert_eq!((out.width(), out.height()), (4, 3));
        assert_eq!(out.to_rgb8().get_pixel(0, 0), &Rgb([255, 0, 0]));
    }

    #[test]
    fn orientation_6_rotates_90_cw() {
        // Top-left red pixel should end up at top-right after 90° CW rotation.
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img, Some(6));
        // 90° CW: (w, h) → (h, w), top-left maps to top-right.
        assert_eq!((out.width(), out.height()), (6, 4));
        let out_rgb = out.to_rgb8();
        assert_eq!(out_rgb.get_pixel(out.width() - 1, 0), &Rgb([255, 0, 0]));
    }

    #[test]
    fn orientation_8_rotates_270_cw() {
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img, Some(8));
        // 270° CW (= 90° CCW): top-left maps to bottom-left.
        assert_eq!((out.width(), out.height()), (6, 4));
        let out_rgb = out.to_rgb8();
        assert_eq!(out_rgb.get_pixel(0, out.height() - 1), &Rgb([255, 0, 0]));
    }

    #[test]
    fn orientation_3_rotates_180() {
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img, Some(3));
        assert_eq!((out.width(), out.height()), (4, 6));
        let out_rgb = out.to_rgb8();
        assert_eq!(
            out_rgb.get_pixel(out.width() - 1, out.height() - 1),
            &Rgb([255, 0, 0])
        );
    }

    #[test]
    fn orientation_2_flips_horizontal() {
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img, Some(2));
        assert_eq!((out.width(), out.height()), (4, 6));
        let out_rgb = out.to_rgb8();
        assert_eq!(out_rgb.get_pixel(out.width() - 1, 0), &Rgb([255, 0, 0]));
    }

    #[test]
    fn orientation_4_flips_vertical() {
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img, Some(4));
        assert_eq!((out.width(), out.height()), (4, 6));
        let out_rgb = out.to_rgb8();
        assert_eq!(out_rgb.get_pixel(0, out.height() - 1), &Rgb([255, 0, 0]));
    }

    #[test]
    fn orientation_unknown_value_leaves_unchanged() {
        let img = corner_marker(4, 6);
        let out = apply_exif_orientation(img.clone(), Some(99));
        assert_eq!((out.width(), out.height()), (4, 6));
        assert_eq!(out.to_rgb8().get_pixel(0, 0), &Rgb([255, 0, 0]));
    }

    #[test]
    fn laplacian_variance_is_zero_for_solid_color() {
        // A uniform image has no edges — Laplacian response is zero everywhere.
        let img = solid(32, 32, [128, 128, 128]);
        let v = laplacian_variance(&img);
        assert!(v.abs() < 0.01, "expected ~0 variance for solid, got {v}");
    }

    #[test]
    fn laplacian_variance_is_positive_for_edged_image() {
        // Half-white / half-black — strong vertical edge in the middle.
        let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(32, 32, |x, _| {
            if x < 16 {
                Rgb([255, 255, 255])
            } else {
                Rgb([0, 0, 0])
            }
        });
        let img = DynamicImage::ImageRgb8(buf);
        let v = laplacian_variance(&img);
        assert!(
            v > 100.0,
            "expected large variance for edged image, got {v}"
        );
    }

    #[test]
    fn laplacian_variance_tiny_image_is_zero() {
        // Below 3×3 = no interior pixels → should return 0 without panicking.
        let img = solid(2, 2, [128, 128, 128]);
        assert_eq!(laplacian_variance(&img), 0.0);
    }
}
