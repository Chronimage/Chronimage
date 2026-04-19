//! Central error type for Chronimage. Prefer `AppResult<T>` at every public boundary.

use serde::{Serialize, Serializer};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("internal: {0}")]
    Internal(String),
}

/// Serialize errors as `{ code, message }` so the TS side gets typed data.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (code, message) = match self {
            AppError::Io(_) => ("IO", self.to_string()),
            AppError::Json(_) => ("JSON", self.to_string()),
            AppError::Db(_) => ("DB", self.to_string()),
            AppError::NotFound(_) => ("NOT_FOUND", self.to_string()),
            AppError::InvalidInput(_) => ("INVALID_INPUT", self.to_string()),
            AppError::PermissionDenied(_) => ("PERMISSION_DENIED", self.to_string()),
            AppError::Internal(_) => ("INTERNAL", self.to_string()),
        };
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", code)?;
        s.serialize_field("message", &message)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_code_and_message() {
        let err = AppError::NotFound("photo 42".into());
        let j = serde_json::to_value(&err).expect("serializes");
        assert_eq!(j["code"], "NOT_FOUND");
        assert!(j["message"].as_str().unwrap().contains("photo 42"));
    }

    #[test]
    fn converts_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: AppError = io_err.into();
        assert!(matches!(err, AppError::Io(_)));
    }
}
