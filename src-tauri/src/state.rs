use crate::ai::{caption::CaptionSession, faces::FacesSession};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Global application state managed by Tauri. Injected into commands via
/// `tauri::State<'_, AppState>`.
pub struct AppState {
    pub pool: SqlitePool,
    /// RetinaFace + ArcFace sessions. Always present; stubs when model files
    /// are absent. `is_stub == true` until model files are downloaded.
    pub faces: Arc<FacesSession>,
    /// llama.cpp caption session. Always present; stubs on CPU-only hardware
    /// or when the GGUF model / sidecar binary are absent.
    pub caption: Arc<CaptionSession>,
}
