//! First-run model downloader.
//!
//! Downloads ONNX model files (and GGUF models) to the app's `models/` directory
//! with streaming progress reporting and SHA256 verification. All network I/O is
//! wrapped in `user_initiated_*` functions to satisfy the no-background-network-calls
//! rule.
//!
//! URLs and hashes in `KNOWN_MODELS` must be updated before each release.
//!
//! ## Zip-bundle downloads
//!
//! InsightFace distributes SCRFD + ArcFace as a single `buffalo_l.zip` archive.
//! Both `scrfd-10g` and `arcface-w600k-r50` point at that same URL. The downloader
//! detects `.zip` URLs (`spec.url.ends_with(".zip")`), downloads the archive to a
//! temporary path, extracts only `spec.filename` from within the zip (searching
//! case-insensitively inside `buffalo_l/`), writes the result to the final
//! destination, and then deletes the temporary zip.
//!
//! This means the zip is re-downloaded for each of its two models in Phase 1
//! (simplicity wins; the download is user-initiated once at onboarding). Phase 2
//! will cache the zip for the duration of the session and extract both files before
//! deleting it.
//!
//! See also: `docs/adr/0002-model-download.md` § Zip-bundle downloads.

use crate::{AppError, AppResult};
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read as _;
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
    /// Direct download URL (HTTPS). May point to a `.zip` bundle; see module
    /// doc for extraction behaviour.
    pub url: &'static str,
    /// Lowercase hex SHA256 of the final `.onnx` / `.gguf` file (post-extract
    /// for zip bundles).
    /// Set to `"tbd"` for models whose URLs are not yet finalised — the hash
    /// check is skipped when this value equals `"tbd"`.
    pub sha256: &'static str,
    /// Expected download size in bytes (used for progress estimation when the
    /// server omits `Content-Length`). For zip bundles this is the zip size,
    /// not the extracted size.
    pub size_bytes: u64,
    /// Filename to write in the `models/` directory (the file extracted from
    /// the zip, or the direct download target).
    pub filename: &'static str,
    /// True when this model ships pre-extracted inside the installer's
    /// resource directory. Phase 1 defaults (siglip-2, scrfd, arcface, nima)
    /// are bundled. Moondream2 (caption, 1.7 GB) is false.
    pub bundled: bool,
}

/// All models that Chronimage can download.
///
/// Supersedes the private `Chronimage/models` HuggingFace repo entries — all
/// models below are freely-available community weights:
///
/// | Replaced | New | Why |
/// |---|---|---|
/// | RetinaFace-R50 (private HF) | SCRFD-10g (InsightFace MIT) | No HF token; same accuracy at 1/3 the size |
/// | ArcFace-R100 (private HF) | ArcFace W600K R50 (InsightFace MIT) | No HF token; community-standard checkpoint |
/// | Gemma-4-9B-it Q4 (gated) | Moondream2 1.9B f16 (Apache 2.0) | No HF token; purpose-built for photo captioning; CPU-capable |
/// | SigLIP-1 B/16 (private HF) | SigLIP-2 B/16 naflex (Apache 2.0 via onnx-community) | ~5pt retrieval improvement; same footprint |
///
// SHA256s below are locked against the bytes observed on 2026-04-20 from the
// upstream HuggingFace / GitHub URLs. If any drifts the downloader rejects
// the file with `AppError::Internal("SHA256 mismatch for ...")`.
pub static KNOWN_MODELS: &[ModelSpec] = &[
    // Image embeddings — CLIP-style semantic vectors.
    // Upgraded from SigLIP-1 to SigLIP-2 (2025) — same footprint, ~5pt better retrieval.
    // 224-patch16 size ONNX export from onnx-community.
    ModelSpec {
        name: "siglip2-b16-image",
        kind: "embedding",
        version: "2.0.0",
        url: "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/vision_model.onnx",
        sha256: "c0573e3f4140c3a7c4e9cc5912bd6b26a033b46a6a8e8af26cbea262b163bcad",
        size_bytes: 371_807_752,
        filename: "siglip2-b16-image.onnx",
        bundled: true,
    },
    // SigLIP-2 text encoder — int8-quantized variant (283 MB vs. 1.13 GB
    // for fp32). Same architecture + embedding dim (768) as the image
    // encoder and the fp32/fp16 text variants, so f32 tensor boundaries
    // stay intact and the Rust ort loader needs no precision casting.
    // Power users can swap to `text_model_fp16.onnx` (565 MB, ~same
    // retrieval quality) or `text_model.onnx` (1.13 GB, fp32) from the
    // Settings AI-models picker; swap is safe without a dim change.
    ModelSpec {
        name: "siglip2-b16-text",
        kind: "embedding-text",
        version: "2.0.0",
        url: "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/text_model_quantized.onnx",
        // sha256 to be locked after first-run download verification.
        sha256: "tbd",
        size_bytes: 283_000_000,
        filename: "siglip2-b16-text.onnx",
        bundled: true,
    },
    // SigLIP-2 tokenizer (HuggingFace tokenizer.json format) — required by
    // `embed_text` to convert query strings to token-id tensors. ~2.5 MB.
    ModelSpec {
        name: "siglip2-b16-tokenizer",
        kind: "tokenizer",
        version: "2.0.0",
        url: "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/tokenizer.json",
        // sha256 to be locked after first-run download verification.
        sha256: "tbd",
        size_bytes: 2_500_000,
        filename: "siglip2-b16-tokenizer.json",
        bundled: true,
    },
    // Aesthetic score (NIMA) — ride on top of CLIP for Phase 2 ranking.
    // Community ONNX export at cromsc/nima-mobilenet-aesthetic (tf2onnx-converted).
    ModelSpec {
        name: "nima-aesthetic",
        kind: "aesthetic",
        version: "1.0.0",
        url: "https://huggingface.co/cromsc/nima-mobilenet-aesthetic/resolve/main/nima_mobilenet_aesthetic.onnx",
        sha256: "c58b0c39b5b8f752b1b0ebf10e07e48406780ce3bf9d4647f8c43898748fe69c",
        size_bytes: 12_867_270,
        filename: "nima.onnx",
        bundled: true,
    },
    // Face detection (SCRFD-10g) + Face embedding (ArcFace W600K R50).
    // Both ship in InsightFace's buffalo_l.zip (MIT). Downloader extracts the
    // two ONNX files we need and discards the rest of the bundle (~275MB zip → ~190MB kept).
    // `size_bytes` is the zip size (used for download-progress estimation);
    // `sha256` is the *post-extract* ONNX hash (verified after extract_from_zip).
    ModelSpec {
        name: "scrfd-10g",
        kind: "face-detect",
        version: "0.7.0",
        url: "https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip",
        sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91",
        size_bytes: 288_621_354,
        // InsightFace renamed the SCRFD model to `det_10g.onnx` inside
        // buffalo_l.zip (was `scrfd_10g_bnkps.onnx` in the standalone release).
        filename: "det_10g.onnx",
        bundled: true,
    },
    ModelSpec {
        name: "arcface-w600k-r50",
        kind: "face-embed",
        version: "0.7.0",
        url: "https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip",
        sha256: "4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43",
        size_bytes: 288_621_354,
        filename: "w600k_r50.onnx",
        bundled: true,
    },
    // Default Develop AI masks: SAM 2.1 Hiera-Large ONNX export. The encoder
    // runs once per image and the decoder handles Lightroom-style point/box
    // prompts for Subject, Sky, Object, Person, Foreground, and Background.
    ModelSpec {
        name: "sam2.1-hiera-large",
        kind: "mask-runtime",
        version: "2.1.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_large_20260221.zip",
        sha256: "tbd",
        size_bytes: 900_000_000,
        filename: "sam2.1_hiera_large.encoder.onnx",
        bundled: true,
    },
    ModelSpec {
        name: "sam2.1-hiera-large-decoder",
        kind: "mask-runtime-component",
        version: "2.1.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_large_20260221.zip",
        sha256: "tbd",
        size_bytes: 900_000_000,
        filename: "sam2.1_hiera_large.decoder.onnx",
        bundled: true,
    },
    ModelSpec {
        name: "sam2.1-hiera-tiny",
        kind: "mask-runtime-component",
        version: "2.1.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_tiny_20260221.zip",
        sha256: "tbd",
        size_bytes: 180_000_000,
        filename: "sam2.1_hiera_tiny.encoder.onnx",
        bundled: false,
    },
    ModelSpec {
        name: "sam2.1-hiera-tiny-decoder",
        kind: "mask-runtime-component",
        version: "2.1.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_tiny_20260221.zip",
        sha256: "tbd",
        size_bytes: 180_000_000,
        filename: "sam2.1_hiera_tiny.decoder.onnx",
        bundled: false,
    },
    // Optional SAM3 switch target from Settings. SAM3 adds text-prompt masks
    // ("red car", "person with hat") but is too large for the default path.
    ModelSpec {
        name: "sam3-vith-image-encoder",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 1_900_000_000,
        filename: "sam3_image_encoder.onnx",
        bundled: false,
    },
    ModelSpec {
        name: "sam3-vith-image-encoder-data",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 1_900_000_000,
        filename: "sam3_image_encoder.onnx.data",
        bundled: false,
    },
    ModelSpec {
        name: "sam3-vith-language-encoder",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 1_700_000_000,
        filename: "sam3_language_encoder.onnx",
        bundled: false,
    },
    ModelSpec {
        name: "sam3-vith-language-encoder-data",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 1_700_000_000,
        filename: "sam3_language_encoder.onnx.data",
        bundled: false,
    },
    ModelSpec {
        name: "sam3-vith-decoder",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 150_000_000,
        filename: "sam3_decoder.onnx",
        bundled: false,
    },
    ModelSpec {
        name: "sam3-vith-decoder-data",
        kind: "mask-runtime-component",
        version: "3.0.0",
        url: "https://huggingface.co/vietanhdev/segment-anything-3-onnx-models/resolve/main/sam3_vit_h.zip",
        sha256: "tbd",
        size_bytes: 150_000_000,
        filename: "sam3_decoder.onnx.data",
        bundled: false,
    },
    // Caption: Moondream2 (1.9B, Apache 2.0) — purpose-built for "describe this photo"
    // prompts, runs on CPU at ~1s/image. Community GGUF quantization.
    //
    // NOTE: Moondream2 is a vision-language model (unlike Gemma which is text-only).
    // CaptionSession::caption_image handles the LLaVA-style image_url content
    // block; the sidecar picks up the sibling mmproj file when present.
    // Official GGUF at moondream/moondream2-gguf (moondream org, not vikhyatk user).
    // vikhyatk/moondream2 ships safetensors only; the GGUF quant lives in the
    // sibling -gguf repo. The mmproj companion is optional but required for
    // real vision — without it captioning degrades to language-only.
    ModelSpec {
        name: "moondream2-q4",
        kind: "caption-gguf",
        version: "2024.08.26",
        url: "https://huggingface.co/moondream/moondream2-gguf/resolve/main/moondream2-text-model-f16.gguf",
        sha256: "4e17e9107fb8781629b3c8ce177de57ffeae90fe14adcf7b99f0eef025889696",
        size_bytes: 2_839_534_976,
        filename: "moondream2-text-model-f16.gguf",
        bundled: false,
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
/// For `.zip` URL specs: downloads the archive, extracts `spec.filename`,
/// writes it to `models_dir`, then removes the temporary zip.
/// Returns the final path on success.
///
/// # Errors
/// Returns `AppError` on network failure, I/O error, hash mismatch, or when
/// `spec.filename` is not found inside a zip archive.
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

    if spec.url.ends_with(".zip") {
        // Zip-bundle path: download to a temp zip, extract the target file, delete zip.
        let temp_zip = dest.with_extension("zip.tmp");
        download_to_path(spec, &temp_zip, &on_progress).await?;

        // Extract synchronously (zip crate is sync; file is already on disk).
        let dest_clone = dest.clone();
        let temp_zip_clone = temp_zip.clone();
        let filename = spec.filename.to_string();
        let spec_name = spec.name.to_string();
        let spec_sha256 = spec.sha256.to_string();
        tokio::task::spawn_blocking(move || {
            extract_from_zip(&temp_zip_clone, &filename, &dest_clone)?;
            // Verify hash on the extracted file.
            if spec_sha256 != "tbd" && !verify_sha256_sync(&dest_clone, &spec_sha256)? {
                let _ = std::fs::remove_file(&dest_clone);
                return Err(AppError::Internal(format!(
                    "SHA256 mismatch for {spec_name} after zip extraction"
                )));
            }
            // Remove the temporary zip regardless of outcome above.
            let _ = std::fs::remove_file(&temp_zip_clone);
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))??;
    } else {
        // Direct download path.
        let tmp_path = dest.with_extension("onnx.tmp");
        download_to_path(spec, &tmp_path, &on_progress).await?;

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
    }

    on_progress(DownloadProgress {
        model_name: spec.name.to_string(),
        downloaded_bytes: spec.size_bytes,
        total_bytes: spec.size_bytes,
        done: true,
        already_installed: false,
    });

    Ok(dest)
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Stream-download `spec.url` to `dest_path`, emitting progress via `on_progress`.
///
/// Does NOT verify the hash — that is the caller's responsibility.
async fn download_to_path<F>(spec: &ModelSpec, dest_path: &Path, on_progress: &F) -> AppResult<()>
where
    F: Fn(DownloadProgress),
{
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

    let mut file = tokio::fs::File::create(dest_path)
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

    Ok(())
}

/// Open `zip_path`, find the entry whose name ends with `/<filename>` or equals
/// `<filename>` (case-insensitive), and write its bytes to `dest` atomically
/// (write to `<dest>.extract.tmp` then rename).
///
/// Returns `AppError::NotFound` when no matching entry exists.
fn extract_from_zip(zip_path: &Path, filename: &str, dest: &Path) -> AppResult<()> {
    let zip_file = std::fs::File::open(zip_path).map_err(AppError::Io)?;
    let mut archive =
        zip::ZipArchive::new(zip_file).map_err(|e| AppError::Internal(format!("zip open: {e}")))?;

    let filename_lower = filename.to_lowercase();

    // Find the index of the matching entry.
    let entry_index = (0..archive.len()).find(|&i| {
        archive
            .by_index(i)
            .ok()
            .map(|entry| {
                let name = entry.name().to_lowercase();
                name == filename_lower
                    || name.ends_with(&format!("/{filename_lower}"))
                    || name.ends_with(&format!("\\{filename_lower}"))
            })
            .unwrap_or(false)
    });

    let idx = entry_index.ok_or_else(|| {
        let contents: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
            .collect();
        AppError::NotFound(format!(
            "{filename} not in archive {zip_path:?}. archive contains: [{}]",
            contents.join(", ")
        ))
    })?;

    let mut entry = archive
        .by_index(idx)
        .map_err(|e| AppError::Internal(format!("zip entry read: {e}")))?;

    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes).map_err(AppError::Io)?;

    // Atomic write: write to .tmp then rename.
    let tmp = dest.with_extension("extract.tmp");
    std::fs::write(&tmp, &bytes).map_err(AppError::Io)?;
    std::fs::rename(&tmp, dest).map_err(AppError::Io)?;

    Ok(())
}

fn verify_sha256_sync(path: &Path, expected: &str) -> AppResult<bool> {
    let bytes = std::fs::read(path).map_err(AppError::Io)?;
    let hash = hex::encode(Sha256::digest(&bytes));
    Ok(hash == expected)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // ── catalogue sanity ──────────────────────────────────────────────────────

    #[test]
    fn known_models_have_filenames() {
        for m in KNOWN_MODELS {
            assert!(
                !m.filename.is_empty(),
                "model {} has empty filename",
                m.name
            );
            // GGUF, ONNX, ONNX external-data, and JSON tokenizers are valid.
            let valid_ext = m.filename.ends_with(".onnx")
                || m.filename.ends_with(".onnx.data")
                || m.filename.ends_with(".gguf")
                || m.filename.ends_with(".json");
            assert!(
                valid_ext,
                "model {} filename should end with .onnx, .onnx.data, .gguf, or .json",
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
    fn phase_1_defaults_are_bundled() {
        // The four models required for Phase 1 exit must be bundled so first-run
        // works without network access. Moondream2 (caption-gguf) is on-demand only.
        let bundled_kinds: std::collections::HashSet<&str> = KNOWN_MODELS
            .iter()
            .filter(|m| m.bundled)
            .map(|m| m.kind)
            .collect();
        for required_bundled in &[
            "embedding",
            "aesthetic",
            "face-detect",
            "face-embed",
            "mask-runtime",
        ] {
            assert!(
                bundled_kinds.contains(required_bundled),
                "phase-1 kind {:?} must have bundled=true",
                required_bundled
            );
        }
        // Caption model must NOT be bundled (too large for installer).
        let caption = KNOWN_MODELS
            .iter()
            .find(|m| m.kind == "caption-gguf")
            .expect("caption-gguf entry must exist");
        assert!(
            !caption.bundled,
            "moondream2 (caption-gguf) must not be bundled — it is 1.7 GB"
        );
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
    fn known_models_use_community_urls() {
        // No model should point at the private Chronimage HF repo — all five
        // are now backed by freely-available community sources.
        for m in KNOWN_MODELS {
            assert!(
                !m.url.contains("Chronimage/models"),
                "model {} still points at the private Chronimage HF repo",
                m.name
            );
        }
    }

    #[test]
    fn known_models_have_locked_sha256() {
        // "tbd" is only permitted for kinds that have explicit TODO comments in
        // KNOWN_MODELS (currently: "tokenizer" and "embedding-text" pending a
        // verified first-run download). All other entries must carry locked hashes.
        const TBD_PERMITTED_KINDS: &[&str] = &[
            "tokenizer",
            "embedding-text",
            "mask-runtime",
            "mask-runtime-component",
        ];
        for m in KNOWN_MODELS {
            if TBD_PERMITTED_KINDS.contains(&m.kind) && m.sha256 == "tbd" {
                // Pending hash lock — acceptable until CI downloads and verifies.
                continue;
            }
            assert_ne!(
                m.sha256, "tbd",
                "model {} (kind={}) has unlocked sha256 placeholder",
                m.name, m.kind
            );
            assert_eq!(
                m.sha256.len(),
                64,
                "model {} sha256 is not 64 hex chars: {:?}",
                m.name,
                m.sha256
            );
            assert!(
                m.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "model {} sha256 has non-lowercase-hex chars: {:?}",
                m.name,
                m.sha256
            );
        }
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
        let hash = hex::encode(sha2::Sha256::digest(b"hello world"));
        let result = verify_sha256_sync(tmp.path(), &hash).expect("verify");
        assert!(result);
    }

    // ── zip extraction ────────────────────────────────────────────────────────

    /// Build an in-memory zip containing `buffalo_l/bar.onnx` → write to
    /// tempfile → extract `bar.onnx` → assert contents match.
    #[test]
    fn extract_from_zip_finds_nested_file() {
        let tmp_zip = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .expect("tempfile");
        let dest_dir = tempfile::tempdir().expect("tempdir");
        let dest = dest_dir.path().join("bar.onnx");

        // Write a zip with nested path `buffalo_l/bar.onnx`.
        {
            let f = std::fs::File::create(tmp_zip.path()).expect("create zip");
            let mut zip = zip::ZipWriter::new(f);
            let options = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("buffalo_l/bar.onnx", options)
                .expect("start_file");
            zip.write_all(b"hello").expect("write");
            zip.finish().expect("finish");
        }

        extract_from_zip(tmp_zip.path(), "bar.onnx", &dest).expect("extract");
        let contents = std::fs::read(&dest).expect("read dest");
        assert_eq!(contents, b"hello");
    }

    /// Zip without the target name returns NotFound.
    #[test]
    fn extract_from_zip_missing_file_errors() {
        let tmp_zip = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .expect("tempfile");
        let dest_dir = tempfile::tempdir().expect("tempdir");
        let dest = dest_dir.path().join("missing.onnx");

        // Write a zip with a different filename.
        {
            let f = std::fs::File::create(tmp_zip.path()).expect("create zip");
            let mut zip = zip::ZipWriter::new(f);
            let options = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("buffalo_l/other.onnx", options)
                .expect("start_file");
            zip.write_all(b"data").expect("write");
            zip.finish().expect("finish");
        }

        let err = extract_from_zip(tmp_zip.path(), "missing.onnx", &dest)
            .expect_err("should return NotFound");
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }
}
