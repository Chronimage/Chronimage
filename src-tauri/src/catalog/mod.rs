//! Catalog domain: SQLite schema, queries, and migrations.
//!
//! Phase 0 shipped the connection model + initial migration SQL. Phase 1
//! wires an sqlx pool + runs migrations at app startup, and exposes a thin
//! `open_pool` API used by Tauri commands and the CLI.

pub mod db;
pub mod models;
pub mod rediscovery;
pub mod rules;
pub mod seed;

pub use db::{open_pool, PoolOptions};
pub use rediscovery::seed_rediscovery_albums;
pub use seed::seed_default_smart_albums;

/// Get or create a row in the `models` table, returning the row id.
///
/// Uses INSERT OR IGNORE so concurrent callers are safe; always ends with a
/// SELECT to handle the case where the row pre-existed.
pub async fn ensure_model_row(pool: &sqlx::SqlitePool, name: &str, kind: &str) -> Option<i64> {
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO models (name, kind, version, sha256) \
         VALUES (?1, ?2, 'unknown', 'unknown')",
    )
    .bind(name)
    .bind(kind)
    .execute(pool)
    .await;

    sqlx::query_scalar::<_, i64>("SELECT id FROM models WHERE name = ?1")
        .bind(name)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}
