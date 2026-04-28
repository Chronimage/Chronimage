use crate::develop::DevelopDecodeCache;
use crate::prompt::supervisor::Supervisor;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Global application state managed by Tauri. Injected into commands via
/// `tauri::State<'_, AppState>`.
///
/// Intentionally minimal: heavy AI sessions are constructed on-demand by the
/// code paths that actually need them (e.g. import pipeline stage-5 builds its
/// own `FacesSession` per run, the future captioning sidecar will spawn its
/// process lazily). Keeping sessions out of setup-time state means the webview
/// shows immediately on launch instead of waiting seconds for ONNX init.
pub struct AppState {
    pub pool: SqlitePool,
    /// Holds a child process handle for the user-configured generative
    /// sidecar when the user has started it via Settings → Prompt sidecar.
    /// Absent / idle otherwise. Phase 4 §2.
    pub sidecar_proc: Supervisor,
    /// Decoded-pixel LRU shared by every develop slider drag. Keeps the
    /// 2048-px source RGB for the current photo (and the previous one)
    /// resident so the CPU pipeline doesn't re-decode JPEG/RAW every tick.
    pub develop_decode_cache: Arc<DevelopDecodeCache>,
}
