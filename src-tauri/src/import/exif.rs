use std::path::Path;

/// Extracted EXIF fields written back into the `photos` row.
#[derive(Debug, Default)]
pub struct ExifData {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub captured_at: Option<String>,       // ISO-8601 UTC
    pub captured_at_local: Option<String>, // naive local string from EXIF
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub aperture: Option<f64>,
    pub shutter: Option<String>,
    pub iso: Option<u32>,
    pub focal_mm: Option<f64>,
    pub gps_lat: Option<f64>,
    pub gps_lng: Option<f64>,
    /// TIFF/EXIF Orientation tag (1-8). See `ai::image_util::apply_exif_orientation`.
    /// None when absent or not a u16 Short — callers should treat as 1 (no rotation).
    pub orientation: Option<u32>,
}

/// Read EXIF from `path`. Returns `ExifData::default()` on any error so callers
/// don't need to handle missing/corrupt EXIF specially.
pub fn read(path: &Path) -> ExifData {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return ExifData::default(),
    };
    let mut buf = std::io::BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut buf) {
        Ok(e) => e,
        Err(_) => return ExifData::default(),
    };

    let width = exif_u32(&exif, exif::Tag::ImageWidth)
        .or_else(|| exif_u32(&exif, exif::Tag::PixelXDimension));
    let height = exif_u32(&exif, exif::Tag::ImageLength)
        .or_else(|| exif_u32(&exif, exif::Tag::PixelYDimension));

    let (captured_at, captured_at_local) =
        if let Some(dt_str) = exif_str(&exif, exif::Tag::DateTimeOriginal) {
            let iso = parse_exif_datetime(&dt_str);
            (iso, Some(dt_str))
        } else {
            (None, None)
        };

    let camera_make = exif_str(&exif, exif::Tag::Make).map(|s| s.trim().to_string());
    let camera_model = exif_str(&exif, exif::Tag::Model).map(|s| s.trim().to_string());
    let lens_model = exif_str(&exif, exif::Tag::LensModel).map(|s| s.trim().to_string());

    let aperture = exif
        .get_field(exif::Tag::FNumber, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Rational(ref v) = f.value {
                v.first()
                    .filter(|r| r.denom != 0)
                    .map(|r| r.num as f64 / r.denom as f64)
            } else {
                None
            }
        });

    let shutter = exif
        .get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Rational(ref v) = f.value {
                v.first().map(|r| {
                    if r.num == 1 || r.denom <= r.num {
                        format!("{}/{}", r.num, r.denom)
                    } else {
                        format!("1/{}", r.denom / r.num)
                    }
                })
            } else {
                None
            }
        });

    let iso = exif
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Short(ref v) = f.value {
                v.first().map(|&x| x as u32)
            } else {
                None
            }
        });

    let focal_mm = exif
        .get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Rational(ref v) = f.value {
                v.first()
                    .filter(|r| r.denom != 0)
                    .map(|r| r.num as f64 / r.denom as f64)
            } else {
                None
            }
        });

    let gps_lat = parse_gps_coord(
        &exif,
        exif::Tag::GPSLatitude,
        exif::Tag::GPSLatitudeRef,
        "S",
    );
    let gps_lng = parse_gps_coord(
        &exif,
        exif::Tag::GPSLongitude,
        exif::Tag::GPSLongitudeRef,
        "W",
    );

    let orientation = exif_u32(&exif, exif::Tag::Orientation).filter(|&v| (1..=8).contains(&v));

    ExifData {
        width,
        height,
        captured_at,
        captured_at_local,
        camera_make,
        camera_model,
        lens_model,
        aperture,
        shutter,
        iso,
        focal_mm,
        gps_lat,
        gps_lng,
        orientation,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn exif_u32(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    let f = exif.get_field(tag, exif::In::PRIMARY)?;
    match &f.value {
        exif::Value::Short(v) => v.first().map(|&x| x as u32),
        exif::Value::Long(v) => v.first().copied(),
        _ => None,
    }
}

fn exif_str(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let f = exif.get_field(tag, exif::In::PRIMARY)?;
    match &f.value {
        exif::Value::Ascii(v) => v
            .first()
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.trim_end_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => Some(f.display_value().to_string()).filter(|s| !s.is_empty()),
    }
}

/// "2024:07:15 18:30:00" → "2024-07-15T18:30:00" (no TZ — we treat as local)
fn parse_exif_datetime(s: &str) -> Option<String> {
    // EXIF: "YYYY:MM:DD HH:MM:SS"
    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let (date_part, time_part) = (parts[0], parts[1]);
    // "YYYY:MM:DD" = 10, "HH:MM:SS" = 8
    if date_part.len() != 10 || time_part.len() != 8 {
        return None;
    }
    let date = date_part.replace(':', "-");
    Some(format!("{}T{}", date, time_part))
}

fn parse_gps_coord(
    exif: &exif::Exif,
    tag: exif::Tag,
    ref_tag: exif::Tag,
    neg_ref: &str,
) -> Option<f64> {
    let f = exif.get_field(tag, exif::In::PRIMARY)?;
    let rationals = match &f.value {
        exif::Value::Rational(v) => v,
        _ => return None,
    };
    if rationals.len() < 3 {
        return None;
    }
    let deg = rationals[0].to_f64();
    let min = rationals[1].to_f64();
    let sec = rationals[2].to_f64();
    let mut coord = deg + min / 60.0 + sec / 3600.0;

    if let Some(r) = exif.get_field(ref_tag, exif::In::PRIMARY) {
        let ref_str = match &r.value {
            exif::Value::Ascii(v) => v
                .first()
                .and_then(|b| std::str::from_utf8(b).ok())
                .map(|s| s.trim_end_matches('\0').to_uppercase()),
            _ => None,
        };
        if ref_str.as_deref() == Some(neg_ref) {
            coord = -coord;
        }
    }
    Some(coord)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exif_datetime_converts_correctly() {
        assert_eq!(
            parse_exif_datetime("2024:07:15 18:30:00"),
            Some("2024-07-15T18:30:00".to_string())
        );
    }

    #[test]
    fn parse_exif_datetime_rejects_bad_input() {
        assert_eq!(parse_exif_datetime("not a date"), None);
    }

    #[test]
    fn read_missing_file_returns_default() {
        let d = read(Path::new("/nonexistent/file.jpg"));
        assert!(d.captured_at.is_none());
        assert!(d.camera_make.is_none());
    }
}
