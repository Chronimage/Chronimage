//! Image captioning via llama.cpp sidecar (Moondream2 1.9B GGUF).
//!
//! ## Overview
//!
//! This module manages a `llama-server.exe` subprocess that exposes an
//! OpenAI-compatible `/v1/chat/completions` endpoint on an ephemeral localhost
//! port. Moondream2 is a **vision-language** model (unlike Gemma which is
//! text-only), so images must be passed as base64 data-URLs using the LLaVA-style
//! `image_url` content block alongside the text prompt.
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
//! - Sidecar binary: `llama-server.exe` — declared in `tauri.conf.json →
//!   bundle.externalBin` and resolved at runtime via
//!   `tauri::utils::platform::current_exe()` sibling lookup.
//!
//! TODO(cc): Moondream2 requires the image path (or base64) to be passed to
//! the sidecar. Update `CaptionSession::load` in Phase-1b to send the
//! LLaVA-style `image_url` content block alongside the text prompt. The
//! `moondream2-mmproj-*.gguf` projector file may also be required depending on
//! the llama.cpp build; check InsightFace / llama.cpp Moondream2 docs.
//!
//! ## Phase-1b wiring plan
//!
//! When `caption_image` is called on a real (non-stub) session:
//!
//! 1. Base64-encode the image file bytes.
//! 2. Build a `POST /v1/chat/completions` body:
//!    ```json
//!    {
//!      "model": "local",
//!      "messages": [{
//!        "role": "user",
//!        "content": [
//!          { "type": "image_url",
//!            "image_url": { "url": "data:image/jpeg;base64,<B64>" } },
//!          { "type": "text", "text": "<PROMPT>" }
//!        ]
//!      }],
//!      "max_tokens": 120
//!    }
//!    ```
//! 3. POST to `http://127.0.0.1:<port>/v1/chat/completions` via `reqwest`.
//! 4. Parse `response.choices[0].message.content` as the caption string.

use crate::ai::budget::HardwareTier;
use crate::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Canonical model filename consumed by the model-download manifest.
pub fn model_filename() -> &'static str {
    "moondream2-text-model-f16.gguf"
}

// ── sidecar process handle ────────────────────────────────────────────────────

/// A running `llama-server` subprocess and the localhost port it is listening on.
///
/// Private to this module. Created by [`CaptionSession::load`]; dropped (and
/// killed) when the owning `CaptionSession` is dropped.
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
/// `is_stub == true` whenever the hardware tier is not GPU, or when either
/// required file (GGUF model or sidecar binary) is absent. In stub mode
/// `caption_image` returns a placeholder string and the subprocess is never
/// spawned.
///
/// `Mutex<Option<Sidecar>>` allows an immutable `&CaptionSession` (held in an
/// `OnceLock` or Arc) to share the session across threads.
#[derive(Debug)]
pub struct CaptionSession {
    sidecar: Mutex<Option<Sidecar>>,
    /// `true` when the sidecar is not running and captions return a placeholder.
    pub is_stub: bool,
}

impl CaptionSession {
    /// Initialise a real caption session.
    ///
    /// Validates that both `model_path` and `llama_server_bin` exist on disk,
    /// then spawns the sidecar subprocess on an ephemeral port and stubs the
    /// HTTP health probe (real HTTP wiring deferred to Phase 1b).
    ///
    /// Returns `AppError::NotFound` when either file is missing.
    pub fn load(model_path: &Path, llama_server_bin: &Path) -> AppResult<Self> {
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

        // Spawn the sidecar subprocess.
        //
        // Flags:
        //   -m  <model>       — GGUF model path
        //   --port <port>     — listen port
        //   --host 127.0.0.1  — loopback only (security)
        //   -ngl 999          — offload all layers to GPU
        let child = tokio::process::Command::new(llama_server_bin)
            .args([
                "-m",
                &model_path.to_string_lossy(),
                "--port",
                &port.to_string(),
                "--host",
                "127.0.0.1",
                "-ngl",
                "999",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::Internal(format!("failed to spawn llama-server: {e}")))?;

        tracing::info!(
            "CaptionSession: sidecar spawned on port {port} \
             (model={:?}, bin={:?})",
            model_path,
            llama_server_bin
        );

        // TODO(cc): probe GET http://127.0.0.1:{port}/health and retry ≤ 30 s
        // before returning Ok. Requires an async context; deferred to Phase 1b
        // when the Tauri setup hook is async.

        Ok(Self {
            sidecar: Mutex::new(Some(Sidecar { child, port })),
            is_stub: false,
        })
    }

    /// Try to load a real session; fall back to stub without error.
    ///
    /// Returns stub (no subprocess spawned) if:
    /// - `tier` is `HardwareTier::CpuOnly` (captioning is GPU-only per PRD §5), or
    /// - either `model_path` or `llama_server_bin` is `None`, or
    /// - either path does not exist on disk.
    ///
    /// The caller does **not** need to handle the absent-model case.
    pub fn load_or_stub(
        model_path: Option<&Path>,
        llama_server_bin: Option<&Path>,
        tier: HardwareTier,
    ) -> Self {
        // GPU gate: captioning is disabled on CPU-only hardware (PRD §5).
        if tier == HardwareTier::CpuOnly {
            tracing::debug!("CaptionSession: CPU-only tier — captioning disabled, using stub");
            return Self::stub();
        }

        let model_exists = model_path.map(|p| p.exists()).unwrap_or(false);
        let bin_exists = llama_server_bin.map(|p| p.exists()).unwrap_or(false);

        if model_exists && bin_exists {
            tracing::info!(
                "CaptionSession: model found at {:?}, bin at {:?} \
                 (stub load — sidecar wiring deferred to phase-1b)",
                model_path,
                llama_server_bin
            );
        } else {
            tracing::debug!(
                "CaptionSession: model_found={model_exists}, bin_found={bin_exists} \
                 — using stub"
            );
        }

        // Phase 1: even when both files exist we remain stub. Real subprocess
        // dispatch lands in Phase 1b once the async setup hook is wired.
        Self::stub()
    }

    /// Caption the image at `image_path`.
    ///
    /// - **Stub session:** returns a placeholder string immediately.
    /// - **Real session:** returns `Err(AppError::Internal(…))` with a detailed
    ///   TODO comment explaining the planned HTTP flow (Phase 1b).
    pub async fn caption_image(&self, image_path: &Path) -> AppResult<String> {
        if self.is_stub {
            let _ = image_path; // suppress unused-variable lint
            return Ok(
                "(stub caption \u{2014} enable GPU tier to wire real captioner)".to_string(),
            );
        }

        // Acquire port from sidecar handle.
        let port = {
            let guard = self
                .sidecar
                .lock()
                .map_err(|_| AppError::Internal("caption sidecar mutex poisoned".into()))?;
            guard
                .as_ref()
                .map(|s| s.port)
                .ok_or_else(|| AppError::Internal("caption sidecar not running".into()))?
        };

        let _ = port; // used in Phase 1b HTTP call below

        // TODO(cc): Phase-1b HTTP flow:
        //
        // 1. Read `image_path` to bytes; base64-encode → `b64`.
        // 2. Detect MIME type from extension (jpeg/png/webp/avif).
        // 3. Build reqwest JSON body:
        //    {
        //      "model": "local",
        //      "messages": [{
        //        "role": "user",
        //        "content": [
        //          { "type": "image_url",
        //            "image_url": { "url": "data:<mime>;base64,<b64>" } },
        //          { "type": "text", "text": DEFAULT_PROMPT }
        //        ]
        //      }],
        //      "max_tokens": 120
        //    }
        // 4. POST to http://127.0.0.1:{port}/v1/chat/completions (reqwest client,
        //    timeout 30 s).
        // 5. Deserialise response; return choices[0].message.content as String.
        // 6. Surface HTTP / parse errors as AppError::Internal.
        Err(AppError::Internal(
            "caption inference not yet wired (phase-1b)".into(),
        ))
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn stub() -> Self {
        Self {
            sidecar: Mutex::new(None),
            is_stub: true,
        }
    }
}

/// Pick an unused TCP port in the ephemeral range by binding to port 0.
///
/// Returns a fixed fallback port (18080) if the OS cannot provide one.
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

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn stub_session_returns_stub_caption() {
        let session = CaptionSession::load_or_stub(None, None, HardwareTier::CpuOnly);
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
        // We use non-existent paths; the CPU gate fires before the path check.
        let fake_model = PathBuf::from("C:/Windows/System32/notepad.exe"); // exists on Windows
        let fake_bin = PathBuf::from("C:/Windows/System32/cmd.exe"); // exists on Windows
        let session =
            CaptionSession::load_or_stub(Some(&fake_model), Some(&fake_bin), HardwareTier::CpuOnly);
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

    #[test]
    fn load_with_missing_model_errors() {
        // Model path does not exist.
        let missing_model = PathBuf::from("/nonexistent/moondream2-text-model-f16.gguf");
        // Use a path that does exist for the binary to isolate the model error.
        let existing_bin = PathBuf::from("C:/Windows/System32/cmd.exe");
        let err = CaptionSession::load(&missing_model, &existing_bin).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected AppError::NotFound for missing model, got: {err:?}"
        );
    }

    #[test]
    fn model_filename_is_moondream2_gguf() {
        assert_eq!(model_filename(), "moondream2-text-model-f16.gguf");
    }
}
