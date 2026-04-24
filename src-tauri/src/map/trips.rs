//! Trip clustering: single-pass temporal + spatial grouping of
//! GPS-tagged photos into `trips` rows. Deterministic — same input
//! ordering always produces the same trip set.

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Trip split when `captured_at` gap > this many seconds.
const TIME_GAP_S: i64 = 48 * 60 * 60;
/// Trip split when distance from running centroid > this many km.
const DIST_CUTOFF_KM: f64 = 30.0;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TripRow {
    pub id: i64,
    pub name: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub center_lat: f64,
    pub center_lng: f64,
    pub radius_km: f64,
    pub photo_count: i64,
    pub auto_generated: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecomputeReceipt {
    pub trip_count: i64,
    pub photo_count: i64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone)]
struct InputPhoto {
    id: i64,
    lat: f64,
    lng: f64,
    captured_at: String,
    captured_ts: i64, // seconds
}

#[derive(Debug, Clone)]
struct BuildingTrip {
    photos: Vec<InputPhoto>,
    sum_lat: f64,
    sum_lng: f64,
}

impl BuildingTrip {
    fn centroid(&self) -> (f64, f64) {
        let n = self.photos.len() as f64;
        if n == 0.0 {
            return (0.0, 0.0);
        }
        (self.sum_lat / n, self.sum_lng / n)
    }
    fn push(&mut self, p: InputPhoto) {
        self.sum_lat += p.lat;
        self.sum_lng += p.lng;
        self.photos.push(p);
    }
}

/// Great-circle distance in km between two points on Earth.
fn haversine_km(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6371.0;
    let to_rad = std::f64::consts::PI / 180.0;
    let dlat = (lat2 - lat1) * to_rad;
    let dlng = (lng2 - lng1) * to_rad;
    let a = (dlat / 2.0).sin().powi(2)
        + (lat1 * to_rad).cos() * (lat2 * to_rad).cos() * (dlng / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Recompute all trips. Clears the existing rows, runs the cluster pass,
/// writes `trips` + `trip_photos`. Returns counts + elapsed.
pub async fn recompute_trips(pool: &SqlitePool) -> AppResult<RecomputeReceipt> {
    let start = std::time::Instant::now();

    type PhotoRow = (i64, Option<f64>, Option<f64>, Option<String>);
    let rows: Vec<PhotoRow> = sqlx::query_as(
        "SELECT id, gps_lat, gps_lng, captured_at FROM photos \
         WHERE gps_lat IS NOT NULL AND gps_lng IS NOT NULL AND captured_at IS NOT NULL \
         ORDER BY captured_at ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut inputs: Vec<InputPhoto> = Vec::with_capacity(rows.len());
    for (id, lat, lng, ts_str) in rows {
        let (Some(lat), Some(lng), Some(ts)) = (lat, lng, ts_str) else {
            continue;
        };
        let Some(captured_ts) = parse_ts(&ts) else {
            continue;
        };
        inputs.push(InputPhoto {
            id,
            lat,
            lng,
            captured_at: ts,
            captured_ts,
        });
    }

    let mut built: Vec<BuildingTrip> = Vec::new();
    for p in inputs {
        let start_new = match built.last() {
            None => true,
            Some(cur) => {
                let last = cur.photos.last().expect("non-empty");
                let gap = p.captured_ts - last.captured_ts;
                if gap > TIME_GAP_S {
                    true
                } else {
                    let (clat, clng) = cur.centroid();
                    haversine_km(clat, clng, p.lat, p.lng) > DIST_CUTOFF_KM
                }
            }
        };
        if start_new {
            built.push(BuildingTrip {
                photos: Vec::new(),
                sum_lat: 0.0,
                sum_lng: 0.0,
            });
        }
        built.last_mut().expect("just pushed if missing").push(p);
    }

    // Persist atomically.
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    // trip_photos cascades via FK; clearing `trips` wipes both.
    sqlx::query("DELETE FROM trips").execute(&mut *tx).await?;

    let mut trip_count = 0i64;
    let mut photo_count = 0i64;
    for b in &built {
        if b.photos.is_empty() {
            continue;
        }
        let (clat, clng) = b.centroid();
        let radius_km = b
            .photos
            .iter()
            .map(|p| haversine_km(clat, clng, p.lat, p.lng))
            .fold(0.0f64, f64::max);
        let start_at = b.photos.first().expect("non-empty").captured_at.clone();
        let end_at = b.photos.last().expect("non-empty").captured_at.clone();

        let trip_id: i64 = sqlx::query_scalar(
            "INSERT INTO trips \
             (name, start_at, end_at, center_lat, center_lng, radius_km, photo_count, auto_generated, updated_at) \
             VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6, 1, ?7) RETURNING id",
        )
        .bind(&start_at)
        .bind(&end_at)
        .bind(clat)
        .bind(clng)
        .bind(radius_km)
        .bind(b.photos.len() as i64)
        .bind(&now)
        .fetch_one(&mut *tx)
        .await?;

        for p in &b.photos {
            sqlx::query("INSERT INTO trip_photos (trip_id, photo_id) VALUES (?1, ?2)")
                .bind(trip_id)
                .bind(p.id)
                .execute(&mut *tx)
                .await?;
        }
        trip_count += 1;
        photo_count += b.photos.len() as i64;
    }

    tx.commit().await?;

    Ok(RecomputeReceipt {
        trip_count,
        photo_count,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

pub async fn list_trips(pool: &SqlitePool) -> AppResult<Vec<TripRow>> {
    sqlx::query_as::<_, TripRow>(
        "SELECT id, name, start_at, end_at, center_lat, center_lng, radius_km, \
                photo_count, auto_generated, updated_at \
         FROM trips ORDER BY start_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn photos_in_trip(pool: &SqlitePool, trip_id: i64) -> AppResult<Vec<i64>> {
    sqlx::query_scalar("SELECT photo_id FROM trip_photos WHERE trip_id = ?1 ORDER BY photo_id")
        .bind(trip_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    /// Seed N photos at (lat, lng) spread across a time window starting at
    /// `base_ts_hours` (hours relative to 2026-10-01T00:00:00Z). Sequential
    /// ids, one photo every `hour_step` hours.
    async fn seed_cluster(
        pool: &SqlitePool,
        next_id: i64,
        n: i64,
        lat: f64,
        lng: f64,
        base_ts_hours: i64,
        hour_step: i64,
    ) -> i64 {
        let base = chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z").unwrap();
        for i in 0..n {
            let id = next_id + i;
            let ts = base + chrono::Duration::hours(base_ts_hours + i * hour_step);
            let ts_str = ts.to_rfc3339();
            let q = format!(
                "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw, gps_lat, gps_lng, captured_at) \
                 VALUES ({id}, '{:0>64}', 'p.jpg', 1, 1, '2026-10-01T00:00:00Z', 0, {lat}, {lng}, '{ts_str}')",
                id
            );
            pool.execute(q.as_str()).await.expect("seed photo");
        }
        next_id + n
    }

    #[tokio::test]
    async fn clusters_three_separated_trips() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        // Trip A: Bengaluru (12.97, 77.59), 4 photos, hours 0..3
        let next = seed_cluster(&pool, 1, 4, 12.97, 77.59, 0, 1).await;
        // 72h gap triggers time split. Trip B: Tokyo (35.68, 139.69), 4 photos.
        let next = seed_cluster(&pool, next, 4, 35.68, 139.69, 4 + 72, 1).await;
        // Same time, but distance split. Trip C: Reykjavik (64.13, -21.94), 4 photos.
        let _ = seed_cluster(&pool, next, 4, 64.13, -21.94, 4 + 72 + 5, 1).await;

        let r = recompute_trips(&pool).await.expect("recompute");
        assert_eq!(r.trip_count, 3);
        assert_eq!(r.photo_count, 12);

        let trips = list_trips(&pool).await.unwrap();
        assert_eq!(trips.len(), 3);

        // Newest first — Reykjavik should be the most recent trip.
        assert!((trips[0].center_lat - 64.13).abs() < 0.01);
        assert!((trips[1].center_lat - 35.68).abs() < 0.01);
        assert!((trips[2].center_lat - 12.97).abs() < 0.01);

        for t in &trips {
            assert_eq!(t.photo_count, 4);
            assert!(t.radius_km < 1.0, "single-location cluster has ~0 radius");
        }
    }

    #[tokio::test]
    async fn skips_photos_without_gps_or_time() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 1, 1, '2026-10-01T00:00:00Z', 0)",
        )
        .await
        .expect("seed");
        let r = recompute_trips(&pool).await.expect("recompute");
        assert_eq!(r.trip_count, 0);
        assert_eq!(r.photo_count, 0);
    }

    #[tokio::test]
    async fn idempotent_across_reruns() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed_cluster(&pool, 1, 3, 12.97, 77.59, 0, 1).await;
        let r1 = recompute_trips(&pool).await.unwrap();
        let r2 = recompute_trips(&pool).await.unwrap();
        assert_eq!(r1.trip_count, r2.trip_count);
        assert_eq!(r1.photo_count, r2.photo_count);
    }

    #[test]
    fn haversine_bengaluru_tokyo() {
        // Great-circle distance ≈ 6660 km per OpenStreetMap / geod.
        let d = haversine_km(12.97, 77.59, 35.68, 139.69);
        assert!((d - 6660.0).abs() < 50.0, "got {d}");
    }

    #[test]
    fn haversine_zero_distance() {
        assert!(haversine_km(12.97, 77.59, 12.97, 77.59) < 1e-6);
    }
}
