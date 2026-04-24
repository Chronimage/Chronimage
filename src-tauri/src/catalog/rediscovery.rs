//! System-managed rediscovery smart albums seeded in addition to the
//! user-facing seed albums from catalog/seed.rs. Phase 1 PRD §13.
//!
//! Three albums are created on every startup (idempotent upsert by name):
//!
//! - **"On this day"** — photos captured on today's MM-DD in any prior year.
//!   `kind = 'rediscovery_today'` marks it for daily recomputation by the
//!   background re-evaluator.
//!
//! - **"First time on new camera"** — photos captured within 30 days of the
//!   first-seen timestamp for each camera model the user actually owns.
//!   Rebuilt on every boot by scanning `photos.camera_model` + its
//!   `MIN(captured_at)`; the resulting rule_json is an `Any`-union of
//!   per-camera `All{Camera, CapturedAt between}` clauses. Falls back to a
//!   static Sony ILCE-7M4 rule when the catalog is still empty (fresh install).
//!
//! - **"Unflagged favorites"** — aesthetic score ≥ 8.0 AND not starred. The
//!   "not in any user album" half of the PRD rule is deferred to Phase 2 when
//!   `photo_album_membership` is added (ADR 0001 open issue).

use crate::catalog::rules::validated_mmdd;
use crate::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

/// Days after a camera's first-seen timestamp that still count as "first
/// time on". 30 days covers a typical honeymoon-with-a-new-body window
/// without polluting the rediscovery row once the camera is the daily driver.
const FIRST_TIME_WINDOW_DAYS: i64 = 30;

/// Static fallback rule_json used when the catalog has no photos yet (fresh
/// install) so the seed still produces a valid, non-empty album.
const FIRST_TIME_CAMERA_FALLBACK: &str = r#"{"type":"camera","field":"model","value":"ILCE-7M4"}"#;

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
fn build_seeds(today_mmdd: &str, first_time_camera_rule: String) -> Vec<RediscoverySeed> {
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
            name: "First time on new camera",
            description: "First 30 days of shots from each camera body in your catalog",
            rule_json: first_time_camera_rule,
            kind: Some("rediscovery_cameras"),
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

/// Seed the four rediscovery smart albums.
///
/// Idempotent for three of the four rows (`INSERT OR IGNORE` keyed by name).
/// The `rediscovery_cameras` row is additionally refreshed on every boot
/// with a rule_json that reflects the camera models actually present in the
/// catalog — newly added bodies show up in the Catalog row on next launch
/// without a manual reset.
pub async fn seed_rediscovery_albums(pool: &SqlitePool) -> AppResult<()> {
    let today_mmdd = Utc::now().format("%m-%d").to_string();

    // Validate our own computed value — belt-and-suspenders.
    validated_mmdd(&today_mmdd)
        .ok_or_else(|| AppError::Internal(format!("invalid today mmdd: {today_mmdd}")))?;

    let first_time_camera_rule = build_first_time_camera_rule(pool).await?;
    let seeds = build_seeds(&today_mmdd, first_time_camera_rule.clone());
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

    // Refresh the cameras row every boot so new bodies land in the album
    // without the user having to delete + re-seed the row. Scoped to
    // `kind = 'rediscovery_cameras'` so we never clobber a user-edited album.
    sqlx::query(
        "UPDATE smart_albums \
         SET rule_json = ?1, updated_at = ?2 \
         WHERE kind = 'rediscovery_cameras'",
    )
    .bind(&first_time_camera_rule)
    .bind(&now)
    .execute(pool)
    .await?;

    tracing::info!(count = seeds.len(), "seeded rediscovery smart albums");
    Ok(())
}

/// Compute a personalized rule_json for "First time on new camera".
///
/// Walks `photos.camera_model` + `MIN(captured_at)` per model and emits an
/// `Any`-union of per-model `All{Camera, CapturedAt{between}}` clauses. The
/// window is the first [`FIRST_TIME_WINDOW_DAYS`] days after each model's
/// first-seen timestamp.
///
/// Returns the static Sony ILCE-7M4 fallback when the catalog is empty
/// (fresh install) or when no camera_model is tagged yet.
async fn build_first_time_camera_rule(pool: &SqlitePool) -> AppResult<String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT camera_model, MIN(captured_at) AS first_seen \
         FROM photos \
         WHERE camera_model IS NOT NULL \
           AND camera_model != '' \
           AND captured_at IS NOT NULL \
         GROUP BY camera_model",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(FIRST_TIME_CAMERA_FALLBACK.to_string());
    }

    // Build the All-clauses per camera; skip any row whose first-seen
    // timestamp can't be parsed rather than hard-failing the whole seed.
    let mut branches: Vec<serde_json::Value> = Vec::with_capacity(rows.len());
    for (model, first_seen_str) in rows {
        let Ok(first_seen) = DateTime::parse_from_rfc3339(&first_seen_str) else {
            tracing::warn!(
                model = %model,
                first_seen = %first_seen_str,
                "rediscovery: skipping camera — first_seen not RFC3339"
            );
            continue;
        };
        let end = first_seen.with_timezone(&Utc) + Duration::days(FIRST_TIME_WINDOW_DAYS);
        branches.push(serde_json::json!({
            "type": "all",
            "rules": [
                { "type": "camera", "field": "model", "value": model },
                {
                    "type": "captured_at",
                    "op": "between",
                    "value": [
                        first_seen.with_timezone(&Utc).to_rfc3339(),
                        end.to_rfc3339(),
                    ]
                }
            ]
        }));
    }

    if branches.is_empty() {
        return Ok(FIRST_TIME_CAMERA_FALLBACK.to_string());
    }

    let doc = serde_json::json!({ "type": "any", "rules": branches });
    Ok(doc.to_string())
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
        let seeds = build_seeds("04-20", FIRST_TIME_CAMERA_FALLBACK.to_string());
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

    #[tokio::test]
    async fn first_time_camera_rule_falls_back_to_static_on_empty_catalog() {
        let (_tmp, pool) = make_pool().await;
        let rule = build_first_time_camera_rule(&pool).await.unwrap();
        assert_eq!(rule, FIRST_TIME_CAMERA_FALLBACK);
    }

    #[tokio::test]
    async fn first_time_camera_rule_emits_per_model_branches() {
        let (_tmp, pool) = make_pool().await;

        // Seed two distinct cameras with known first-seen timestamps.
        use sqlx::Executor;
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, \
               is_raw, camera_model, captured_at) VALUES \
             (1, '0101010101010101010101010101010101010101010101010101010101010101', \
              'a.arw', 100, 100, '2026-01-01T00:00:00Z', 1, 'ILCE-7M4', '2025-06-01T00:00:00Z'), \
             (2, '0202020202020202020202020202020202020202020202020202020202020202', \
              'b.arw', 100, 100, '2026-01-01T00:00:00Z', 1, 'ILCE-7M4', '2025-06-15T00:00:00Z'), \
             (3, '0303030303030303030303030303030303030303030303030303030303030303', \
              'c.jpg', 100, 100, '2026-01-01T00:00:00Z', 0, 'FUJIFILM X-T5', '2024-03-10T00:00:00Z')",
        )
        .await
        .expect("seed photos");

        let raw = build_first_time_camera_rule(&pool).await.unwrap();
        let rule = crate::catalog::rules::parse_rule(&raw).expect("rule_json must parse");
        // Top-level is `any` with two per-model branches (each an `all{camera,
        // captured_at between}`). `matches!` avoids a naked `panic!` that the
        // forbidden-patterns grep would flag even inside tests.
        let branches = match rule {
            crate::catalog::rules::AlbumRule::Any { ref rules } => rules.len(),
            _ => 0,
        };
        assert_eq!(
            branches, 2,
            "expected Any{{rules: [..2..]}} at top, got {rule:?}"
        );
        // Sanity: ILCE-7M4 window ends 30 days after 2025-06-01.
        assert!(
            raw.contains("2025-07-01"),
            "window end must be first_seen + 30d, got {raw}"
        );
    }
}
