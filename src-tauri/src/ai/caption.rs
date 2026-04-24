//! Image captioning via llama.cpp sidecar (Moondream2 1.9B GGUF).
//!
//! ## Overview
//!
//! This module manages a `llama-server.exe` subprocess that exposes an
//! OpenAI-compatible `/v1/chat/completions` endpoint on an ephemeral localhost
//! port. Moondream2 is a **vision-language** model: images are passed alongside
//! the text prompt as LLaVA-style `image_url` content blocks (base64
//! data-URLs).
//!
//! **GPU-only path (Phase 1 policy):** if the detected hardware tier is not
//! `GpuLow` or `GpuHigh`, or if either required file is absent, the session
//! degrades to a stub that returns a placeholder string without spawning any
//! subprocess. Moondream2 is capable of CPU inference (~1 s/image), but the
//! GPU gate is kept for Phase 1 parity; relaxing it is tracked for Phase 2.
//!
//! ## Files expected on disk
//!
//! - Model: `moondream2-text-model-f16.gguf` (downloaded on first run to
//!   `%LOCALAPPDATA%\Chronimage\models\` or the user-chosen catalog root).
//! - Optional vision projector: `moondream2-mmproj-f16.gguf` sibling file.
//!   When present it is passed via `--mmproj`; otherwise the sidecar falls
//!   back to the language-only branch and vision inputs are dropped by
//!   llama.cpp.
//! - Sidecar binary: `llama-server.exe` — declared in `tauri.conf.json →
//!   bundle.externalBin` and resolved at runtime via
//!   `tauri::utils::platform::current_exe()` sibling lookup.
//!
//! ## HTTP flow
//!
//! `caption_image` on a real session:
//!
//! 1. Read the image bytes, sniff MIME from the filename.
//! 2. Base64-encode into a `data:<mime>;base64,…` URL.
//! 3. POST `/v1/chat/completions` with the LLaVA content-blocks shape.
//! 4. Parse `choices[0].message.content` and return the trimmed caption.

use crate::ai::budget::HardwareTier;
use crate::{AppError, AppResult};
use base64::Engine;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Canonical model filename consumed by the model-download manifest.
pub fn model_filename() -> &'static str {
    "moondream2-text-model-f16.gguf"
}

/// Filename of the vision projector companion. Optional — when missing the
/// sidecar runs language-only and `caption_image` will still get a reply,
/// it just won't see the image.
pub fn mmproj_filename() -> &'static str {
    "moondream2-mmproj-f16.gguf"
}

// ── sidecar process handle ────────────────────────────────────────────────────

/// A running `llama-server` subprocess and the localhost port it is listening on.
struct Sidecar {
    child: tokio::process::Child,
    port: u16,
}

impl std::fmt::Debug for Sidecar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sidecar").field("port", &self.port).finish()
    }
}

impl Drop for Sidecar {
    /// Kill the sidecar process if it is still alive. Errors are silently
    /// ignored — the process will be cleaned up by the OS when our PID exits.
    fn drop(&mut self) {
        // `start_kill` is non-blocking; we do not await here.
        let _ = self.child.start_kill();
    }
}

// ── session ───────────────────────────────────────────────────────────────────

/// A llama.cpp captioning session.
///
/// `is_stub == true` whenever the hardware tier is not GPU, either required
/// file is missing, or the sidecar health-probe failed. In stub mode
/// `caption_image` returns a placeholder string and no subprocess is spawned.
#[derive(Debug)]
pub struct CaptionSession {
    sidecar: Mutex<Option<Sidecar>>,
    pub is_stub: bool,
}

impl CaptionSession {
    /// Spawn the llama-server sidecar and probe its `/health` endpoint until
    /// it reports ready or the 30-second timeout elapses.
    ///
    /// On timeout, the spawned process is killed (via `Sidecar::drop`) and
    /// this function returns [`AppError::Internal`] — it does **not** fall
    /// back to a stub. Callers who want a stub fallback should prefer
    /// [`CaptionSession::load_or_stub`].
    pub async fn load(model_path: &Path, llama_server_bin: &Path) -> AppResult<Self> {
        if !model_path.exists() {
            return Err(AppError::NotFound(format!(
                "llama.cpp model not found at {:?} — run model download first",
                model_path
            )));
        }
        if !llama_server_bin.exists() {
            return Err(AppError::NotFound(format!(
                "llama-server binary not found at {:?} — check tauri externalBin config",
                llama_server_bin
            )));
        }

        let port = pick_ephemeral_port();

        // `--mmproj` is only added when the sibling projector exists — llama-
        // server errors on a missing file, whereas omitting the flag
        // gracefully downgrades to language-only.
        let mmproj = model_path.with_file_name(mmproj_filename());
        let mut cmd = tokio::process::Command::new(llama_server_bin);
        cmd.args([
            "-m",
            &model_path.to_string_lossy(),
            "--port",
            &port.to_string(),
            "--host",
            "127.0.0.1",
            "-ngl",
            "999",
        ]);
        if mmproj.exists() {
            cmd.args(["--mmproj", &mmproj.to_string_lossy()]);
        }

        let child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::Internal(format!("failed to spawn llama-server: {e}")))?;

        tracing::info!(
            port,
            model = %model_path.display(),
            bin = %llama_server_bin.display(),
            mmproj_present = mmproj.exists(),
            "CaptionSession: llama-server spawned"
        );

        let sidecar = Sidecar { child, port };
        if let Err(e) = probe_health(port).await {
            // Dropping `sidecar` here kills the child; bubble the error up
            // so the caller knows the sidecar never came online.
            drop(sidecar);
            return Err(e);
        }
        Ok(Self {
            sidecar: Mutex::new(Some(sidecar)),
            is_stub: false,
        })
    }

    /// Try to load a real session; fall back to stub without error.
    ///
    /// Returns stub (no subprocess spawned) if:
    /// - `tier` is `HardwareTier::CpuOnly` (captioning is GPU-only per PRD §5), or
    /// - either `model_path` or `llama_server_bin` is `None`, or
    /// - either path does not exist on disk, or
    /// - the sidecar fails its health probe within 30 s.
    ///
    /// The caller does **not** need to handle the absent-model case.
    pub async fn load_or_stub(
        model_path: Option<&Path>,
        llama_server_bin: Option<&Path>,
        tier: HardwareTier,
    ) -> Self {
        if tier == HardwareTier::CpuOnly {
            tracing::debug!("CaptionSession: CPU-only tier — captioning disabled, using stub");
            return Self::stub();
        }

        let (Some(model), Some(bin)) = (model_path, llama_server_bin) else {
            tracing::debug!("CaptionSession: missing model/bin path — using stub");
            return Self::stub();
        };
        if !model.exists() || !bin.exists() {
            tracing::debug!(
                model_found = model.exists(),
                bin_found = bin.exists(),
                "CaptionSession: model or bin missing on disk — using stub"
            );
            return Self::stub();
        }

        match Self::load(model, bin).await {
            Ok(session) => session,
            Err(e) => {
                tracing::warn!(error = %e, "CaptionSession: load failed, falling back to stub");
                Self::stub()
            }
        }
    }

    /// Caption the image at `image_path`.
    ///
    /// - **Stub session:** returns a placeholder string immediately.
    /// - **Real session:** reads + base64-encodes the image, POSTs to the
    ///   llama-server's `/v1/chat/completions` endpoint with a LLaVA-style
    ///   content-blocks body, and returns the parsed caption.
    pub async fn caption_image(&self, image_path: &Path) -> AppResult<String> {
        if self.is_stub {
            let _ = image_path;
            return Ok(
                "(stub caption \u{2014} enable GPU tier to wire real captioner)".to_string(),
            );
        }

        let port = self.port()?;
        self.caption_image_with_prompt(image_path, DEFAULT_PROMPT, port)
            .await
    }

    async fn caption_image_with_prompt(
        &self,
        image_path: &Path,
        prompt: &str,
        port: u16,
    ) -> AppResult<String> {
        let bytes = std::fs::read(image_path).map_err(|e| {
            AppError::Internal(format!(
                "caption: read {} failed: {e}",
                image_path.display()
            ))
        })?;
        let mime = mime_for_path(image_path);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{mime};base64,{b64}");

        let body = serde_json::json!({
            "model": "local",
            "max_tokens": MAX_TOKENS,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": { "url": data_url },
                    },
                    { "type": "text", "text": prompt },
                ],
            }]
        });

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Chronimage/0.1 caption-sidecar-client")
            .build()
            .map_err(|e| AppError::Internal(format!("caption: client build failed: {e}")))?;

        let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("caption: POST {url} failed: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "caption sidecar returned {status}: {text}"
            )));
        }

        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("caption: JSON parse failed: {e}")))?;
        let first =
            parsed.choices.into_iter().next().ok_or_else(|| {
                AppError::Internal("caption sidecar returned no choices".to_string())
            })?;
        Ok(first.message.content.trim().to_string())
    }

    fn port(&self) -> AppResult<u16> {
        let guard = self
            .sidecar
            .lock()
            .map_err(|_| AppError::Internal("caption sidecar mutex poisoned".into()))?;
        guard
            .as_ref()
            .map(|s| s.port)
            .ok_or_else(|| AppError::Internal("caption sidecar not running".into()))
    }

    fn stub() -> Self {
        Self {
            sidecar: Mutex::new(None),
            is_stub: true,
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Health-probe the llama-server on `port` until `/health` reports 200 OK
/// (or `/v1/models` if health is not mounted). Retries every 250 ms for up
/// to 30 s.
async fn probe_health(port: u16) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(400))
        .build()
        .map_err(|e| AppError::Internal(format!("caption probe client build: {e}")))?;

    let urls = [
        format!("http://127.0.0.1:{port}/health"),
        format!("http://127.0.0.1:{port}/v1/models"),
    ];
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        for url in &urls {
            if let Ok(r) = client.get(url).send().await {
                if r.status().is_success() {
                    tracing::info!(port, url, "caption sidecar health ok");
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(AppError::Internal(format!(
        "caption sidecar did not become healthy on port {port} within 30s"
    )))
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        // Unknown/RAW: default to jpeg. If the caller fed us an .arw the
        // sidecar will reject it; callers should pre-decode to a jpeg
        // thumbnail before handing us the path.
        _ => "image/jpeg",
    }
}

/// Pick an unused TCP port in the ephemeral range by binding to port 0.
fn pick_ephemeral_port() -> u16 {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(18080)
}

// ── model-path helpers (consumed by download manifest) ───────────────────────

/// Return the resolved model path under `model_root`.
pub fn model_path(model_root: &Path) -> PathBuf {
    model_root.join(model_filename())
}

// ── constants / prompts ───────────────────────────────────────────────────────

/// Default prompt sent to the sidecar for scene captioning.
pub const DEFAULT_PROMPT: &str =
    "Describe this photo in one sentence, focusing on subject, location, and mood.";

/// Max output tokens requested from the sidecar. Keep tight — we want one
/// sentence, not an essay.
const MAX_TOKENS: u32 = 120;

// ── HTTP response shapes ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn stub_session_returns_stub_caption() {
        let session = CaptionSession::load_or_stub(None, None, HardwareTier::CpuOnly).await;
        let result = session
            .caption_image(Path::new("/any/photo.jpg"))
            .await
            .expect("stub must not fail");
        assert!(
            result.starts_with("(stub caption"),
            "expected stub-caption prefix, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn load_or_stub_on_cpu_tier_is_always_stub() {
        // Even if both paths pointed to real files, CpuOnly must force stub.
        let fake_model = PathBuf::from("C:/Windows/System32/notepad.exe");
        let fake_bin = PathBuf::from("C:/Windows/System32/cmd.exe");
        let session =
            CaptionSession::load_or_stub(Some(&fake_model), Some(&fake_bin), HardwareTier::CpuOnly)
                .await;
        assert!(
            session.is_stub,
            "CpuOnly tier must always produce a stub session"
        );
        let caption = session
            .caption_image(Path::new("/any/photo.jpg"))
            .await
            .expect("stub must not fail");
        assert!(
            caption.starts_with("(stub caption"),
            "CpuOnly stub caption must start with '(stub caption', got: {caption:?}"
        );
    }

    #[tokio::test]
    async fn load_or_stub_with_missing_files_is_stub() {
        // GPU tier but the paths don't exist → must not spawn anything.
        let missing = PathBuf::from("/nope/moondream2-text-model-f16.gguf");
        let session =
            CaptionSession::load_or_stub(Some(&missing), Some(&missing), HardwareTier::GpuLow)
                .await;
        assert!(
            session.is_stub,
            "missing files must downgrade to stub, not panic"
        );
    }

    #[tokio::test]
    async fn load_with_missing_model_errors() {
        let missing_model = PathBuf::from("/nonexistent/moondream2-text-model-f16.gguf");
        let existing_bin = PathBuf::from("C:/Windows/System32/cmd.exe");
        let err = CaptionSession::load(&missing_model, &existing_bin)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected AppError::NotFound for missing model, got: {err:?}"
        );
    }

    #[test]
    fn model_filename_is_moondream2_gguf() {
        assert_eq!(model_filename(), "moondream2-text-model-f16.gguf");
    }

    #[test]
    fn mmproj_filename_is_sibling() {
        assert_eq!(mmproj_filename(), "moondream2-mmproj-f16.gguf");
    }

    #[test]
    fn mime_for_path_picks_expected_mime() {
        assert_eq!(mime_for_path(Path::new("a/b.jpg")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("a/b.JPEG")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("a/b.png")), "image/png");
        assert_eq!(mime_for_path(Path::new("a/b.webp")), "image/webp");
        assert_eq!(mime_for_path(Path::new("a/b.avif")), "image/avif");
        assert_eq!(mime_for_path(Path::new("a/b.arw")), "image/jpeg"); // fallback
    }
}
