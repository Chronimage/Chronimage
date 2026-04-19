//! Google Photos Takeout sidecar enrichment.
//!
//! Google Takeout exports each photo alongside a `<filename>.json` sidecar
//! (e.g. `IMG_1234.jpg.json`) that carries metadata not embedded in the file:
//! `photoTakenTime`, GPS coordinates, and the original title. This module
//! reads those sidecars after the core pipeline has run and enriches the
//! catalog rows with the richer metadata.

use serde::Deserialize;
use sqlx::SqlitePool;
use std::path::Path;

/// Subset of the Google Takeout sidecar JSON we care about.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TakeoutSidecar {
    photo_taken_time: Option<TakeoutTimestamp>,
    geo_data: Option<TakeoutGeo>,
    geo_data_exif: Option<TakeoutGeo>,
}

#[derive(Debug, Deserialize)]
struct TakeoutTimestamp {
    /// Unix epoch seconds as a string (Google uses strings for int64).
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TakeoutGeo {
    latitude: Option<f64>,
    longitude: Option<f64>,
}

/// Parsed takeout metadata ready to apply to a DB row.
#[derive(Debug, Default)]
struct SidecarMeta {
    captured_at: Option<String>,
    gps_lat: Option<f64>,
    gps_lng: Option<f64>,
}

/// Read a single `<photo>.json` sidecar and extract relevant fields.
fn read_sidecar(json_path: &Path) -> Option<SidecarMeta> {
    let text = std::fs::read_to_string(json_path).ok()?;
    let sidecar: TakeoutSidecar = serde_json::from_str(&text).ok()?;

    let captured_at = sidecar.photo_taken_time.as_ref().and_then(|t| {
        let ts: i64 = t.timestamp.as_deref()?.parse().ok()?;
        let dt = chrono::DateTime::from_timestamp(ts, 0)?;
        Some(dt.to_rfc3339())
    });

    // Prefer geoDataExif when present (EXIF-derived is more accurate).
    let geo = sidecar.geo_data_exif.as_ref().or(sidecar.geo_data.as_ref());
    let (gps_lat, gps_lng) = match geo {
        Some(g) if g.latitude.is_some() && g.latitude.unwrap_or(0.0).abs() > 1e-6 => {
            (g.latitude, g.longitude)
        }
        _ => (None, None),
    };

    Some(SidecarMeta {
        captured_at,
        gps_lat,
        gps_lng,
    })
}

/// Walk `root` for `*.json` sidecar files and apply their metadata to any
/// matching `photos` row in the catalog.
///
/// The match key is the base filename (sidecar `foo.jpg.json` → `foo.jpg`).
/// If `captured_at` in the DB is already set, we leave it alone — EXIF takes
/// precedence over Takeout's server timestamp.
pub async fn enrich_from_sidecars(pool: &SqlitePool, root: &Path) -> crate::AppResult<u64> {
    let entries = tokio::task::spawn_blocking({
        let root = root.to_path_buf();
        move || collect_sidecars(&root)
    })
    .await
    .map_err(|e| crate::AppError::Internal(format!("sidecar scan join: {e}")))?;

    let mut enriched: u64 = 0;

    for (filename, json_path) in entries {
        let Some(meta) = read_sidecar(&json_path) else {
            continue;
        };

        // Only update captured_at when the DB row has none (EXIF wins).
        let rows_affected = sqlx::query(
            "UPDATE photos SET \
               captured_at = COALESCE(captured_at, ?1), \
               gps_lat     = COALESCE(gps_lat,     ?2), \
               gps_lng     = COALESCE(gps_lng,     ?3) \
             WHERE filename = ?4 \
               AND (?1 IS NOT NULL OR ?2 IS NOT NULL)",
        )
        .bind(meta.captured_at)
        .bind(meta.gps_lat)
        .bind(meta.gps_lng)
        .bind(&filename)
        .execute(pool)
        .await?
        .rows_affected();

        enriched += rows_affected;
    }

    Ok(enriched)
}

/// Collect all `*.json` sidecar paths under `root`.
/// Returns `(photo_filename, sidecar_path)` pairs.
fn collect_sidecars(root: &Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        let path = e.path();
        // Only JSON files whose name ends in ".json"
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        // Strip the trailing ".json" to recover the photo filename.
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            out.push((stem.to_owned(), path.to_path_buf()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    fn write_json(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_timestamp_and_geo() {
        let f = write_json(
            r#"{
              "photoTakenTime": { "timestamp": "1609459200", "formatted": "Jan 1, 2021, 12:00:00 AM UTC" },
              "geoData": { "latitude": 37.7749, "longitude": -122.4194, "altitude": 0.0 },
              "geoDataExif": { "latitude": 37.7750, "longitude": -122.4190, "altitude": 0.0 }
            }"#,
        );
        let meta = read_sidecar(f.path()).unwrap();
        assert!(meta
            .captured_at
            .as_deref()
            .unwrap()
            .starts_with("2021-01-01"));
        // geoDataExif preferred
        assert!((meta.gps_lat.unwrap() - 37.7750).abs() < 1e-4);
        assert!((meta.gps_lng.unwrap() - (-122.4190)).abs() < 1e-4);
    }

    #[test]
    fn skips_zero_geo() {
        let f = write_json(r#"{"geoData": {"latitude": 0.0, "longitude": 0.0}}"#);
        let meta = read_sidecar(f.path()).unwrap();
        assert!(meta.gps_lat.is_none());
        assert!(meta.gps_lng.is_none());
    }

    #[test]
    fn returns_none_on_invalid_json() {
        let f = write_json("not json at all");
        assert!(read_sidecar(f.path()).is_none());
    }

    #[test]
    fn collect_sidecars_finds_json_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("IMG_001.jpg.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("IMG_001.jpg"), b"fake").unwrap();
        let found = collect_sidecars(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "IMG_001.jpg");
    }
}
