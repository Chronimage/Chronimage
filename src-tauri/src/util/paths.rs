//! Canonical app-data paths.

use crate::{AppError, AppResult, APP_ID};
use std::path::PathBuf;
use tauri::Manager as _;

/// Per-user app data directory. Falls back to the current directory only in
/// tests (where `dirs::data_local_dir` may be unset).
pub fn app_data_dir() -> AppResult<PathBuf> {
    if let Some(d) = dirs::data_local_dir() {
        Ok(d.join(APP_ID))
    } else {
        Err(AppError::Internal("data_local_dir unavailable".into()))
    }
}

/// Path to the catalog SQLite database.
pub fn catalog_db_path() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("catalog.db"))
}

/// Path to the model cache directory.
///
/// Respects the `CHRONIMAGE_MODELS_DIR` env override when set. This lets
/// integration tests point at an empty tempdir so stage-4 AI enrichment
/// (NIMA / SigLIP) short-circuits instead of loading the developer's
/// real multi-hundred-MB models and running inference on synthetic fixtures.
pub fn models_dir() -> AppResult<PathBuf> {
    if let Ok(override_path) = std::env::var("CHRONIMAGE_MODELS_DIR") {
        return Ok(PathBuf::from(override_path));
    }
    Ok(app_data_dir()?.join("models"))
}

/// Resolve the installer's bundled-models resource directory.
///
/// In production this resolves to `<resource_dir>/models/bundled/` via Tauri's
/// path API, which maps to the resource directory declared in `tauri.conf.json`.
///
/// In tests and dev builds, `CHRONIMAGE_BUNDLED_MODELS_DIR` overrides the path
/// so callers can seed a tempdir with fake model files without requiring a
/// full Tauri runtime. Returns `None` when neither the env override nor a
/// valid Tauri resource path is available (e.g. during `cargo test` without
/// the env var).
pub fn bundled_models_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    // Test / CI override — mirrors the CHRONIMAGE_MODELS_DIR pattern.
    if let Ok(override_path) = std::env::var("CHRONIMAGE_BUNDLED_MODELS_DIR") {
        return Some(PathBuf::from(override_path));
    }
    // Production: resolve via Tauri's resource directory.
    app.path()
        .resolve("models/bundled", tauri::path::BaseDirectory::Resource)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_absolute_and_contain_app_id() {
        let d = app_data_dir().expect("data dir");
        assert!(d.is_absolute());
        assert!(d.to_string_lossy().contains(APP_ID));
    }

    #[test]
    fn catalog_path_lives_under_data_dir() {
        let p = catalog_db_path().expect("catalog path");
        let d = app_data_dir().expect("data dir");
        assert!(p.starts_with(d));
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("catalog.db"));
    }
}
