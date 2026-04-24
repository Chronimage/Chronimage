//! Export engine — enqueue + execute export jobs one item at a time.
//!
//! Keeping the runtime single-threaded (one item per `run_next_item` call)
//! simplifies the first pass: the UI can drive the progress ticker itself
//! without us needing a worker pool or pause/resume handles. Parallel +
//! GPU acceleration land in a follow-up.

use super::preset::{ColorProfile, ExportPreset, Format};
use crate::{ai::image_util::apply_exif_orientation, AppError, AppResult};
use image::{
    codecs::jpeg::JpegEncoder, imageops::FilterType, DynamicImage, GenericImageView, ImageEncoder,
    ImageFormat,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Paused,
    Done,
    Cancelled,
    Error,
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Paused => "paused",
            JobStatus::Done => "done",
            JobStatus::Cancelled => "cancelled",
            JobStatus::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExportJob {
    pub id: i64,
    pub created_at: String,
    pub preset_json: String,
    pub total_photos: i64,
    pub done_count: i64,
    pub error_count: i64,
    pub status: String,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExportJobItem {
    pub id: i64,
    pub job_id: i64,
    pub photo_id: i64,
    pub status: String,
    pub output_path: Option<String>,
    pub error_msg: Option<String>,
}

/// Per-item progress event payload emitted by [`run_next_item`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProgress {
    pub job_id: i64,
    pub photo_id: i64,
    pub done_count: i64,
    pub error_count: i64,
    pub total: i64,
    pub status: JobStatus,
    pub item_status: String,
    pub op: String,
    pub output_path: Option<String>,
    pub error_msg: Option<String>,
}

/// Enqueue a job. Creates the `export_jobs` row + one `export_job_items` row
/// per photo. Returns the new job id.
pub async fn enqueue_job(
    pool: &SqlitePool,
    photo_ids: &[i64],
    preset: &ExportPreset,
    output_dir: &Path,
) -> AppResult<i64> {
    if photo_ids.is_empty() {
        return Err(AppError::InvalidInput("no photos to export".into()));
    }
    std::fs::create_dir_all(output_dir)?;

    let now = chrono::Utc::now().to_rfc3339();
    let preset_json = serde_json::to_string(preset)?;
    let output_dir_str = output_dir.to_string_lossy().to_string();

    let mut tx = pool.begin().await?;
    let job_id: i64 = sqlx::query_scalar(
        "INSERT INTO export_jobs (created_at, preset_json, total_photos, status, output_dir) \
         VALUES (?1, ?2, ?3, 'queued', ?4) RETURNING id",
    )
    .bind(&now)
    .bind(&preset_json)
    .bind(photo_ids.len() as i64)
    .bind(&output_dir_str)
    .fetch_one(&mut *tx)
    .await?;

    for pid in photo_ids {
        sqlx::query(
            "INSERT INTO export_job_items (job_id, photo_id, status) VALUES (?1, ?2, 'queued')",
        )
        .bind(job_id)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(job_id)
}

/// List jobs newest-first. Mainly for a "recent exports" view + tests.
pub async fn list_jobs(pool: &SqlitePool) -> AppResult<Vec<ExportJob>> {
    sqlx::query_as::<_, ExportJob>(
        "SELECT id, created_at, preset_json, total_photos, done_count, error_count, \
                status, output_dir \
         FROM export_jobs ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

/// Process the next queued item in `job_id`. Returns:
/// - `Some(ExportProgress)` — an item was processed (success or error).
/// - `None` — no queued items remain; caller should poll `list_jobs` for
///   final status.
///
/// The caller is responsible for looping over `run_next_item` until it
/// returns `None`. Each call is a self-contained unit so a Tauri command can
/// fire one + emit the event, keeping the UI responsive.
pub async fn run_next_item(pool: &SqlitePool, job_id: i64) -> AppResult<Option<ExportProgress>> {
    // Flip job to running the first time we touch it.
    sqlx::query("UPDATE export_jobs SET status = 'running' WHERE id = ?1 AND status = 'queued'")
        .bind(job_id)
        .execute(pool)
        .await?;

    // Grab the next queued item + its photo metadata.
    let row = sqlx::query_as::<_, (i64, i64, String, String, Option<i64>)>(
        "SELECT i.id, i.photo_id, p.sha256, p.filename, p.orientation \
         FROM export_job_items i JOIN photos p ON p.id = i.photo_id \
         WHERE i.job_id = ?1 AND i.status = 'queued' \
         ORDER BY i.id LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;

    let Some((item_id, photo_id, _sha, filename, orientation)) = row else {
        // Nothing left — finalise job status.
        finalise_job(pool, job_id).await?;
        return Ok(None);
    };

    // Mark item running + fetch preset + output_dir.
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE export_job_items SET status = 'running', started_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(item_id)
        .execute(pool)
        .await?;

    let (preset_json, output_dir): (String, String) =
        sqlx::query_as("SELECT preset_json, output_dir FROM export_jobs WHERE id = ?1")
            .bind(job_id)
            .fetch_one(pool)
            .await?;
    let preset: ExportPreset = serde_json::from_str(&preset_json)?;
    let output_dir = PathBuf::from(&output_dir);

    // Resolve a source path. Prefer a local copy; fall back to the first
    // non-null path in source_copies.
    let src_path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
    )
    .bind(photo_id)
    .fetch_optional(pool)
    .await?;

    let result = match src_path {
        Some(src) => encode_one(
            &src,
            &filename,
            orientation.unwrap_or(1),
            &preset,
            &output_dir,
        ),
        None => Err(AppError::NotFound(format!(
            "no local copy for photo {photo_id}"
        ))),
    };

    let (item_status, output_path, error_msg) = match &result {
        Ok(p) => ("done", Some(p.to_string_lossy().to_string()), None),
        Err(e) => ("error", None, Some(e.to_string())),
    };

    let finished = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE export_job_items SET status = ?1, output_path = ?2, error_msg = ?3, finished_at = ?4 \
         WHERE id = ?5",
    )
    .bind(item_status)
    .bind(&output_path)
    .bind(&error_msg)
    .bind(&finished)
    .bind(item_id)
    .execute(pool)
    .await?;

    // Bump counters on the parent job.
    let col = if item_status == "done" {
        "done_count"
    } else {
        "error_count"
    };
    sqlx::query(&format!(
        "UPDATE export_jobs SET {col} = {col} + 1 WHERE id = ?1"
    ))
    .bind(job_id)
    .execute(pool)
    .await?;

    let (done_count, error_count, total): (i64, i64, i64) = sqlx::query_as(
        "SELECT done_count, error_count, total_photos FROM export_jobs WHERE id = ?1",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(ExportProgress {
        job_id,
        photo_id,
        done_count,
        error_count,
        total,
        status: JobStatus::Running,
        item_status: item_status.to_string(),
        op: format!("{:?}", preset.format),
        output_path,
        error_msg,
    }))
}

async fn finalise_job(pool: &SqlitePool, job_id: i64) -> AppResult<()> {
    let (done, errors, total): (i64, i64, i64) = sqlx::query_as(
        "SELECT done_count, error_count, total_photos FROM export_jobs WHERE id = ?1",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    let status = if done + errors < total {
        "paused"
    } else if errors > 0 && done == 0 {
        "error"
    } else {
        "done"
    };
    let finished = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE export_jobs SET status = ?1, finished_at = ?2 WHERE id = ?3")
        .bind(status)
        .bind(&finished)
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Decode → orient → resize → encode a single photo.
fn encode_one(
    src_path: &str,
    filename: &str,
    orientation: i64,
    preset: &ExportPreset,
    output_dir: &Path,
) -> AppResult<PathBuf> {
    #[cfg(not(feature = "heic"))]
    if matches!(preset.format, Format::Heic) {
        return Err(AppError::InvalidInput(
            "HEIC export requires the `heic` cargo feature + libheif installed. \
             On Windows: vcpkg install libheif, then cargo build --features heic."
                .into(),
        ));
    }
    if !matches!(preset.color, ColorProfile::Srgb) {
        tracing::warn!(
            color = ?preset.color,
            "non-sRGB color profile requested; encoding as sRGB (Phase-3 follow-up)"
        );
    }

    let img = image::ImageReader::open(src_path)
        .map_err(|e| AppError::Internal(format!("open {src_path}: {e}")))?
        .with_guessed_format()
        .map_err(|e| AppError::Internal(format!("sniff {src_path}: {e}")))?
        .decode()
        .map_err(|e| AppError::Internal(format!("decode {src_path}: {e}")))?;

    // EXIF-correct orientation before resize so the output geometry matches
    // what the user sees in Catalog.
    let img = apply_exif_orientation(img, Some(orientation as u32));

    let resized = resize_to_long_edge(img, preset.long_edge_px);

    let base = stem_without_ext(filename);
    let ext = match preset.format {
        Format::Jpeg => "jpg",
        Format::Tiff => "tif",
        Format::Heic => unreachable!(),
    };
    let out = output_dir.join(format!("{base}.{ext}"));

    match preset.format {
        Format::Jpeg => encode_jpeg(&resized, &out, preset.quality)?,
        Format::Tiff => encode_tiff(&resized, &out)?,
        #[cfg(feature = "heic")]
        Format::Heic => encode_heic(&resized, &out, preset.quality)?,
        #[cfg(not(feature = "heic"))]
        Format::Heic => unreachable!("guarded above when feature disabled"),
    }
    Ok(out)
}

#[cfg(feature = "heic")]
fn encode_heic(img: &DynamicImage, out: &Path, quality: u8) -> AppResult<()> {
    use libheif_rs::{
        Channel, ColorSpace, CompressionFormat, EncoderQuality, HeifContext, LibHeif, RgbChroma,
    };
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let heif = LibHeif::new();
    let mut ctx = HeifContext::new().map_err(|e| AppError::Internal(format!("heif ctx: {e}")))?;
    let mut enc = heif
        .encoder_for_format(CompressionFormat::Hevc)
        .map_err(|e| AppError::Internal(format!("heif encoder: {e}")))?;
    enc.set_quality(EncoderQuality::Lossy(quality))
        .map_err(|e| AppError::Internal(format!("heif quality: {e}")))?;
    let mut heif_img = libheif_rs::Image::new(w, h, ColorSpace::Rgb(RgbChroma::Rgb))
        .map_err(|e| AppError::Internal(format!("heif image: {e}")))?;
    heif_img
        .create_plane(Channel::Interleaved, w, h, 8)
        .map_err(|e| AppError::Internal(format!("heif plane: {e}")))?;
    {
        let mut plane = heif_img
            .planes_mut()
            .interleaved
            .ok_or_else(|| AppError::Internal("heif plane missing".into()))?;
        let stride = plane.stride;
        let data = plane.data;
        for y in 0..(h as usize) {
            let src_start = y * (w as usize) * 3;
            let dst_start = y * stride;
            data[dst_start..dst_start + (w as usize) * 3]
                .copy_from_slice(&rgb.as_raw()[src_start..src_start + (w as usize) * 3]);
        }
    }
    ctx.encode_image(&heif_img, &mut enc, None)
        .map_err(|e| AppError::Internal(format!("heif encode: {e}")))?;
    ctx.write_to_file(out.to_string_lossy().as_ref())
        .map_err(|e| AppError::Internal(format!("heif write {}: {e}", out.display())))?;
    Ok(())
}

fn resize_to_long_edge(img: DynamicImage, long_edge: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if long_edge == 0 || (w.max(h) <= long_edge) {
        return img;
    }
    let scale = long_edge as f32 / (w.max(h) as f32);
    let nw = ((w as f32) * scale).round().max(1.0) as u32;
    let nh = ((h as f32) * scale).round().max(1.0) as u32;
    img.resize_exact(nw, nh, FilterType::Lanczos3)
}

fn encode_jpeg(img: &DynamicImage, out: &Path, quality: u8) -> AppResult<()> {
    #[cfg(feature = "mozjpeg")]
    {
        return encode_jpeg_mozjpeg(img, out, quality);
    }
    #[cfg(not(feature = "mozjpeg"))]
    encode_jpeg_image_crate(img, out, quality)
}

fn encode_jpeg_image_crate(img: &DynamicImage, out: &Path, quality: u8) -> AppResult<()> {
    let mut file = std::fs::File::create(out)?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let encoder = JpegEncoder::new_with_quality(&mut file, quality);
    encoder
        .write_image(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("jpeg encode {}: {e}", out.display())))?;
    Ok(())
}

#[cfg(feature = "mozjpeg")]
fn encode_jpeg_mozjpeg(img: &DynamicImage, out: &Path, quality: u8) -> AppResult<()> {
    use mozjpeg::{ColorSpace, Compress, ScanMode};
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(w as usize, h as usize);
    comp.set_quality(quality as f32);
    comp.set_scan_optimization_mode(ScanMode::AllComponentsTogether);
    let mut comp = comp.start_compress(Vec::new()).map_err(|e| {
        AppError::Internal(format!("mozjpeg start_compress {}: {e}", out.display()))
    })?;
    comp.write_scanlines(rgb.as_raw())
        .map_err(|e| AppError::Internal(format!("mozjpeg scanlines {}: {e}", out.display())))?;
    let bytes = comp
        .finish()
        .map_err(|e| AppError::Internal(format!("mozjpeg finish {}: {e}", out.display())))?;
    std::fs::write(out, &bytes)
        .map_err(|e| AppError::Internal(format!("write {}: {e}", out.display())))?;
    Ok(())
}

fn encode_tiff(img: &DynamicImage, out: &Path) -> AppResult<()> {
    img.save_with_format(out, ImageFormat::Tiff)
        .map_err(|e| AppError::Internal(format!("tiff encode {}: {e}", out.display())))?;
    Ok(())
}

fn stem_without_ext(filename: &str) -> String {
    Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export")
        .to_string()
}

// Silence unused-import warning in test builds when helpers are test-only.
#[allow(dead_code)]
fn _status_str_for_clippy(s: JobStatus) -> &'static str {
    s.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use image::{ImageBuffer, Rgb};
    use sqlx::Executor;
    use tempfile::TempDir;

    async fn setup_pool_with_photo(tmp: &TempDir) -> (SqlitePool, PathBuf) {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        // Generate a small test JPEG on disk.
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(64, 48, |_x, _y| Rgb([200, 100, 50]));
        let src = tmp.path().join("test.jpg");
        img.save(&src).expect("save src");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw, orientation) \
             VALUES (1, '0000000000000000000000000000000000000000000000000000000000000000', 'test.jpg', 64, 48, '2026-04-26T00:00:00Z', 0, 1)",
        )
        .await
        .expect("seed photo");
        let src_str = src.to_string_lossy().to_string();
        let q = format!(
            "INSERT INTO source_copies (photo_id, source_id, path, verified_sha256, last_seen_at) \
             VALUES (1, 1, '{}', '0000000000000000000000000000000000000000000000000000000000000000', '2026-04-26T00:00:00Z')",
            src_str.replace('\\', "\\\\").replace('\'', "''")
        );
        // Need a source row for FK sanity (phase-0 migration has a FK on source_id).
        pool.execute("INSERT INTO sources (id, name, kind, status, config_json, created_at) VALUES (1, 't', 'local', 'idle', '{}', '2026-04-26T00:00:00Z')").await.expect("seed source");
        pool.execute(q.as_str()).await.expect("seed copy");
        (pool, src)
    }

    #[tokio::test]
    async fn enqueue_creates_job_and_items() {
        let tmp = TempDir::new().unwrap();
        let (pool, _src) = setup_pool_with_photo(&tmp).await;
        let preset = ExportPreset::web_default();
        let out = tmp.path().join("out");
        let job = enqueue_job(&pool, &[1], &preset, &out).await.unwrap();
        assert!(job > 0);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM export_job_items WHERE job_id = ?1")
                .bind(job)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn run_next_item_encodes_jpeg() {
        let tmp = TempDir::new().unwrap();
        let (pool, _src) = setup_pool_with_photo(&tmp).await;
        let preset = ExportPreset {
            long_edge_px: 32,
            ..ExportPreset::web_default()
        };
        let out = tmp.path().join("out");
        let job = enqueue_job(&pool, &[1], &preset, &out).await.unwrap();
        let progress = run_next_item(&pool, job).await.unwrap().expect("progress");
        assert_eq!(progress.item_status, "done");
        let path = progress.output_path.unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        // Second call finalises + returns None.
        let next = run_next_item(&pool, job).await.unwrap();
        assert!(next.is_none());
        let status: String = sqlx::query_scalar("SELECT status FROM export_jobs WHERE id = ?1")
            .bind(job)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "done");
    }

    #[tokio::test]
    async fn heic_returns_invalid_input() {
        let tmp = TempDir::new().unwrap();
        let (pool, _src) = setup_pool_with_photo(&tmp).await;
        let preset = ExportPreset {
            format: Format::Heic,
            ..ExportPreset::web_default()
        };
        let out = tmp.path().join("out");
        let job = enqueue_job(&pool, &[1], &preset, &out).await.unwrap();
        let progress = run_next_item(&pool, job).await.unwrap().expect("progress");
        assert_eq!(progress.item_status, "error");
        assert!(progress.error_msg.unwrap().to_lowercase().contains("heic"));
    }
}
