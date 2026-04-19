//! Catalog domain: SQLite schema, queries, and migrations.
//!
//! Phase 0 shipped the connection model + initial migration SQL. Phase 1
//! wires an sqlx pool + runs migrations at app startup, and exposes a thin
//! `open_pool` API used by Tauri commands and the CLI.

pub mod db;
pub mod models;
pub mod rules;
pub mod seed;

pub use db::{open_pool, PoolOptions};
pub use seed::seed_default_smart_albums;
