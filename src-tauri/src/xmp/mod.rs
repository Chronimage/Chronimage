//! Phase 4 §7 — XMP sidecar import.
//!
//! At import time, for every JPG or RAW we look for a matching
//! `<stem>.xmp` sidecar file and extract:
//! - `xmp:Rating` (0-5) → `photos.rating`
//! - `xmp:Label` (colour name) → `photos.color_label` (lower-cased)
//! - `dc:subject` bag → `tags` rows with `kind='user'`
//!
//! XMP is RDF/XML; we don't need a full RDF parser since we only care
//! about three well-known attributes / nodes under a single
//! `rdf:Description` element. A targeted scan over events from
//! `quick-xml` keeps this under 200 LOC and avoids pulling a serde-xml
//! crate.
//!
//! Write-out (tags/rating → sidecar) is deferred — the PRD's §7 has it
//! behind an opt-in Settings toggle that's not yet wired.

use crate::{AppError, AppResult};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::Path;

/// Metadata pulled out of an XMP packet. All fields optional; an empty
/// [`XmpData`] is returned when nothing relevant was found.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmpData {
    pub rating: Option<i64>,
    pub color_label: Option<String>,
    pub subjects: Vec<String>,
}

impl XmpData {
    pub fn is_empty(&self) -> bool {
        self.rating.is_none() && self.color_label.is_none() && self.subjects.is_empty()
    }
}

/// Allowed XMP colour labels per the Adobe spec. Anything else maps to
/// `None` so we never persist garbage in `photos.color_label`.
fn normalise_label(raw: &str) -> Option<String> {
    let lc = raw.trim().to_lowercase();
    match lc.as_str() {
        "red" | "yellow" | "green" | "blue" | "purple" => Some(lc),
        _ => None,
    }
}

/// Read + parse an XMP sidecar. Returns:
/// - `Ok(Some(data))` with any fields we found
/// - `Ok(None)` when the file doesn't exist
/// - `Err` for read / parse failures
pub fn read_sidecar(path: &Path) -> AppResult<Option<XmpData>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(&bytes).into_owned(),
    };
    Ok(Some(parse_str(&text)))
}

/// Probe `<photo_path>.xmp`, `<photo_path>.XMP`, and `<photo_path_sans_ext>.xmp`
/// — Lightroom writes the bare-stem variant for RAWs (`IMG_0001.xmp` next
/// to `IMG_0001.ARW`). Returns the first hit.
pub fn sidecar_for(photo_path: &Path) -> Option<std::path::PathBuf> {
    // Lightroom form: same stem, `.xmp` extension.
    if let Some(stem) = photo_path.file_stem() {
        if let Some(parent) = photo_path.parent() {
            for ext in ["xmp", "XMP"] {
                let mut candidate = parent.to_path_buf();
                candidate.push(stem);
                candidate.set_extension(ext);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    // Full-filename form: `IMG_0001.ARW.xmp`.
    let original_str = photo_path.to_string_lossy();
    for ext in ["xmp", "XMP"] {
        let candidate = std::path::PathBuf::from(format!("{original_str}.{ext}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Core parser — stream the XML, pluck the attributes / child text we
/// care about. Robust to whitespace, comments, and attribute ordering.
pub fn parse_str(xml: &str) -> XmpData {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut data = XmpData::default();
    let mut in_subject_bag = false;
    let mut in_subject_li = false;
    let mut current_li: String = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                let local = std::str::from_utf8(name.local_name().as_ref())
                    .unwrap_or("")
                    .to_string();
                // rdf:Description carries rating + label + subject as
                // either attributes or child elements.
                if local == "Description" {
                    for attr in e.attributes().with_checks(false).flatten() {
                        let key = std::str::from_utf8(attr.key.local_name().as_ref())
                            .unwrap_or("")
                            .to_string();
                        let val = attr.unescape_value().unwrap_or_default().to_string();
                        apply_attr(&mut data, &key, &val);
                    }
                } else if local == "Rating" && data.rating.is_none() {
                    // Child-element form: <xmp:Rating>4</xmp:Rating>
                    if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                        if let Ok(v) = t.unescape().map(|s| s.trim().to_string()) {
                            if let Ok(n) = v.parse::<i64>() {
                                data.rating = Some(n.clamp(0, 5));
                            }
                        }
                    }
                } else if local == "Label" && data.color_label.is_none() {
                    if let Ok(Event::Text(t)) = reader.read_event_into(&mut buf) {
                        if let Ok(v) = t.unescape().map(|s| s.trim().to_string()) {
                            data.color_label = normalise_label(&v);
                        }
                    }
                } else if local == "subject" {
                    in_subject_bag = true;
                } else if in_subject_bag && local == "li" {
                    in_subject_li = true;
                    current_li.clear();
                }
            }
            Ok(Event::Text(t)) if in_subject_li => {
                if let Ok(v) = t.unescape() {
                    current_li.push_str(v.as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = std::str::from_utf8(name.local_name().as_ref())
                    .unwrap_or("")
                    .to_string();
                if local == "li" && in_subject_li {
                    let s = current_li.trim().to_string();
                    if !s.is_empty() {
                        data.subjects.push(s);
                    }
                    in_subject_li = false;
                } else if local == "subject" {
                    in_subject_bag = false;
                }
            }
            Err(_) => {
                // Bail on malformed XML; we've captured whatever we got.
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    data
}

fn apply_attr(data: &mut XmpData, key: &str, val: &str) {
    match key {
        "Rating" if data.rating.is_none() => {
            if let Ok(n) = val.trim().parse::<i64>() {
                data.rating = Some(n.clamp(0, 5));
            }
        }
        "Label" if data.color_label.is_none() => {
            data.color_label = normalise_label(val);
        }
        _ => {}
    }
}

/// Persist the XMP data onto a photo row. Returns the number of DB
/// writes (1 for rating/label + 1 per subject insert).
pub async fn apply_to_photo(pool: &SqlitePool, photo_id: i64, xmp: &XmpData) -> AppResult<usize> {
    if xmp.is_empty() {
        return Ok(0);
    }
    let mut writes = 0usize;
    let mut tx = pool.begin().await?;

    if let Some(rating) = xmp.rating {
        sqlx::query("UPDATE photos SET rating = ?1 WHERE id = ?2")
            .bind(rating)
            .bind(photo_id)
            .execute(&mut *tx)
            .await?;
        writes += 1;
    }
    if let Some(label) = &xmp.color_label {
        sqlx::query("UPDATE photos SET color_label = ?1 WHERE id = ?2")
            .bind(label)
            .bind(photo_id)
            .execute(&mut *tx)
            .await?;
        writes += 1;
    }
    if !xmp.subjects.is_empty() {
        let now = chrono::Utc::now().to_rfc3339();
        for subject in &xmp.subjects {
            let label = subject.trim();
            if label.is_empty() {
                continue;
            }
            // INSERT OR IGNORE so re-import doesn't duplicate.
            let res = sqlx::query(
                "INSERT OR IGNORE INTO tags (photo_id, label, kind, confidence, created_at) \
                 VALUES (?1, ?2, 'user', 1.0, ?3)",
            )
            .bind(photo_id)
            .bind(label)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            writes += res.rows_affected() as usize;
        }
    }

    tx.commit().await?;
    Ok(writes)
}

/// Best-effort: scan every photo in the catalog, look for a sidecar next
/// to any source_copies path, and apply what we find. User-triggered
/// via a Tauri command so existing catalogs (imported before Phase 4)
/// can pick up sidecar metadata without a re-import.
pub async fn rescan_all(pool: &SqlitePool) -> AppResult<RescanReceipt> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sc.photo_id, sc.path \
         FROM source_copies sc \
         WHERE sc.path IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut scanned = 0usize;
    let mut applied = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (photo_id, path_str) in rows {
        scanned += 1;
        let path = std::path::PathBuf::from(&path_str);
        let Some(sidecar) = sidecar_for(&path) else {
            continue;
        };
        match read_sidecar(&sidecar) {
            Ok(Some(data)) if !data.is_empty() => {
                if let Err(e) = apply_to_photo(pool, photo_id, &data).await {
                    errors.push(format!("{}: {e}", sidecar.display()));
                } else {
                    applied += 1;
                }
            }
            Ok(_) => {}
            Err(e) => errors.push(format!("{}: {e}", sidecar.display())),
        }
    }
    Ok(RescanReceipt {
        scanned,
        applied,
        error_count: errors.len(),
        errors,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RescanReceipt {
    pub scanned: usize,
    pub applied: usize,
    pub error_count: usize,
    pub errors: Vec<String>,
}

// Silence "impossible" warning — `AppError::from` for the `FromStr` parse
// above isn't used but keeps the file's dependency surface ready for
// future xmp::write() wiring.
#[allow(dead_code)]
fn _noop(e: AppError) -> AppError {
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    const SAMPLE: &str = r#"<?xml version='1.0'?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
           xmlns:dc="http://purl.org/dc/elements/1.1/"
           xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description rdf:about="" xmp:Rating="4" xmp:Label="Green">
      <dc:subject>
        <rdf:Bag>
          <rdf:li>portrait</rdf:li>
          <rdf:li>golden-hour</rdf:li>
          <rdf:li></rdf:li>
        </rdf:Bag>
      </dc:subject>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn parses_rating_label_subjects_from_attrs() {
        let d = parse_str(SAMPLE);
        assert_eq!(d.rating, Some(4));
        assert_eq!(d.color_label.as_deref(), Some("green"));
        assert_eq!(
            d.subjects,
            vec!["portrait".to_string(), "golden-hour".into()]
        );
    }

    #[test]
    fn clamps_out_of_range_rating() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
<rdf:RDF><rdf:Description xmp:Rating="9"/></rdf:RDF></x:xmpmeta>"#;
        let d = parse_str(xml);
        assert_eq!(d.rating, Some(5));
    }

    #[test]
    fn rejects_unknown_color_label() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
<rdf:RDF><rdf:Description xmp:Label="Teal"/></rdf:RDF></x:xmpmeta>"#;
        let d = parse_str(xml);
        assert!(d.color_label.is_none());
    }

    #[test]
    fn handles_rating_as_child_element() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
<rdf:RDF><rdf:Description><xmp:Rating>3</xmp:Rating></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let d = parse_str(xml);
        assert_eq!(d.rating, Some(3));
    }

    #[test]
    fn empty_data_is_empty() {
        assert!(XmpData::default().is_empty());
        let d = parse_str("<x:xmpmeta/>");
        assert!(d.is_empty());
    }

    #[tokio::test]
    async fn apply_writes_rating_label_and_tags() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 100, 100, '2026-10-01T00:00:00Z', 0)",
        )
        .await
        .expect("seed");

        let data = parse_str(SAMPLE);
        let writes = apply_to_photo(&pool, 1, &data).await.expect("apply");
        assert!(writes >= 4, "rating + label + 2 subjects = 4 writes");

        let rating: i64 = sqlx::query_scalar("SELECT rating FROM photos WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rating, 4);

        let label: Option<String> =
            sqlx::query_scalar("SELECT color_label FROM photos WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label.as_deref(), Some("green"));

        let tag_labels: Vec<String> = sqlx::query_scalar(
            "SELECT label FROM tags WHERE photo_id = 1 AND kind = 'user' ORDER BY label",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            tag_labels,
            vec!["golden-hour".to_string(), "portrait".into()]
        );
    }

    #[test]
    fn sidecar_for_matches_stem_plus_xmp() {
        let tmp = tempfile::tempdir().unwrap();
        let photo = tmp.path().join("IMG_0001.ARW");
        std::fs::write(&photo, b"raw bytes").unwrap();
        let side = tmp.path().join("IMG_0001.xmp");
        std::fs::write(&side, b"<x:xmpmeta/>").unwrap();
        assert_eq!(sidecar_for(&photo), Some(side));
    }

    #[test]
    fn sidecar_for_matches_full_filename_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let photo = tmp.path().join("IMG_0002.jpg");
        std::fs::write(&photo, b"jpg").unwrap();
        let side = tmp.path().join("IMG_0002.jpg.xmp");
        std::fs::write(&side, b"<x:xmpmeta/>").unwrap();
        assert_eq!(sidecar_for(&photo), Some(side));
    }

    #[test]
    fn sidecar_for_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let photo = tmp.path().join("IMG_0003.jpg");
        std::fs::write(&photo, b"jpg").unwrap();
        assert!(sidecar_for(&photo).is_none());
    }
}
