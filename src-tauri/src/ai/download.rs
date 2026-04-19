//! First-run model downloader.
//!
//! Downloads ONNX model files to the app's `models/` directory with streaming
//! progress reporting and SHA256 verification. All network I/O is wrapped in
//! `user_initiated_*` functions to satisfy the no-background-network-calls rule.
//!
//! URLs and hashes in `KNOWN_MODELS` must be updated before each release.

use crate::{AppError, AppResult};
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

// ── Model catalogue ───────────────────────────────────────────────────────────

/// Metadata for a single downloadable model.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: &'static str,
    pub kind: &'static str,
    /// Semver-style version string stored in the `models` table.
    pub version: &'static str,
    /// Direct download URL (HTTPS).
    pub url: &'static str,
    /// Lowercase hex SHA256 of the final `.onnx` file.
    /// Set to `"tbd"` for models whose URLs are not yet finalised — the hash
    /// check is skipped when this value equals `"tbd"`.
    pub sha256: &'static str,
    /// Expected file size in bytes (used for progress estimation when the
    /// server omits `Content-Length`).
    pub size_bytes: u64,
    /// Filename to write in the `models/` directory.
    pub filename: &'static str,
}

/// All models that Chronimage can download.
///
/// URLs point to the project's HuggingFace model repo.
/// SHA256 hashes must be updated when model files are re-exported.
pub static KNOWN_MODELS: &[ModelSpec] = &[
    ModelSpec {
        name: "siglip-b16-image",
        kind: "embedding",
        version: "1.0.0",
        url: "https://huggingface.co/Chronimage/models/resolve/main/siglip-b16-image.onnx",
        sha256: "tbd",
        size_bytes: 350_000_000,
        filename: "siglip-b16-image.onnx",
    },
    ModelSpec {
        name: "nima",
        kind: "aesthetic",
        version: "1.0.0",
        url: "https://huggingface.co/Chronimage/models/resolve/main/nima.onnx",
        sha256: "tbd",
        size_bytes: 14_000_000,
        filename: "nima.onnx",
    },
    ModelSpec {
        name: "retinaface-r50",
        kind: "face-detect",
        version: "1.0.0",
        url: "https://huggingface.co/Chronimage/models/resolve/main/retinaface-r50.onnx",
        sha256: "tbd",
        size_bytes: 110_000_000,
        filename: "retinaface-r50.onnx",
    },
    ModelSpec {
        name: "arcface-r100",
        kind: "face-embed",
        version: "1.0.0",
        url: "https://huggingface.co/Chronimage/models/resolve/main/arcface-r100.onnx",
        sha256: "tbd",
        size_bytes: 260_000_000,
        filename: "arcface-r100.onnx",
    },
    ModelSpec {
        name: "gemma-4-9b-it-q4_k_m",
        kind: "caption-gguf",
        version: "1.0.0",
        url: "https://huggingface.co/Chronimage/models/resolve/main/gemma-4-9b-it-q4_k_m.gguf",
        sha256: "tbd",
        size_bytes: 5_800_000_000,
        filename: "gemma-4-9b-it-q4_k_m.gguf",
    },
];

// ── Progress type ─────────────────────────────────────────────────────────────

/// Emitted on `"chronimage://download-progress"` during model downloads.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_name: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    /// True on the final event for this model.
    pub done: bool,
    /// Set when the model was already present and verified — no download needed.
    pub already_installed: bool,
}

// ── Core download logic ───────────────────────────────────────────────────────

/// Download `spec` to `models_dir`, emitting progress via `on_progress`.
///
/// Skips the download if the file already exists and its SHA256 matches.
/// Returns the final path on success.
///
/// # Panics
/// Never panics — all errors are surfaced via `AppResult`.
pub async fn user_initiated_download_model<F>(
    spec: &ModelSpec,
    models_dir: &Path,
    on_progress: F,
) -> AppResult<PathBuf>
where
    F: Fn(DownloadProgress),
{
    std::fs::create_dir_all(models_dir).map_err(AppError::Io)?;

    let dest = models_dir.join(spec.filename);

    // Skip if already installed and hash matches.
    if dest.exists() && (spec.sha256 == "tbd" || verify_sha256_sync(&dest, spec.sha256)?) {
        on_progress(DownloadProgress {
            model_name: spec.name.to_string(),
            downloaded_bytes: spec.size_bytes,
            total_bytes: spec.size_bytes,
            done: true,
            already_installed: true,
        });
        return Ok(dest);
    }

    let client = reqwest::Client::builder()
        .user_agent("Chronimage/0.1")
        .build()
        .map_err(|e| AppError::Internal(format!("reqwest client: {e}")))?;

    let response = client
        .get(spec.url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("download {}: {e}", spec.name)))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "download {} returned HTTP {}",
            spec.name,
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(spec.size_bytes);
    let mut downloaded: u64 = 0;

    let tmp_path = dest.with_extension("onnx.tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(AppError::Io)?;

    let mut stream = response.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| AppError::Internal(format!("stream chunk: {e}")))?;
        file.write_all(&chunk).await.map_err(AppError::Io)?;
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress {
            model_name: spec.name.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: total,
            done: false,
            already_installed: false,
        });
    }
    file.flush().await.map_err(AppError::Io)?;
    drop(file);

    // Verify hash only when it is not the placeholder.
    if spec.sha256 != "tbd" && !verify_sha256_sync(&tmp_path, spec.sha256)? {
        tokio::fs::remove_file(&tmp_path).await.ok();
        return Err(AppError::Internal(format!(
            "SHA256 mismatch for {}",
            spec.name
        )));
    }

    tokio::fs::rename(&tmp_path, &dest)
        .await
        .map_err(AppError::Io)?;

    on_progress(DownloadProgress {
        model_name: spec.name.to_string(),
        downloaded_bytes: total,
        total_bytes: total,
        done: true,
        already_installed: false,
    });

    Ok(dest)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn verify_sha256_sync(path: &Path, expected: &str) -> AppResult<bool> {
    let bytes = std::fs::read(path).map_err(AppError::Io)?;
    let hash = hex::encode(Sha256::digest(&bytes));
    Ok(hash == expected)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_have_filenames() {
        for m in KNOWN_MODELS {
            assert!(
                !m.filename.is_empty(),
                "model {} has empty filename",
                m.name
            );
            // GGUF models use a different extension; all others must be .onnx.
            let valid_ext = m.filename.ends_with(".onnx") || m.filename.ends_with(".gguf");
            assert!(
                valid_ext,
                "model {} filename should end with .onnx or .gguf",
                m.name
            );
        }
    }

    #[test]
    fn known_models_includes_all_phase1_kinds() {
        let kinds: std::collections::HashSet<&str> = KNOWN_MODELS.iter().map(|m| m.kind).collect();
        for required in &[
            "embedding",
            "aesthetic",
            "face-detect",
            "face-embed",
            "caption-gguf",
        ] {
            assert!(
                kinds.contains(required),
                "KNOWN_MODELS missing kind {:?}",
                required
            );
        }
    }

    #[test]
    fn known_models_have_unique_names() {
        let names: Vec<_> = KNOWN_MODELS.iter().map(|m| m.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "duplicate model names");
    }

    #[test]
    fn verify_sha256_wrong_hash_returns_false() {
        let tmp = tempfile::Builder::new()
            .suffix(".bin")
            .tempfile()
            .expect("tempfile");
        std::fs::write(tmp.path(), b"hello world").expect("write");
        let result = verify_sha256_sync(
            tmp.path(),
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("verify");
        assert!(!result);
    }

    #[test]
    fn verify_sha256_correct_hash_returns_true() {
        let tmp = tempfile::Builder::new()
            .suffix(".bin")
            .tempfile()
            .expect("tempfile");
        std::fs::write(tmp.path(), b"hello world").expect("write");
        // SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576b82ec25f16e073ad
        // Actually: b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576b82ec25f16e073ad is wrong
        // Correct SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576b82ec25f16e073ad
        let hash = hex::encode(sha2::Sha256::digest(b"hello world"));
        let result = verify_sha256_sync(tmp.path(), &hash).expect("verify");
        assert!(result);
    }
}
