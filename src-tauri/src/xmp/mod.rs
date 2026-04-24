//! Phase 4 §7 — XMP sidecar import + opt-in write-out.
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
//! Write-out — gated behind the `xmp.write_on_change` setting (KV
//! `settings` table, default `false`). When on, every user-driven tag
//! add/remove triggers [`export_for_photo`], which emits a minimal
//! Adobe-compatible sidecar next to the first `source_copies` path.

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

// ── write-out ─────────────────────────────────────────────────────────────

/// KV key used to gate write-out. Default off.
pub const WRITE_ON_CHANGE_KEY: &str = "xmp.write_on_change";

pub async fn is_write_on_change_enabled(pool: &SqlitePool) -> AppResult<bool> {
    let row: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(WRITE_ON_CHANGE_KEY)
        .fetch_optional(pool)
        .await?;
    Ok(matches!(row.as_deref(), Some("1" | "true" | "on")))
}

pub async fn set_write_on_change(pool: &SqlitePool, enabled: bool) -> AppResult<()> {
    let val = if enabled { "1" } else { "0" };
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(WRITE_ON_CHANGE_KEY)
    .bind(val)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Emit a minimal Adobe-compatible XMP packet. Round-trip safe — a
/// subsequent [`parse_str`] yields [`XmpData`] equal to the input.
pub fn serialise(data: &XmpData) -> String {
    let mut descr_attrs = String::new();
    if let Some(r) = data.rating {
        descr_attrs.push_str(&format!(" xmp:Rating=\"{}\"", r.clamp(0, 5)));
    }
    if let Some(lbl) = &data.color_label {
        let cap = capitalise(lbl);
        descr_attrs.push_str(&format!(" xmp:Label=\"{}\"", xml_escape(&cap)));
    }

    let mut subject_block = String::new();
    if !data.subjects.is_empty() {
        subject_block.push_str("      <dc:subject>\n        <rdf:Bag>\n");
        for s in &data.subjects {
            subject_block.push_str(&format!("          <rdf:li>{}</rdf:li>\n", xml_escape(s)));
        }
        subject_block.push_str("        </rdf:Bag>\n      </dc:subject>\n");
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n  \
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"\n           \
xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n           \
xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n    \
<rdf:Description rdf:about=\"\"{descr_attrs}>\n\
{subject_block}    </rdf:Description>\n  </rdf:RDF>\n</x:xmpmeta>\n"
    )
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Write a sidecar file next to `photo_path`. Uses the Lightroom
/// convention (`<stem>.xmp`) so Lightroom / Darktable pick it up.
pub fn write_sidecar(photo_path: &Path, data: &XmpData) -> AppResult<std::path::PathBuf> {
    let stem = photo_path
        .file_stem()
        .ok_or_else(|| AppError::InvalidInput("photo path has no stem".into()))?;
    let parent = photo_path
        .parent()
        .ok_or_else(|| AppError::InvalidInput("photo path has no parent".into()))?;
    let mut sidecar = parent.to_path_buf();
    sidecar.push(stem);
    sidecar.set_extension("xmp");
    std::fs::write(&sidecar, serialise(data))?;
    Ok(sidecar)
}

/// Pull the current rating/label/user-tags for `photo_id` and, if
/// write-out is enabled + a source file exists on disk, write a sidecar.
/// Returns `Ok(None)` when write-out is off or no path is available.
pub async fn export_for_photo(
    pool: &SqlitePool,
    photo_id: i64,
) -> AppResult<Option<std::path::PathBuf>> {
    if !is_write_on_change_enabled(pool).await? {
        return Ok(None);
    }
    let Some((rating, color_label)): Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT rating, color_label FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let subjects: Vec<String> = sqlx::query_scalar(
        "SELECT label FROM tags WHERE photo_id = ?1 AND kind = 'user' ORDER BY label",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await?;
    let data = XmpData {
        rating: if rating > 0 { Some(rating) } else { None },
        color_label,
        subjects,
    };

    let path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
    )
    .bind(photo_id)
    .fetch_optional(pool)
    .await?;
    let Some(path) = path else {
        return Ok(None);
    };
    let photo_path = std::path::PathBuf::from(path);
    if !photo_path.exists() {
        return Ok(None);
    }
    let written = write_sidecar(&photo_path, &data)?;
    Ok(Some(written))
}

/// Force-export every photo in the catalog (regardless of the
/// write-on-change flag). User-triggered via `xmp_export_all` command.
pub async fn export_all(pool: &SqlitePool) -> AppResult<ExportReceipt> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM photos")
        .fetch_all(pool)
        .await?;

    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for id in ids {
        // Read the same data as export_for_photo but bypass the gate.
        let res = force_export(pool, id).await;
        match res {
            Ok(Some(_)) => written += 1,
            Ok(None) => skipped += 1,
            Err(e) => errors.push(format!("photo {id}: {e}")),
        }
    }
    Ok(ExportReceipt {
        written,
        skipped,
        error_count: errors.len(),
        errors,
    })
}

async fn force_export(pool: &SqlitePool, photo_id: i64) -> AppResult<Option<std::path::PathBuf>> {
    let Some((rating, color_label)): Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT rating, color_label FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let subjects: Vec<String> = sqlx::query_scalar(
        "SELECT label FROM tags WHERE photo_id = ?1 AND kind = 'user' ORDER BY label",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await?;
    let data = XmpData {
        rating: if rating > 0 { Some(rating) } else { None },
        color_label,
        subjects,
    };
    if data.is_empty() {
        return Ok(None);
    }

    let path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
    )
    .bind(photo_id)
    .fetch_optional(pool)
    .await?;
    let Some(path) = path else {
        return Ok(None);
    };
    let photo_path = std::path::PathBuf::from(path);
    if !photo_path.exists() {
        return Ok(None);
    }
    let written = write_sidecar(&photo_path, &data)?;
    Ok(Some(written))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReceipt {
    pub written: usize,
    pub skipped: usize,
    pub error_count: usize,
    pub errors: Vec<String>,
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

    #[test]
    fn serialise_roundtrips_through_parse_str() {
        let original = XmpData {
            rating: Some(3),
            color_label: Some("blue".into()),
            subjects: vec!["street".into(), "night & day".into()],
        };
        let xml = serialise(&original);
        let parsed = parse_str(&xml);
        assert_eq!(parsed.rating, Some(3));
        assert_eq!(parsed.color_label.as_deref(), Some("blue"));
        assert_eq!(parsed.subjects, original.subjects);
    }

    #[test]
    fn write_sidecar_writes_next_to_photo() {
        let tmp = tempfile::tempdir().unwrap();
        let photo = tmp.path().join("IMG_9999.jpg");
        std::fs::write(&photo, b"jpg").unwrap();
        let data = XmpData {
            rating: Some(5),
            color_label: None,
            subjects: vec!["keep".into()],
        };
        let side = write_sidecar(&photo, &data).unwrap();
        assert_eq!(side, tmp.path().join("IMG_9999.xmp"));
        let text = std::fs::read_to_string(&side).unwrap();
        assert!(text.contains("xmp:Rating=\"5\""));
        assert!(text.contains("<rdf:li>keep</rdf:li>"));
    }

    #[tokio::test]
    async fn write_on_change_default_off_and_toggles() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        assert!(!is_write_on_change_enabled(&pool).await.unwrap());
        set_write_on_change(&pool, true).await.unwrap();
        assert!(is_write_on_change_enabled(&pool).await.unwrap());
        set_write_on_change(&pool, false).await.unwrap();
        assert!(!is_write_on_change_enabled(&pool).await.unwrap());
    }
}
