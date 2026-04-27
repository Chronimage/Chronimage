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

/// HEIC / HEIF extensions. The `image` crate cannot decode these by itself;
/// `open_any` routes them through platform/optional HEIF decoders before
/// trying the embedded-JPEG scanner.
const HEIF_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "avif"];

/// Does this path look like a HEIF container? Case-insensitive.
pub fn is_heif_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| HEIF_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// Return a display orientation for `path`, using platform metadata readers for
/// formats that `kamadak-exif` cannot parse as a plain EXIF container.
pub fn orientation_for_path(path: &std::path::Path) -> Option<u32> {
    if !is_heif_extension(path) {
        return None;
    }

    #[cfg(windows)]
    {
        match wic_orientation_for_path(path) {
            Ok(Some(orientation)) => return Some(orientation),
            Ok(None) => {}
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "wic: orientation metadata read failed"
                );
            }
        }

        match shell_orientation_for_path(path) {
            Ok(orientation) => orientation,
            Err(e) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %e,
                    "shell: orientation metadata read failed"
                );
                None
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// Prefer stored EXIF orientation, but treat a default `1` on HEIF files as
/// provisional because the regular EXIF reader often cannot see HEIF metadata.
pub fn effective_orientation_for_path(
    path: &std::path::Path,
    stored_orientation: Option<u32>,
) -> Option<u32> {
    let stored_orientation = stored_orientation.filter(|v| (1..=8).contains(v));
    if is_heif_extension(path) && stored_orientation.unwrap_or(1) == 1 {
        orientation_for_path(path).or(stored_orientation)
    } else {
        stored_orientation
    }
}

/// Some platform decoders return HEIF pixels already transformed into display
/// orientation. Callers that rotate after `open_any` should skip EXIF rotation
/// for those paths to avoid turning portrait HEICs sideways.
pub fn orientation_for_decoded_path(
    path: &std::path::Path,
    stored_orientation: Option<u32>,
) -> Option<u32> {
    if decoded_pixels_are_display_oriented(path) {
        Some(1)
    } else {
        effective_orientation_for_path(path, stored_orientation)
    }
}

/// Whether `open_any` returns display-oriented pixels for this path.
pub fn decoded_pixels_are_display_oriented(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        is_heif_extension(path)
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

pub fn legacy_thumbnail_cache_path(
    thumbs_dir: &std::path::Path,
    sha256: &str,
    size: u32,
) -> std::path::PathBuf {
    thumbs_dir.join(format!("{sha256}_{size}.jpg"))
}

/// Max longest-edge for the per-photo AI-preview cache. Chosen so that:
/// - SCRFD face detection (640×640 input) has headroom,
/// - SigLIP / NIMA (224×224 input) is well-covered,
/// - JPEG q=85 lands ~150 KB per photo, keeping the cache lean.
pub const AI_PREVIEW_MAX_EDGE: u32 = 1280;

/// Path of the per-photo AI/dedupe input cache. Layout
/// `{thumbs_dir}/{sha256}_aipreview.jpg`. Sized at most
/// `AI_PREVIEW_MAX_EDGE` on the longest edge, JPEG-encoded.
///
/// The on-disk naming intentionally shares the `{sha256}_*.jpg` prefix
/// the source-disconnect cleanup uses, so this file is removed
/// automatically when its photo leaves the catalog.
pub fn ai_preview_cache_path(thumbs_dir: &std::path::Path, sha256: &str) -> std::path::PathBuf {
    thumbs_dir.join(format!("{sha256}_aipreview.jpg"))
}

/// Write the AI-preview cache JPEG for `sha256` if it doesn't already
/// exist. Resizes `img` to fit `AI_PREVIEW_MAX_EDGE` on the longest edge
/// (skipping the resize when the source is already smaller).
///
/// Best-effort: returns Ok even if the directory create or file write
/// fails — the AI stages still work via the byte-scan fallback in
/// `open_for_ai`. Logs the underlying error for visibility.
pub fn write_ai_preview_cache(thumbs_dir: &std::path::Path, sha256: &str, img: &DynamicImage) {
    let cache_path = ai_preview_cache_path(thumbs_dir, sha256);
    if cache_path.exists() {
        return;
    }
    let (w, h) = (img.width(), img.height());
    let resized = if w.max(h) > AI_PREVIEW_MAX_EDGE {
        img.thumbnail(AI_PREVIEW_MAX_EDGE, AI_PREVIEW_MAX_EDGE)
    } else {
        img.clone()
    };
    let buf = match encode_jpeg(&resized, 85) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                sha256,
                error = %e,
                "ai_preview_cache: encode_jpeg failed"
            );
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(thumbs_dir) {
        tracing::warn!(
            sha256,
            dir = %thumbs_dir.display(),
            error = %e,
            "ai_preview_cache: create_dir_all failed"
        );
        return;
    }
    if let Err(e) = std::fs::write(&cache_path, &buf) {
        tracing::warn!(
            sha256,
            path = %cache_path.display(),
            error = %e,
            "ai_preview_cache: write failed"
        );
    }
}

/// Open a photo for AI/dedupe input. When `sha256` is `Some` AND the
/// AI-preview cache exists, decode that JPEG (fast — single-threaded
/// JPEG decode via the `image` crate, no HEIC/RAW codec round-trip).
/// Otherwise fall back to the full `open_any` cascade.
///
/// `sha256 = None` is the test-friendly path: synthetic test images
/// don't have a content hash, and falling through to `open_any` still
/// works for the JPEG/PNG fixtures the AI tests use.
pub fn open_for_ai(path: &std::path::Path, sha256: Option<&str>) -> Result<DynamicImage, String> {
    if let Some(sha) = sha256 {
        if let Ok(thumbs_dir) = crate::util::paths::thumbnails_dir() {
            let cache_path = ai_preview_cache_path(&thumbs_dir, sha);
            if cache_path.exists() {
                if let Ok(img) = image::open(&cache_path) {
                    return Ok(img);
                }
                tracing::warn!(
                    sha256 = sha,
                    path = %cache_path.display(),
                    "open_for_ai: cached preview decode failed, regenerating from source"
                );
                // Fall through to open_any below. The stale cache file
                // will be overwritten next time the pipeline runs.
            }
        }
    }
    open_any(path)
}

/// Open an image file. The decoder picked depends on the extension:
///
/// - **JPEG/PNG/TIFF/WebP/GIF/BMP** → `image` crate (pure-Rust, fast path).
///   This step is skipped for HEIF/RAW extensions because the `image`
///   crate has no decoder for either, so attempting it would just produce
///   a misleading "couldn't decode" log on every HEIC/RAW import.
/// - **RAW** (ARW/CR2/CR3/NEF/DNG/…) → `rawler` preview (embedded JPEG).
/// - **HEIF/HEIC/AVIF/HIF** → platform/optional HEIF decode. On Windows
///   the WIC HEIF Image Extension is the standard path (Microsoft's
///   licensed HEVC codec); `--features heic` enables libheif as an
///   additional cross-platform decoder.
/// - Anything else falls through to a last-resort **embedded-JPEG scan**
///   (SOI `FF D8 FF` through EOI `FF D9`). Many RAW and HEIF containers
///   carry an embedded JPEG preview; finding it gives us at least a small
///   thumbnail when the dedicated decoder fails.
///
/// Each failure is logged via `tracing::warn!` so the on-disk reason
/// shows up in Loki — the old code swallowed rawler errors silently and
/// the user saw purple placeholders with no diagnostic.
pub fn open_any(path: &std::path::Path) -> Result<DynamicImage, String> {
    // Skip the `image` crate entirely for known HEIF/RAW extensions —
    // it has no decoder for either format, so attempting it just emits a
    // misleading "couldn't decode" log on every HEIC/RAW import. The
    // dedicated decoders below are the only ones that work for these.
    let skip_image_crate = is_heif_extension(path) || is_raw_extension(path);

    if !skip_image_crate {
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
        // path. We skip this for confirmed HEIF/RAW above because the
        // sniffer doesn't recognise either format anyway.
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

    // HEIC / HEIF fast paths. The primary image in iPhone HEIC is HEVC-
    // encoded, so byte-scanning usually cannot recover it. On Windows we
    // ask WIC to use the installed HEIF/HEVC codecs; optional libheif covers
    // builds where that cargo feature is enabled.
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

    #[cfg(windows)]
    if is_heif_extension(path) {
        match decode_heif_via_wic(path) {
            Ok(img) => {
                tracing::info!(
                    path = %path.display(),
                    "open_any: decoded via Windows WIC ({}x{})",
                    img.width(),
                    img.height()
                );
                return Ok(img);
            }
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                "wic: HEIF decode failed, falling through to byte scan"
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
            "open_any: {} — HEIF container has no extractable preview. Full HEVC decoding requires an installed Windows HEIF/HEVC codec or libheif via `cargo build --features heic` after `vcpkg install libheif` (Windows) / `apt install libheif-dev` (Linux)",
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

/// HEIF decoder via Windows Imaging Component. This uses the OS codec stack
/// (HEIF Image Extensions / HEVC decoder) and keeps default Windows builds
/// free of libheif's native build dependency.
#[cfg(windows)]
fn wic_orientation_for_path(path: &std::path::Path) -> Result<Option<u32>, String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GENERIC_READ, RPC_E_CHANGED_MODE},
            Graphics::Imaging::{
                CLSID_WICImagingFactory, IWICImagingFactory, WICDecodeMetadataCacheOnDemand,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize,
                StructuredStorage::{
                    PropVariantClear, PropVariantToString, PropVariantToUInt16,
                    PropVariantToUInt32, PROPVARIANT,
                },
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
            },
        },
    };

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn read_orientation_value(value: &PROPVARIANT) -> Option<u32> {
        let numeric = unsafe {
            PropVariantToUInt16(value as *const PROPVARIANT)
                .map(u32::from)
                .or_else(|_| PropVariantToUInt32(value as *const PROPVARIANT))
        };
        if let Ok(v) = numeric {
            return (1..=8).contains(&v).then_some(v);
        }

        let mut text = [0u16; 32];
        if unsafe { PropVariantToString(value as *const PROPVARIANT, &mut text) }.is_ok() {
            let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
            let s = String::from_utf16_lossy(&text[..end]);
            if let Ok(v) = s.trim().parse::<u32>() {
                return (1..=8).contains(&v).then_some(v);
            }
        }
        None
    }

    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _com_guard = if init == RPC_E_CHANGED_MODE {
        ComGuard(false)
    } else {
        init.ok()
            .map_err(|e| format!("wic: CoInitializeEx failed: {e}"))?;
        ComGuard(true)
    };

    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let factory: IWICImagingFactory = unsafe {
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("wic: CoCreateInstance WIC factory: {e}"))?
    };
    let decoder = unsafe {
        factory.CreateDecoderFromFilename(
            PCWSTR(wide_path.as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )
    }
    .map_err(|e| format!("wic: CreateDecoderFromFilename {}: {e}", path.display()))?;
    let frame = unsafe { decoder.GetFrame(0) }
        .map_err(|e| format!("wic: GetFrame(0) {}: {e}", path.display()))?;
    let reader = unsafe { frame.GetMetadataQueryReader() }
        .map_err(|e| format!("wic: GetMetadataQueryReader {}: {e}", path.display()))?;

    for query in [
        "/ifd/{ushort=274}",
        "/app1/ifd/{ushort=274}",
        "/ifd/exif/{ushort=274}",
        "/xmp/tiff:Orientation",
    ] {
        let wide_query: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();
        let mut value = PROPVARIANT::default();
        let read_result =
            unsafe { reader.GetMetadataByName(PCWSTR(wide_query.as_ptr()), &mut value) };
        if read_result.is_err() {
            continue;
        }

        let orientation = read_orientation_value(&value);
        let _ = unsafe { PropVariantClear(&mut value as *mut PROPVARIANT) };
        if let Some(orientation) = orientation {
            tracing::debug!(
                path = %path.display(),
                query,
                orientation,
                "wic: found HEIF orientation metadata"
            );
            return Ok(Some(orientation));
        }
    }

    Ok(None)
}

#[cfg(windows)]
fn shell_orientation_for_path(path: &std::path::Path) -> Result<Option<u32>, String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            Storage::EnhancedStorage::PKEY_Photo_Orientation,
            System::Com::{CoInitializeEx, CoUninitialize, IBindCtx, COINIT_MULTITHREADED},
            UI::Shell::{IShellItem2, SHCreateItemFromParsingName},
        },
    };

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _com_guard = if init == RPC_E_CHANGED_MODE {
        ComGuard(false)
    } else {
        init.ok()
            .map_err(|e| format!("shell: CoInitializeEx failed: {e}"))?;
        ComGuard(true)
    };

    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let item: IShellItem2 =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None::<&IBindCtx>) }
            .map_err(|e| format!("shell: SHCreateItemFromParsingName {}: {e}", path.display()))?;

    let orientation = unsafe { item.GetUInt32(&PKEY_Photo_Orientation as *const _) }
        .map_err(|e| format!("shell: GetUInt32(PKEY_Photo_Orientation): {e}"))?;
    Ok((1..=8).contains(&orientation).then_some(orientation))
}

/// True for the WIC errors that are worth retrying once with a brief
/// delay rather than dropping straight to the byte-scan fallback.
///
/// Microsoft's HEIF Image Extension is fronted by Media Foundation's HEVC
/// decoder, whose internal MFT pool can return `WAIT_TIMEOUT` (`0x80070102`)
/// under concurrent decode load. The codec is flaky-not-broken: a short
/// pause + retry almost always succeeds. Serializing all callers behind a
/// mutex would also avoid the timeout, but at the cost of 4× slower import
/// throughput on iPhone HEIC libraries — retry-on-timeout keeps the
/// 4-way parallelism the import pipeline relies on.
#[cfg(windows)]
fn is_transient_wic_failure(err: &str) -> bool {
    err.contains("0x80070102")
        || err
            .to_ascii_lowercase()
            .contains("wait operation timed out")
}

#[cfg(windows)]
fn decode_heif_via_wic(path: &std::path::Path) -> Result<DynamicImage, String> {
    match decode_heif_via_wic_attempt(path) {
        Ok(img) => Ok(img),
        Err(e) if is_transient_wic_failure(&e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "wic: transient HEIF timeout, retrying after 50ms"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
            decode_heif_via_wic_attempt(path)
        }
        Err(e) => Err(e),
    }
}

#[cfg(windows)]
fn decode_heif_via_wic_attempt(path: &std::path::Path) -> Result<DynamicImage, String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GENERIC_READ, RPC_E_CHANGED_MODE},
            Graphics::Imaging::{
                CLSID_WICImagingFactory, GUID_WICPixelFormat24bppRGB, IWICImagingFactory,
                WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
                WICDecodeMetadataCacheOnDemand,
            },
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
        },
    };

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _com_guard = if init == RPC_E_CHANGED_MODE {
        ComGuard(false)
    } else {
        init.ok()
            .map_err(|e| format!("wic: CoInitializeEx failed: {e}"))?;
        ComGuard(true)
    };

    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let factory: IWICImagingFactory = unsafe {
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("wic: CoCreateInstance WIC factory: {e}"))?
    };
    let decoder = unsafe {
        factory.CreateDecoderFromFilename(
            PCWSTR(wide_path.as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )
    }
    .map_err(|e| format!("wic: CreateDecoderFromFilename {}: {e}", path.display()))?;
    let frame = unsafe { decoder.GetFrame(0) }
        .map_err(|e| format!("wic: GetFrame(0) {}: {e}", path.display()))?;
    let converter = unsafe { factory.CreateFormatConverter() }
        .map_err(|e| format!("wic: CreateFormatConverter: {e}"))?;

    unsafe {
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat24bppRGB,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
    }
    .map_err(|e| format!("wic: convert to 24bpp RGB: {e}"))?;

    let mut width = 0u32;
    let mut height = 0u32;
    unsafe { converter.GetSize(&mut width, &mut height) }
        .map_err(|e| format!("wic: GetSize: {e}"))?;
    if width == 0 || height == 0 {
        return Err("wic: decoded image has zero dimensions".to_string());
    }

    let stride = width
        .checked_mul(3)
        .ok_or_else(|| format!("wic: stride overflow for {}x{}", width, height))?;
    let len = stride
        .checked_mul(height)
        .ok_or_else(|| format!("wic: buffer overflow for {}x{}", width, height))?;
    let mut pixels = vec![0u8; len as usize];
    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut pixels) }
        .map_err(|e| format!("wic: CopyPixels: {e}"))?;

    let buf = image::RgbImage::from_raw(width, height, pixels)
        .ok_or_else(|| "wic: RgbImage::from_raw size mismatch".to_string())?;
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

    #[cfg(windows)]
    #[test]
    fn is_transient_wic_failure_matches_real_codepaths() {
        // Exact format the windows crate emits — must match for retry.
        assert!(is_transient_wic_failure(
            "wic: CopyPixels: The wait operation timed out. (0x80070102)"
        ));
        // HRESULT-only form (defensive — if a future windows-rs error
        // formatting drops the human-readable prefix, the hex is enough).
        assert!(is_transient_wic_failure("error 0x80070102"));
        // Lowercase variant — match should be case-insensitive on the
        // text path, since Windows API error messages don't always come
        // through with consistent casing.
        assert!(is_transient_wic_failure("Wait Operation Timed Out"));
        // Other failures must NOT trigger retry — e.g. a real codec error.
        assert!(!is_transient_wic_failure(
            "wic: CreateDecoderFromFilename: codec not found (0x80070002)"
        ));
        assert!(!is_transient_wic_failure(
            "wic: convert to 24bpp RGB: invalid format"
        ));
    }

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
    fn is_heif_extension_recognises_common_formats() {
        use std::path::Path;
        assert!(is_heif_extension(Path::new("IMG_0001.HEIC")));
        assert!(is_heif_extension(Path::new("clip.heif")));
        assert!(is_heif_extension(Path::new("photo.hif")));
        assert!(is_heif_extension(Path::new("render.avif")));
        assert!(!is_heif_extension(Path::new("photo.jpg")));
        assert!(!is_heif_extension(Path::new("noext")));
    }

    #[test]
    fn legacy_thumbnail_cache_path_matches_cache_contract() {
        use std::path::Path;
        let root = Path::new("thumbs");
        assert_eq!(
            legacy_thumbnail_cache_path(root, "abc", 480),
            root.join("abc_480.jpg")
        );
    }

    #[test]
    fn ai_preview_cache_path_uses_aipreview_suffix() {
        // Suffix is intentionally `_aipreview` (not a number), so the
        // disconnect-cleanup loop in `delete_source_impl` (which splits on
        // `_` and drops every `{sha}_*.jpg` for orphan photos) still
        // matches and removes the file. Any change to this naming must
        // also keep that cleanup test happy.
        use std::path::Path;
        let root = Path::new("thumbs");
        let path = ai_preview_cache_path(root, "abc123");
        assert_eq!(path, root.join("abc123_aipreview.jpg"));
        // Verify the prefix-style cleanup pattern still tags this file.
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let (sha, suffix) = stem.rsplit_once('_').unwrap();
        assert_eq!(sha, "abc123");
        assert_eq!(suffix, "aipreview");
    }

    #[test]
    fn write_ai_preview_cache_skips_when_already_present() {
        // Pre-existing cache files must not be silently overwritten — the
        // first writer wins, since two parallel imports of the same SHA
        // would produce identical bytes anyway and writing twice wastes IO.
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let sha = "deadbeef";
        let path = ai_preview_cache_path(tmp.path(), sha);
        std::fs::write(&path, b"sentinel").unwrap();
        let img = solid(64, 64, [10, 20, 30]);
        write_ai_preview_cache(tmp.path(), sha, &img);
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, b"sentinel", "existing cache must be untouched");
    }

    #[test]
    fn write_ai_preview_cache_downscales_oversize_image_and_writes_valid_jpeg() {
        // The cache must (1) cap the longest edge at AI_PREVIEW_MAX_EDGE
        // and (2) write a JPEG that the `image` crate can re-decode — that
        // re-decode is exactly the fast path AI stages take when the
        // pipeline has already seeded the cache.
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let sha = "ai_preview_oversize";
        let oversize = solid(AI_PREVIEW_MAX_EDGE * 2, AI_PREVIEW_MAX_EDGE, [200, 100, 50]);
        write_ai_preview_cache(tmp.path(), sha, &oversize);

        let cache_path = ai_preview_cache_path(tmp.path(), sha);
        assert!(cache_path.exists(), "cache file must exist");

        let decoded = image::open(&cache_path).expect("re-decode cached jpeg");
        assert!(
            decoded.width().max(decoded.height()) <= AI_PREVIEW_MAX_EDGE,
            "cached image must fit the AI_PREVIEW_MAX_EDGE box, got {}×{}",
            decoded.width(),
            decoded.height(),
        );
    }

    #[test]
    fn write_ai_preview_cache_keeps_small_image_at_full_size() {
        // A 600×400 photo is already smaller than AI_PREVIEW_MAX_EDGE, so
        // the cache should keep it at full size — no point upsampling, and
        // we don't want to lose detail on small originals.
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let sha = "ai_preview_small";
        let small = solid(600, 400, [10, 20, 30]);
        write_ai_preview_cache(tmp.path(), sha, &small);

        let decoded = image::open(ai_preview_cache_path(tmp.path(), sha)).expect("decode");
        assert_eq!((decoded.width(), decoded.height()), (600, 400));
    }

    #[test]
    fn open_for_ai_with_no_sha_falls_through_to_open_any() {
        // Without a sha hint the helper has nothing to look up — it must
        // route to open_any. For a missing path that surfaces an Err
        // (open_any: stat failed), proving the fall-through happened.
        assert!(open_for_ai(std::path::Path::new("/nonexistent.jpg"), None).is_err());
    }

    #[test]
    fn orientation_for_decoded_path_skips_windows_heif_rotation() {
        use std::path::Path;
        assert_eq!(
            orientation_for_decoded_path(Path::new("photo.jpg"), Some(6)),
            Some(6)
        );

        #[cfg(windows)]
        assert_eq!(
            orientation_for_decoded_path(Path::new("IMG_0001.HEIC"), Some(6)),
            Some(1)
        );

        #[cfg(not(windows))]
        assert_eq!(
            orientation_for_decoded_path(Path::new("IMG_0001.HEIC"), Some(6)),
            Some(6)
        );
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
