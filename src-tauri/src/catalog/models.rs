//! Data-model types mirroring the SQLite schema.
//!
//! Keep field names aligned with the migration columns. `catalog-architect`
//! agent edits both in lockstep.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub width: i64,
    pub height: i64,
    pub captured_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    pub is_raw: bool,
    pub paired_photo_id: Option<i64>,
    pub phash: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub aesthetic_score: Option<f64>,
    pub size_bytes: Option<i64>,
    pub raw_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub kind: SourceKind,
    pub status: String,
    pub config_json: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Local,
    External,
    Nas,
    Sd,
    Iphone,
    Android,
    GooglePhotos,
    Icloud,
    Onedrive,
    Dropbox,
}

impl SourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::External => "external",
            Self::Nas => "nas",
            Self::Sd => "sd",
            Self::Iphone => "iphone",
            Self::Android => "android",
            Self::GooglePhotos => "google_photos",
            Self::Icloud => "icloud",
            Self::Onedrive => "onedrive",
            Self::Dropbox => "dropbox",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCopy {
    pub id: i64,
    pub photo_id: i64,
    pub source_id: i64,
    pub external_id: Option<String>,
    pub path: Option<String>,
    pub is_primary: bool,
    pub verified_sha256: Option<String>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub id: i64,
    pub source_id: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total_files: i64,
    pub imported_count: i64,
    pub error_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartAlbum {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tag: Option<String>,
    pub photo_count: i64,
    pub cover_photo_ids: Vec<i64>,
}

/// A face-cluster summary row returned by [`face_clusters_list`].
///
/// `face_count` is the number of faces assigned to this cluster.
/// `cover_photo_id` is the photo that contains the cluster's `cover_face_id`
/// (NULL when the cluster has no faces or `cover_face_id` is not set).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRow {
    pub id: i64,
    pub name: Option<String>,
    pub is_named: bool,
    pub face_count: i64,
    pub cover_photo_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_strings_are_snake_case() {
        assert_eq!(SourceKind::Local.as_str(), "local");
        assert_eq!(SourceKind::GooglePhotos.as_str(), "google_photos");
        assert_eq!(SourceKind::Icloud.as_str(), "icloud");
    }

    #[test]
    fn source_kind_serializes_as_snake_case() {
        let v = serde_json::to_value(SourceKind::GooglePhotos).expect("serialize");
        assert_eq!(v.as_str(), Some("google_photos"));
    }
}
