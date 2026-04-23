//! Execution-provider selection for ONNX Runtime sessions.
//!
//! All four production ONNX sessions (SigLIP image, SigLIP text, SCRFD,
//! ArcFace, NIMA) call [`session_builder_with_ep`] instead of
//! `Session::builder()` directly.  The helper:
//!
//! 1. Tries DirectML (device 0) on Windows when the hardware tier is
//!    `GpuLow` or `GpuHigh` — this covers any NVIDIA/AMD/Intel dGPU with
//!    VRAM ≥ 4 GB that supports DirectX 12.
//! 2. Falls back to CPU silently if DML registration errors (missing DLL,
//!    D3D12 not available, etc.) — the session still loads and inference
//!    continues on CPU.
//! 3. On non-Windows targets the DML branch is compiled out; only CPU is
//!    ever attempted, so cross-compilation to Linux CI stays clean.
//!
//! A `tracing::info!` line is emitted at session-load time naming the EP
//! actually registered, so users can verify GPU acceleration in the log.

use ort::session::builder::SessionBuilder;

/// Returns a `SessionBuilder` pre-configured with the best available EP.
///
/// On Windows + GPU tier: attempts DirectML (device 0), falls back to CPU.
/// Everywhere else: CPU only.
///
/// `session_name` is used only in log messages (e.g. `"siglip-image"`).
///
/// Returns `ort::Error` on a fatal ORT initialisation failure (extremely rare;
/// usually means the ORT DLL itself could not be loaded).
pub fn session_builder_with_ep(session_name: &str) -> Result<SessionBuilder, ort::Error> {
    let builder = SessionBuilder::new()?;

    #[cfg(target_os = "windows")]
    {
        use crate::ai::budget::{detect, HardwareTier};
        use ort::ep::DirectML;

        let hw = detect();
        if matches!(hw.tier, HardwareTier::GpuLow | HardwareTier::GpuHigh) {
            // Attempt DML; `with_execution_providers` logs internally via ort's
            // tracing integration, but we add our own info line for clarity.
            let ep = DirectML::default().with_device_id(0).build();
            // with_execution_providers returns BuilderResult =
            // Result<SessionBuilder, Error<SessionBuilder>>.  On Err we can
            // recover the builder and fall through to CPU.
            let result = builder.with_execution_providers([ep]);
            match result {
                Ok(b) => {
                    tracing::info!(
                        session = session_name,
                        adapter = %hw.adapter_name,
                        vram_mb = hw.vram_mb,
                        "ONNX session using DirectML EP"
                    );
                    return Ok(b);
                }
                Err(e) => {
                    // Recover the builder from the error value and fall through
                    // to CPU.  Log at warn so the status bar can surface it.
                    tracing::warn!(
                        session = session_name,
                        error = %e,
                        "DirectML EP registration failed — falling back to CPU"
                    );
                    let fallback = e.recover();
                    tracing::info!(
                        session = session_name,
                        "ONNX session using CPU EP (DML fallback)"
                    );
                    return Ok(fallback);
                }
            }
        }

        tracing::info!(
            session = session_name,
            tier = ?hw.tier,
            "ONNX session using CPU EP (below GPU tier)"
        );
        Ok(builder)
    }

    #[cfg(not(target_os = "windows"))]
    {
        tracing::info!(
            session = session_name,
            "ONNX session using CPU EP (non-Windows)"
        );
        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The builder must be obtainable without a real GPU present (e.g. in CI).
    #[test]
    fn session_builder_returns_ok_without_gpu() {
        // This calls detect() internally; on CI that returns CpuOnly which skips
        // the DML branch entirely.  We just need it not to panic or return Err.
        let result = session_builder_with_ep("test-session");
        assert!(result.is_ok(), "session_builder_with_ep returned Err");
    }
}
