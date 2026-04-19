//! System-managed rediscovery smart albums seeded in addition to the
//! user-facing seed albums from catalog/seed.rs. Phase 1 PRD §13.
//!
//! Three albums are created on every startup (idempotent upsert by name):
//!
//! - **"On this day"** — photos captured on today's MM-DD in any prior year.
//!   `kind = 'rediscovery_today'` marks it for daily recomputation by the
//!   background re-evaluator.
//!
//! - **"First time on new camera"** — photos captured on a specific camera
//!   model (Sony ILCE-7M4 as the Phase 1 persona default). Phase 2 will
//!   personalize this per-user by computing the first-seen timestamp for each
//!   camera model in the catalog.
//!
//! - **"Unflagged favorites"** — aesthetic score ≥ 8.0 AND not starred. The
//!   "not in any user album" half of the PRD rule is deferred to Phase 2 when
//!   `photo_album_membership` is added (ADR 0001 open issue).

use crate::catalog::rules::validated_mmdd;
use crate::{AppError, AppResult};
use chrono::Utc;
use sqlx::SqlitePool;

/// Row shape for an upsert into `smart_albums`.
struct RediscoverySeed {
    name: &'static str,
    description: &'static str,
    rule_json: String,
    /// NULL for static albums; `'rediscovery_today'` for the MM-DD album.
    kind: Option<&'static str>,
}

/// Build the three rediscovery seed rows.
///
/// `today_mmdd` must be a validated "MM-DD" string (e.g. "04-20") produced by
/// the caller from the current UTC date. Separated from I/O so tests can
/// inject a fixed date.
fn build_seeds(today_mmdd: &str) -> Vec<RediscoverySeed> {
    vec![
        RediscoverySeed {
            name: "On this day",
            description: "Photos taken on this calendar day in previous years",
            rule_json: format!(
                r#"{{"type":"captured_at","op":"on_mmdd","value":"{today_mmdd}"}}"#
            ),
            kind: Some("rediscovery_today"),
        },
        RediscoverySeed {
            // Phase 2 TODO(cc): personalize by scanning the catalog for each
            // camera model's first-seen date and replacing the between bounds.
            name: "First time on new camera",
            description: "Sony A7 IV — first shots on this camera body",
            rule_json: r#"{"type":"camera","field":"model","value":"ILCE-7M4"}"#.to_owned(),
            kind: None,
        },
        RediscoverySeed {
            name: "Unflagged favorites",
            description: "High-quality shots you haven't interacted with yet",
            rule_json: r#"{"type":"all","rules":[{"type":"quality","field":"aesthetic","op":"gte","value":8.0},{"type":"not","rule":{"type":"starred","value":true}}]}"#.to_owned(),
            kind: None,
        },
        RediscoverySeed {
            // Resolved by migrations 20260423000001 (photos.last_viewed_at +
            // photo_views_sync triggers) and 20260424000000 (FTS5 fix).
            name: "Unseen in 2 years",
            description: "Photos not viewed in over two years with strong aesthetics",
            rule_json: r#"{"type":"all","rules":[{"type":"last_viewed","op":"older_than_days","value":730},{"type":"quality","field":"aesthetic","op":"gte","value":6.5}]}"#.to_owned(),
            kind: Some("rediscovery_unseen"),
        },
    ]
}

/// Seed the three rediscovery smart albums.
///
/// Idempotent: uses `INSERT OR IGNORE` keyed by `name`. Safe to call on every
/// app startup. Albums that already exist are left untouched (the re-evaluator
/// handles keeping `rule_json` current for `rediscovery_today` albums).
pub async fn seed_rediscovery_albums(pool: &SqlitePool) -> AppResult<()> {
    let today_mmdd = Utc::now().format("%m-%d").to_string();

    // Validate our own computed value — belt-and-suspenders.
    validated_mmdd(&today_mmdd)
        .ok_or_else(|| AppError::Internal(format!("invalid today mmdd: {today_mmdd}")))?;

    let seeds = build_seeds(&today_mmdd);
    let now = Utc::now().to_rfc3339();

    for seed in &seeds {
        sqlx::query(
            "INSERT OR IGNORE INTO smart_albums
             (name, description, rule_json, kind, cover_photo_ids, photo_count,
              tag, is_system, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '[]', 0, 'rediscovery', 1, ?5, ?5)",
        )
        .bind(seed.name)
        .bind(seed.description)
        .bind(&seed.rule_json)
        .bind(seed.kind)
        .bind(&now)
        .execute(pool)
        .await?;
    }

    tracing::info!(count = seeds.len(), "seeded rediscovery smart albums");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        db::{open_pool, PoolOptions},
        rules::{rule_to_sql, AlbumRule},
    };
    use tempfile::TempDir;

    async fn make_pool() -> (TempDir, SqlitePool) {
        let tmp = TempDir::new().unwrap();
        let pool = open_pool(PoolOptions::new(tmp.path().join("c.db")))
            .await
            .unwrap();
        (tmp, pool)
    }

    #[tokio::test]
    async fn seeds_four_rediscovery_albums() {
        let (_tmp, pool) = make_pool().await;
        seed_rediscovery_albums(&pool).await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM smart_albums WHERE tag = 'rediscovery'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 4, "expected 4 rediscovery albums, got {count}");

        // Verify all expected names exist.
        for name in [
            "On this day",
            "First time on new camera",
            "Unflagged favorites",
            "Unseen in 2 years",
        ] {
            let found: Option<i64> =
                sqlx::query_scalar("SELECT id FROM smart_albums WHERE name = ?1")
                    .bind(name)
                    .fetch_optional(&pool)
                    .await
                    .unwrap();
            assert!(found.is_some(), "missing album: {name}");
        }
    }

    #[tokio::test]
    async fn rediscovery_seed_is_idempotent() {
        let (_tmp, pool) = make_pool().await;

        seed_rediscovery_albums(&pool).await.unwrap();
        seed_rediscovery_albums(&pool).await.unwrap();
        seed_rediscovery_albums(&pool).await.unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM smart_albums WHERE tag = 'rediscovery'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            count, 4,
            "idempotency failed: got {count} albums after 3 calls"
        );
    }

    #[test]
    fn on_this_day_rule_parses_to_strftime_sql() {
        // Build the seeds with a fixed MM-DD and assert the generated SQL fragment.
        let seeds = build_seeds("04-20");
        let on_this_day = seeds
            .iter()
            .find(|s| s.name == "On this day")
            .expect("On this day seed missing");

        let rule: AlbumRule = crate::catalog::rules::parse_rule(&on_this_day.rule_json)
            .expect("rule_json must parse");
        let sql = rule_to_sql(&rule).expect("on_mmdd rule must produce SQL");

        assert!(
            sql.contains("strftime('%m-%d', captured_at)"),
            "expected strftime in SQL, got: {sql}"
        );
        assert!(
            sql.contains("'04-20'"),
            "expected MM-DD value in SQL, got: {sql}"
        );
    }
}
