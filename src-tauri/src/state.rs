use sqlx::SqlitePool;

/// Global application state managed by Tauri. Injected into commands via
/// `tauri::State<'_, AppState>`.
pub struct AppState {
    pub pool: SqlitePool,
}
