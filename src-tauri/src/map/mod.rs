//! Phase 4 §5 — GPS trip clustering (partial scope).
//!
//! The full map screen (leaflet/maplibre + OSM tile cache + offline
//! reverse-geocoder) is a follow-up. This module ships the backend
//! compute so the existing Catalog **Places facet** can graduate from
//! raw GPS coordinates to named trip pins, and so a future map view can
//! read pre-clustered data without recomputing every open.
//!
//! Algorithm: temporal+spatial single-pass cluster.
//! - Sort photos by `captured_at` ascending.
//! - Start a new trip when either:
//!   (a) `captured_at` gap from the previous photo > 48h, OR
//!   (b) Haversine distance from the running centroid > 30 km.
//! - At trip close, compute `start_at`, `end_at`, centroid (arithmetic
//!   mean of lat/lng), and `radius_km` = max distance of any photo
//!   from the centroid.

pub mod trips;

pub use trips::{list_trips, photos_in_trip, recompute_trips, RecomputeReceipt, TripRow};
