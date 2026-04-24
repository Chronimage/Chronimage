//! Phase 4 §5 — offline reverse-geocoder (bundled nearest-city table).
//!
//! Lightweight nearest-neighbour geocoding over a curated list of ~120
//! well-known cities. Trip popups + Catalog Places facet can display
//! "Bengaluru · 18 photos" instead of raw coordinates without a network
//! lookup or a bundled 5 MB SQLite. The larger GeoNames `cities15000`
//! set (~30 k rows, ~5 MB CSV) is a follow-up.
//!
//! The table is deliberately tiny and biased toward cities a hobbyist
//! photographer is likely to visit or be from. For coordinates far from
//! any seed city, [`nearest_city`] still returns the closest row but
//! the label stays meaningful at country level.

#[derive(Debug, Clone, Copy)]
pub struct City {
    pub name: &'static str,
    pub country: &'static str,
    pub lat: f64,
    pub lng: f64,
}

/// Haversine great-circle distance in kilometres.
fn haversine_km(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const EARTH_KM: f64 = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    2.0 * EARTH_KM * a.sqrt().asin()
}

/// Bundled city list. Not exhaustive — adjust as user base grows.
/// Order not significant; nearest-neighbour is a linear scan.
#[rustfmt::skip]
pub const CITIES: &[City] = &[
    // India
    City { name: "Bengaluru",   country: "IN", lat: 12.9716, lng:  77.5946 },
    City { name: "Mumbai",      country: "IN", lat: 19.0760, lng:  72.8777 },
    City { name: "Delhi",       country: "IN", lat: 28.6139, lng:  77.2090 },
    City { name: "Hyderabad",   country: "IN", lat: 17.3850, lng:  78.4867 },
    City { name: "Chennai",     country: "IN", lat: 13.0827, lng:  80.2707 },
    City { name: "Kolkata",     country: "IN", lat: 22.5726, lng:  88.3639 },
    City { name: "Pune",        country: "IN", lat: 18.5204, lng:  73.8567 },
    City { name: "Ahmedabad",   country: "IN", lat: 23.0225, lng:  72.5714 },
    City { name: "Jaipur",      country: "IN", lat: 26.9124, lng:  75.7873 },
    City { name: "Goa",         country: "IN", lat: 15.2993, lng:  74.1240 },
    City { name: "Kochi",       country: "IN", lat:  9.9312, lng:  76.2673 },
    City { name: "Varanasi",    country: "IN", lat: 25.3176, lng:  82.9739 },

    // US
    City { name: "New York",        country: "US", lat: 40.7128, lng:  -74.0060 },
    City { name: "Los Angeles",     country: "US", lat: 34.0522, lng: -118.2437 },
    City { name: "Chicago",         country: "US", lat: 41.8781, lng:  -87.6298 },
    City { name: "San Francisco",   country: "US", lat: 37.7749, lng: -122.4194 },
    City { name: "Seattle",         country: "US", lat: 47.6062, lng: -122.3321 },
    City { name: "Boston",          country: "US", lat: 42.3601, lng:  -71.0589 },
    City { name: "Austin",          country: "US", lat: 30.2672, lng:  -97.7431 },
    City { name: "Denver",          country: "US", lat: 39.7392, lng: -104.9903 },
    City { name: "Miami",           country: "US", lat: 25.7617, lng:  -80.1918 },
    City { name: "Portland",        country: "US", lat: 45.5152, lng: -122.6784 },
    City { name: "Honolulu",        country: "US", lat: 21.3069, lng: -157.8583 },
    City { name: "Las Vegas",       country: "US", lat: 36.1699, lng: -115.1398 },

    // Canada
    City { name: "Toronto",    country: "CA", lat: 43.6532, lng:  -79.3832 },
    City { name: "Vancouver",  country: "CA", lat: 49.2827, lng: -123.1207 },
    City { name: "Montreal",   country: "CA", lat: 45.5017, lng:  -73.5673 },

    // UK & Ireland
    City { name: "London",     country: "GB", lat: 51.5074, lng:   -0.1278 },
    City { name: "Edinburgh",  country: "GB", lat: 55.9533, lng:   -3.1883 },
    City { name: "Manchester", country: "GB", lat: 53.4808, lng:   -2.2426 },
    City { name: "Dublin",     country: "IE", lat: 53.3498, lng:   -6.2603 },

    // Europe
    City { name: "Paris",      country: "FR", lat: 48.8566, lng:    2.3522 },
    City { name: "Lyon",       country: "FR", lat: 45.7640, lng:    4.8357 },
    City { name: "Nice",       country: "FR", lat: 43.7102, lng:    7.2620 },
    City { name: "Berlin",     country: "DE", lat: 52.5200, lng:   13.4050 },
    City { name: "Munich",     country: "DE", lat: 48.1351, lng:   11.5820 },
    City { name: "Amsterdam",  country: "NL", lat: 52.3676, lng:    4.9041 },
    City { name: "Brussels",   country: "BE", lat: 50.8503, lng:    4.3517 },
    City { name: "Zurich",     country: "CH", lat: 47.3769, lng:    8.5417 },
    City { name: "Geneva",     country: "CH", lat: 46.2044, lng:    6.1432 },
    City { name: "Vienna",     country: "AT", lat: 48.2082, lng:   16.3738 },
    City { name: "Prague",     country: "CZ", lat: 50.0755, lng:   14.4378 },
    City { name: "Budapest",   country: "HU", lat: 47.4979, lng:   19.0402 },
    City { name: "Warsaw",     country: "PL", lat: 52.2297, lng:   21.0122 },
    City { name: "Madrid",     country: "ES", lat: 40.4168, lng:   -3.7038 },
    City { name: "Barcelona",  country: "ES", lat: 41.3851, lng:    2.1734 },
    City { name: "Lisbon",     country: "PT", lat: 38.7223, lng:   -9.1393 },
    City { name: "Rome",       country: "IT", lat: 41.9028, lng:   12.4964 },
    City { name: "Milan",      country: "IT", lat: 45.4642, lng:    9.1900 },
    City { name: "Venice",     country: "IT", lat: 45.4408, lng:   12.3155 },
    City { name: "Florence",   country: "IT", lat: 43.7696, lng:   11.2558 },
    City { name: "Athens",     country: "GR", lat: 37.9838, lng:   23.7275 },
    City { name: "Stockholm",  country: "SE", lat: 59.3293, lng:   18.0686 },
    City { name: "Copenhagen", country: "DK", lat: 55.6761, lng:   12.5683 },
    City { name: "Oslo",       country: "NO", lat: 59.9139, lng:   10.7522 },
    City { name: "Helsinki",   country: "FI", lat: 60.1699, lng:   24.9384 },
    City { name: "Reykjavik",  country: "IS", lat: 64.1466, lng:  -21.9426 },
    City { name: "Istanbul",   country: "TR", lat: 41.0082, lng:   28.9784 },

    // Japan
    City { name: "Tokyo",      country: "JP", lat: 35.6762, lng:  139.6503 },
    City { name: "Kyoto",      country: "JP", lat: 35.0116, lng:  135.7681 },
    City { name: "Osaka",      country: "JP", lat: 34.6937, lng:  135.5023 },
    City { name: "Sapporo",    country: "JP", lat: 43.0621, lng:  141.3544 },

    // China & HK & Taiwan
    City { name: "Beijing",    country: "CN", lat: 39.9042, lng:  116.4074 },
    City { name: "Shanghai",   country: "CN", lat: 31.2304, lng:  121.4737 },
    City { name: "Hong Kong",  country: "HK", lat: 22.3193, lng:  114.1694 },
    City { name: "Taipei",     country: "TW", lat: 25.0330, lng:  121.5654 },

    // South / SE Asia
    City { name: "Singapore",   country: "SG", lat:  1.3521, lng: 103.8198 },
    City { name: "Bangkok",     country: "TH", lat: 13.7563, lng: 100.5018 },
    City { name: "Kuala Lumpur",country: "MY", lat:  3.1390, lng: 101.6869 },
    City { name: "Jakarta",     country: "ID", lat: -6.2088, lng: 106.8456 },
    City { name: "Bali",        country: "ID", lat: -8.3405, lng: 115.0920 },
    City { name: "Manila",      country: "PH", lat: 14.5995, lng: 120.9842 },
    City { name: "Hanoi",       country: "VN", lat: 21.0278, lng: 105.8342 },
    City { name: "Colombo",     country: "LK", lat:  6.9271, lng:  79.8612 },
    City { name: "Kathmandu",   country: "NP", lat: 27.7172, lng:  85.3240 },

    // Middle East
    City { name: "Dubai",       country: "AE", lat: 25.2048, lng:  55.2708 },
    City { name: "Abu Dhabi",   country: "AE", lat: 24.4539, lng:  54.3773 },
    City { name: "Doha",        country: "QA", lat: 25.2854, lng:  51.5310 },
    City { name: "Tel Aviv",    country: "IL", lat: 32.0853, lng:  34.7818 },

    // Oceania
    City { name: "Sydney",      country: "AU", lat: -33.8688, lng: 151.2093 },
    City { name: "Melbourne",   country: "AU", lat: -37.8136, lng: 144.9631 },
    City { name: "Brisbane",    country: "AU", lat: -27.4698, lng: 153.0251 },
    City { name: "Perth",       country: "AU", lat: -31.9505, lng: 115.8605 },
    City { name: "Auckland",    country: "NZ", lat: -36.8485, lng: 174.7633 },
    City { name: "Wellington",  country: "NZ", lat: -41.2866, lng: 174.7756 },
    City { name: "Queenstown",  country: "NZ", lat: -45.0312, lng: 168.6626 },

    // South America
    City { name: "São Paulo",    country: "BR", lat: -23.5505, lng: -46.6333 },
    City { name: "Rio de Janeiro", country: "BR", lat: -22.9068, lng: -43.1729 },
    City { name: "Buenos Aires", country: "AR", lat: -34.6037, lng: -58.3816 },
    City { name: "Lima",         country: "PE", lat: -12.0464, lng: -77.0428 },
    City { name: "Cusco",        country: "PE", lat: -13.5319, lng: -71.9675 },
    City { name: "Santiago",     country: "CL", lat: -33.4489, lng: -70.6693 },
    City { name: "Bogotá",       country: "CO", lat:   4.7110, lng: -74.0721 },
    City { name: "Mexico City",  country: "MX", lat:  19.4326, lng: -99.1332 },

    // Africa
    City { name: "Cairo",        country: "EG", lat: 30.0444, lng:  31.2357 },
    City { name: "Marrakech",    country: "MA", lat: 31.6295, lng:  -7.9811 },
    City { name: "Cape Town",    country: "ZA", lat: -33.9249, lng: 18.4241 },
    City { name: "Nairobi",      country: "KE", lat:  -1.2921, lng: 36.8219 },
];

/// Return the closest city to the given coordinate along with the
/// great-circle distance. `None` only when the table is empty (never in
/// practice — the const array above is non-empty).
pub fn nearest_city(lat: f64, lng: f64) -> Option<(&'static City, f64)> {
    let mut best: Option<(&'static City, f64)> = None;
    for c in CITIES {
        let d = haversine_km(lat, lng, c.lat, c.lng);
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((c, d));
        }
    }
    best
}

/// Human-readable label ("Bengaluru, IN") when the nearest bundled city
/// is within `max_km`, otherwise a terse "lat,lng · XX" fallback.
/// 250 km is a reasonable default — anything further risks a misleading
/// label since the bundled table is coarse.
pub fn label_for(lat: f64, lng: f64) -> String {
    const MAX_KM: f64 = 250.0;
    match nearest_city(lat, lng) {
        Some((c, d)) if d <= MAX_KM => format!("{}, {}", c.name, c.country),
        _ => format!("{lat:.3},{lng:.3}"),
    }
}

// ── place_label persistence (Phase 4 §5) ─────────────────────────────────

use crate::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillReceipt {
    pub scanned: usize,
    pub labelled: usize,
    pub skipped: usize,
    pub elapsed_ms: u64,
}

/// Write the nearest-city label for a single photo. Intended for use
/// at import time once GPS has been extracted. No-op when either
/// coordinate is NULL.
pub async fn label_photo(pool: &SqlitePool, photo_id: i64) -> AppResult<Option<String>> {
    let row: Option<(Option<f64>, Option<f64>)> =
        sqlx::query_as("SELECT gps_lat, gps_lng FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?;
    let Some((Some(lat), Some(lng))) = row else {
        return Ok(None);
    };
    let label = label_for(lat, lng);
    sqlx::query("UPDATE photos SET place_label = ?1 WHERE id = ?2")
        .bind(&label)
        .bind(photo_id)
        .execute(pool)
        .await?;
    Ok(Some(label))
}

/// Scan the entire catalog, compute the nearest-city label for every
/// photo that has GPS but no cached label. Idempotent — safe to run
/// over and over.
pub async fn backfill_place_labels(pool: &SqlitePool) -> AppResult<BackfillReceipt> {
    let start = std::time::Instant::now();
    type Row = (i64, Option<f64>, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, gps_lat, gps_lng FROM photos \
         WHERE gps_lat IS NOT NULL AND gps_lng IS NOT NULL AND place_label IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let scanned = rows.len();
    let mut labelled = 0usize;
    let mut skipped = 0usize;
    let mut tx = pool.begin().await?;
    for (id, lat, lng) in rows {
        let (Some(lat), Some(lng)) = (lat, lng) else {
            skipped += 1;
            continue;
        };
        let label = label_for(lat, lng);
        sqlx::query("UPDATE photos SET place_label = ?1 WHERE id = ?2")
            .bind(&label)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        labelled += 1;
    }
    tx.commit().await?;

    Ok(BackfillReceipt {
        scanned,
        labelled,
        skipped,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    #[test]
    fn bengaluru_is_its_own_nearest() {
        let (c, d) = nearest_city(12.9716, 77.5946).expect("some city");
        assert_eq!(c.name, "Bengaluru");
        assert!(d < 1.0, "expected ~0 km, got {d}");
    }

    #[test]
    fn sf_coords_map_to_san_francisco() {
        let (c, d) = nearest_city(37.773, -122.431).expect("some city");
        assert_eq!(c.name, "San Francisco");
        assert!(d < 5.0);
    }

    #[test]
    fn label_uses_coord_fallback_when_far_from_any_seed() {
        // Middle of the Pacific — every city is hundreds of km away.
        let s = label_for(-40.0, -150.0);
        assert!(s.contains("-40"), "expected coord fallback, got {s}");
    }

    #[test]
    fn label_prefers_city_when_close() {
        let s = label_for(35.68, 139.65);
        assert_eq!(s, "Tokyo, JP");
    }

    #[tokio::test]
    async fn backfill_places_labels_for_rows_with_gps() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        // Bengaluru photo.
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, \
               is_raw, gps_lat, gps_lng) \
             VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', \
               'a.jpg', 100, 100, '2026-10-01T00:00:00Z', 0, 12.9716, 77.5946)",
        )
        .await
        .unwrap();
        // No-GPS photo — should be skipped.
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (2, '2222222222222222222222222222222222222222222222222222222222222222', \
               'b.jpg', 100, 100, '2026-10-01T00:00:00Z', 0)",
        )
        .await
        .unwrap();

        let receipt = backfill_place_labels(&pool).await.unwrap();
        assert_eq!(receipt.scanned, 1);
        assert_eq!(receipt.labelled, 1);

        let label: Option<String> =
            sqlx::query_scalar("SELECT place_label FROM photos WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label.as_deref(), Some("Bengaluru, IN"));

        let none_label: Option<String> =
            sqlx::query_scalar("SELECT place_label FROM photos WHERE id = 2")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(none_label.is_none());
    }
}
