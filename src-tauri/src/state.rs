use sqlx::SqlitePool;

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
}
