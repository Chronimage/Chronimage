//! Canonical app-data paths.

use crate::{AppError, AppResult, APP_ID};
use std::path::PathBuf;

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
pub fn models_dir() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("models"))
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
