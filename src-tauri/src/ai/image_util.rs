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

/// Extensions the `image` crate can't decode but that carry an embedded
/// JPEG preview we can pull out via `rawler`.
const RAW_EXTENSIONS: &[&str] = &[
    "arw", "cr2", "cr3", "nef", "nrw", "raf", "rw2", "orf", "dng", "pef", "srw",
];

/// Does this path look like a RAW file by extension? Case-insensitive.
pub fn is_raw_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| RAW_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// HEIC / HEIF extensions — the `image` crate can't decode these without
/// the optional `heif` feature (which pulls in libheif, a C dep). Instead
/// we rely on `scan_embedded_jpeg` below to pull the preview out of the
/// `iprp` / `Exif` box that iPhone etc. always write.
const HEIF_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "avif"];

/// Does this path look like a HEIF container? Case-insensitive.
pub fn is_heif_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| HEIF_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// Open an image file. Pure-Rust, no system deps. The decode cascade:
///
/// 1. `image::open` (JPEG/PNG/TIFF/WebP/GIF/BMP — the common case).
/// 2. `rawler` preview for RAW extensions (ARW/CR2/CR3/NEF/DNG/…) —
///    pulls the embedded JPEG preview.
/// 3. Last-resort **embedded-JPEG scan** — every modern camera RAW and
///    every iPhone HEIC writes at least one full JPEG (SOI `FF D8 FF`
///    through EOI `FF D9`) somewhere inside the container. The `image`
///    crate ignores trailing data, so if we find a valid SOI we can hand
///    that slice straight to the JPEG decoder. Catches HEIC (no libheif
///    needed) + any RAW whose format ID rawler doesn't recognise.
///
/// Each failure is logged via `tracing::warn!` so the on-disk reason
/// shows up in Loki — the old code swallowed rawler errors silently and
/// the user saw purple placeholders with no diagnostic.
pub fn open_any(path: &std::path::Path) -> Result<DynamicImage, String> {
    // Fast path — JPEG/PNG/TIFF/WebP/etc. handled by `image`.
    match image::open(path) {
        Ok(img) => return Ok(img),
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "open_any: image crate couldn't decode, trying fallbacks"
            );
        }
    }

    // Lenient retry — `image::ImageReader::with_guessed_format` sniffs
    // the magic bytes instead of trusting the extension, so it picks up
    // JPEGs wearing a `.HEIC` suffix (screenshots), MPO dual-stream
    // iPhone JPEGs, and TIFFs served with odd extensions. This is the
    // difference between `image::open` (strict) and the content-aware
    // path.
    if let Ok(f) = std::fs::File::open(path) {
        let reader = std::io::BufReader::new(f);
        if let Ok(sniffed) = image::ImageReader::new(reader).with_guessed_format() {
            if let Ok(img) = sniffed.decode() {
                tracing::info!(
                    path = %path.display(),
                    "open_any: decoded via sniffed-format fallback ({}×{})",
                    img.width(),
                    img.height()
                );
                return Ok(img);
            }
        }
    }

    // RAW fallback via rawler.
    if is_raw_extension(path) {
        match rawler::rawsource::RawSource::new(path) {
            Ok(source) => match rawler::get_decoder(&source) {
                Ok(decoder) => {
                    let params = rawler::decoders::RawDecodeParams::default();
                    match decoder.preview_image(&source, &params) {
                        Ok(Some(img)) => return Ok(img),
                        Ok(None) => tracing::debug!(
                            path = %path.display(),
                            "rawler: preview_image returned None, trying thumbnail_image"
                        ),
                        Err(e) => tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "rawler: preview_image errored"
                        ),
                    }
                    match decoder.thumbnail_image(&source, &params) {
                        Ok(Some(img)) => return Ok(img),
                        Ok(None) => tracing::debug!(
                            path = %path.display(),
                            "rawler: thumbnail_image returned None"
                        ),
                        Err(e) => tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "rawler: thumbnail_image errored"
                        ),
                    }
                }
                Err(e) => tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "rawler: get_decoder failed, falling through to jpeg scan"
                ),
            },
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                "rawler: open failed, falling through to jpeg scan"
            ),
        }
    }

    // HEIC fast path via libheif (when the `heic` cargo feature is on).
    // This is the only robust way to decode iPhone HEIC files — the
    // main image is HEVC-encoded and can't be extracted by scanning.
    #[cfg(feature = "heic")]
    if is_heif_extension(path) {
        match decode_heif_via_libheif(path) {
            Ok(img) => {
                tracing::info!(
                    path = %path.display(),
                    "open_any: decoded via libheif ({}×{})",
                    img.width(),
                    img.height()
                );
                return Ok(img);
            }
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                "libheif: decode failed, falling through to byte scan"
            ),
        }
    }

    // Byte-scan fallback — find any JPEG stream embedded anywhere in
    // the file. Works on camera RAWs whose vendor rawler doesn't
    // support. For HEIC this typically hits the ~160×120 EXIF IFD1
    // thumbnail that iPhones embed — low-res but better than a
    // placeholder when libheif isn't built in.
    // Limit to files under ~256 MB to keep this bounded.
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > 0 && meta.len() < 256 * 1024 * 1024 => {}
        Ok(_) => {
            return Err(format!(
                "open_any: {} is empty or too large to scan",
                path.display()
            ))
        }
        Err(e) => return Err(format!("open_any: stat failed for {}: {e}", path.display())),
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("open_any: read failed for {}: {e}", path.display()))?;
    if let Some(img) = scan_embedded_jpeg(&bytes) {
        tracing::info!(
            path = %path.display(),
            "open_any: decoded via embedded-jpeg scan (size {}×{})",
            img.width(),
            img.height()
        );
        return Ok(img);
    }

    if is_heif_extension(path) {
        return Err(format!(
            "open_any: {} — HEIF container has no extractable preview. Full HEVC decoding requires libheif — rebuild with `cargo build --features heic` after `vcpkg install libheif` (Windows) / `apt install libheif-dev` (Linux)",
            path.display()
        ));
    }
    Err(format!("open_any: no decoder accepted {}", path.display()))
}

/// Full HEIF decoder via libheif — handles iPhone HEIC + Android HEIF
/// main images (which are HEVC-encoded and can't be scanned for JPEG).
/// Gated behind `cfg(feature = "heic")` so default builds that don't
/// have libheif-dev / vcpkg-installed libheif still compile.
#[cfg(feature = "heic")]
fn decode_heif_via_libheif(path: &std::path::Path) -> Result<DynamicImage, String> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let heif = LibHeif::new();
    let ctx = HeifContext::read_from_file(
        path.to_str()
            .ok_or_else(|| format!("libheif: non-UTF-8 path {}", path.display()))?,
    )
    .map_err(|e| format!("libheif: read {}: {e}", path.display()))?;
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| format!("libheif: primary handle: {e}"))?;
    let img = heif
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
        .map_err(|e| format!("libheif: decode: {e}"))?;

    let planes = img.planes();
    let plane = planes
        .interleaved
        .ok_or_else(|| "libheif: no interleaved plane".to_string())?;
    let w = plane.width;
    let h = plane.height;
    let stride = plane.stride;
    let src = plane.data;
    // Copy row-by-row to strip stride padding; image crate expects tightly
    // packed RGB.
    let row_bytes = (w as usize) * 3;
    let mut packed = Vec::with_capacity(row_bytes * (h as usize));
    for y in 0..(h as usize) {
        let row_start = y * stride;
        let row_end = row_start + row_bytes;
        packed.extend_from_slice(&src[row_start..row_end]);
    }
    let buf = image::RgbImage::from_raw(w, h, packed)
        .ok_or_else(|| "libheif: RgbImage::from_raw size mismatch".to_string())?;
    Ok(DynamicImage::ImageRgb8(buf))
}

/// Scan a byte buffer for embedded JPEG streams and return the largest
/// one that decodes successfully. JPEG streams start with the SOI marker
/// `FF D8 FF` and end with EOI `FF D9`. A camera RAW or HEIC container
/// can have 2–4 of these (1–3 thumbnails + a full preview); we pick the
/// one that produces the largest decoded image.
fn scan_embedded_jpeg(bytes: &[u8]) -> Option<DynamicImage> {
    // Find all SOI candidates. SOI is exactly 3 bytes because a valid
    // JPEG starts FF D8 FF <marker>. Stops at the last such match.
    let mut starts: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] == 0xFF && bytes[i + 1] == 0xD8 && bytes[i + 2] == 0xFF {
            starts.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return None;
    }

    let mut best: Option<DynamicImage> = None;
    let mut best_area: u64 = 0;
    for start in starts {
        // Slice to end of file — the JPEG decoder stops at EOI.
        let slice = &bytes[start..];
        if let Ok(img) = image::load_from_memory_with_format(slice, image::ImageFormat::Jpeg) {
            let area = u64::from(img.width()) * u64::from(img.height());
            if area > best_area {
                best_area = area;
                best = Some(img);
            }
        }
    }
    best
}

/// Encode `img` as JPEG at `quality` into a `Vec<u8>`. `quality` is 0-100
/// where higher = larger file + better detail. The `image` crate default
/// is 75 which leaves visible blocky artefacts on our catalog thumbnails;
/// callers should pass 88-92 for display-grade output.
pub fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(64 * 1024);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(img)
        .map_err(|e| format!("encode_jpeg: {e}"))?;
    Ok(out)
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

    #[test]
    fn is_raw_extension_recognises_common_formats() {
        use std::path::Path;
        assert!(is_raw_extension(Path::new("DSC02451.ARW")));
        assert!(is_raw_extension(Path::new("DSC02452.arw")));
        assert!(is_raw_extension(Path::new("IMG_1234.CR3")));
        assert!(is_raw_extension(Path::new("a.cr2")));
        assert!(is_raw_extension(Path::new("b.nef")));
        assert!(is_raw_extension(Path::new("c.dng")));
        assert!(!is_raw_extension(Path::new("photo.jpg")));
        assert!(!is_raw_extension(Path::new("scan.tif")));
        assert!(!is_raw_extension(Path::new("noext")));
    }

    #[test]
    fn open_any_reads_a_jpeg_via_image_crate_path() {
        use image::ImageFormat;
        use std::io::Write;
        let img = solid(16, 16, [200, 100, 50]);
        let mut bytes: Vec<u8> = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Jpeg)
            .expect("encode");
        let tmp = tempfile::NamedTempFile::with_suffix(".jpg").expect("tmp");
        tmp.as_file().write_all(&bytes).expect("write");
        let opened = open_any(tmp.path()).expect("open");
        assert_eq!(opened.width(), 16);
        assert_eq!(opened.height(), 16);
    }
}
