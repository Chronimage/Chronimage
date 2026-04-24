//! Phase-2 cull verdict engine + Cull Bin lifecycle.
//!
//! Two responsibilities split across sub-modules:
//! - [`verdict`] — `apply_verdict(photo_id, Verdict)` writes rows into the
//!   `cull_bin` table, freezes `source_copies`, fires progress events.
//! - [`bin`] — list / restore / delete / daily-sweep operations on
//!   `cull_bin`.
//!
//! Both take a `&SqlitePool` rather than Tauri `State` so they can be unit-
//! tested against a fixture DB without a Tauri runtime.

pub mod bin;
pub mod verdict;
