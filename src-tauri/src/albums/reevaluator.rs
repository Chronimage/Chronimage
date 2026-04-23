//! Background smart-album re-evaluator. Phase 1 PRD §7.
//!
//! Runs on a fixed cadence (default 10 min) and keeps `smart_albums.photo_count`
//! and `smart_albums.cover_photo_ids` fresh without blocking the UI.
//!
//! For albums with `kind = 'rediscovery_today'` the rule_json is recomputed
//! to embed today's MM-DD before matching, so "On this day" always reflects
//! the current calendar day even if the app has been running since midnight.
//!
//! Public seams:
//! - [`reevaluate_all`]  — single pass; called by the loop and by tests.
//! - [`spawn_reevaluator`] — starts the 10-min background loop; call from `main.rs`.

use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::catalog::rules;
use crate::AppResult;

// ── Internal row type ────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct AlbumRow {
    id: i64,
    rule_json: String,
    kind: Option<String>,
}

// ── Core logic ───────────────────────────────────────────────────────────────

/// Re-evaluate every smart album once: refresh `photo_count` and
/// `cover_photo_ids`. For `kind = 'rediscovery_today'` albums the MM-DD in
/// `rule_json` is recomputed to today's date before evaluation and the updated
/// JSON is persisted back to `smart_albums.rule_json`.
///
/// Returns the number of albums whose `photo_count` or `cover_photo_ids` was
/// written (i.e. every album that was successfully processed, regardless of
/// whether the values changed).
pub async fn reevaluate_all(pool: &SqlitePool) -> AppResult<usize> {
    let albums = sqlx::query_as::<_, AlbumRow>("SELECT id, rule_json, kind FROM smart_albums")
        .fetch_all(pool)
        .await?;

    let now = Utc::now().to_rfc3339();
    let today_mmdd = Utc::now().format("%m-%d").to_string();
    let mut updated: usize = 0;

    for album in &albums {
        // For rediscovery_today albums, recompute the MM-DD in rule_json.
        let effective_rule_json: String = if album.kind.as_deref() == Some("rediscovery_today") {
            let new_json =
                format!(r#"{{"type":"captured_at","op":"on_mmdd","value":"{today_mmdd}"}}"#);
            // Persist the refreshed rule_json so the stored value is always current.
            if let Err(e) =
                sqlx::query("UPDATE smart_albums SET rule_json = ?1, updated_at = ?2 WHERE id = ?3")
                    .bind(&new_json)
                    .bind(&now)
                    .bind(album.id)
                    .execute(pool)
                    .await
            {
                tracing::warn!(
                    error = %e,
                    album_id = album.id,
                    "reevaluator: failed to update rediscovery_today rule_json"
                );
                continue;
            }
            new_json
        } else {
            album.rule_json.clone()
        };

        let count = rules::count_matching(pool, &effective_rule_json).await;
        let cover_ids = rules::matching_photo_ids(pool, &effective_rule_json, 20).await;
        let cover_json = serde_json::to_string(&cover_ids).unwrap_or_else(|_| "[]".to_string());

        match sqlx::query(
            "UPDATE smart_albums \
             SET photo_count = ?1, cover_photo_ids = ?2, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(count)
        .bind(&cover_json)
        .bind(&now)
        .bind(album.id)
        .execute(pool)
        .await
        {
            Ok(_) => updated += 1,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    album_id = album.id,
                    "reevaluator: failed to update album counts"
                );
            }
        }
    }

    tracing::debug!(albums = updated, "reevaluator: pass complete");
    Ok(updated)
}

/// Spawn the background re-evaluator loop.
///
/// Ticks immediately on spawn, then every `interval`. Errors from
/// [`reevaluate_all`] are logged via `tracing::warn!` and never cause a panic.
/// The returned `JoinHandle` can be aborted on shutdown, but it is safe to
/// drop — the task runs until the process exits.
pub fn spawn_reevaluator(pool: SqlitePool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(600));
        // MissedTickBehavior::Skip: if a pass takes >10 min, don't queue up catches.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // First call to tick() fires immediately (no initial delay).
            interval.tick().await;

            if let Err(e) = reevaluate_all(&pool).await {
                tracing::warn!(error = %e, "reevaluator: pass failed");
            }
        }
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        db::{open_pool, PoolOptions},
        seed::seed_default_smart_albums,
    };
    use tempfile::TempDir;

    async fn make_pool() -> (TempDir, SqlitePool) {
        let tmp = TempDir::new().unwrap();
        let pool = open_pool(PoolOptions::new(tmp.path().join("c.db")))
            .await
            .unwrap();
        (tmp, pool)
    }

    // ── Existing tests (kept from scaffold) ──────────────────────────────────

    /// Inserting a photo that matches the "Unflagged favorites" rule should
    /// result in that photo appearing in `cover_photo_ids` after a pass.
    #[tokio::test]
    async fn tick_once_updates_cover_photo_ids_json() {
        let (_tmp, pool) = make_pool().await;
        seed_default_smart_albums(&pool).await.unwrap();

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, \
             aesthetic_score, is_starred) \
             VALUES (?1, 'fav.jpg', 100, 100, ?2, 0, 9.0, 0)",
        )
        .bind("a".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let photo_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE sha256 = ?1")
            .bind("a".repeat(64))
            .fetch_one(&pool)
            .await
            .unwrap();

        reevaluate_all(&pool).await.unwrap();

        let cover_json: String = sqlx::query_scalar(
            "SELECT cover_photo_ids FROM smart_albums WHERE name = 'Unflagged favorites'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let ids: Vec<i64> = serde_json::from_str(&cover_json).unwrap();
        assert!(
            ids.contains(&photo_id),
            "photo_id {photo_id} not in cover ids: {cover_json}"
        );
    }

    /// "Unflagged favorites" must count only photos with aesthetic >= 8.0 AND
    /// is_starred = 0.
    #[tokio::test]
    async fn unflagged_favorites_counts_only_high_aesthetic_not_starred() {
        let (_tmp, pool) = make_pool().await;
        seed_default_smart_albums(&pool).await.unwrap();

        let now = Utc::now().to_rfc3339();

        // Match: high aesthetic, not starred.
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, \
             aesthetic_score, is_starred) VALUES (?1, 'match.jpg', 0, 0, ?2, 0, 9.0, 0)",
        )
        .bind("b".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // No match: high aesthetic but starred.
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, \
             aesthetic_score, is_starred) VALUES (?1, 'starred.jpg', 0, 0, ?2, 0, 9.0, 1)",
        )
        .bind("c".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // No match: low aesthetic, not starred.
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, \
             aesthetic_score, is_starred) VALUES (?1, 'low.jpg', 0, 0, ?2, 0, 5.0, 0)",
        )
        .bind("d".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        reevaluate_all(&pool).await.unwrap();

        let (photo_count,): (i64,) = sqlx::query_as(
            "SELECT photo_count FROM smart_albums WHERE name = 'Unflagged favorites'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            photo_count, 1,
            "expected 1 unflagged favorite, got {photo_count}"
        );
    }

    // ── Required new tests (PRD §7 + §13) ───────────────────────────────────

    /// After seeding + inserting a matching photo, `reevaluate_all` must write
    /// a non-zero `photo_count` and a valid `cover_photo_ids` JSON array.
    ///
    /// Inserts a tag so the `Tag` rule variant is exercised end-to-end.
    /// Resolved by migration 20260424000000 — the broken `UPDATE photos_fts`
    /// triggers were replaced with delete-then-reinsert, so inserting into
    /// `tags` no longer raises "cannot UPDATE contentless fts5 table".
    // resolved by migration 20260424000000
    #[tokio::test]
    async fn reevaluate_updates_photo_count_and_cover_ids() {
        let (_tmp, pool) = make_pool().await;
        seed_default_smart_albums(&pool).await.unwrap();

        let now = Utc::now().to_rfc3339();

        // Insert a high-ISO photo so it matches the "Night & Low Light" album
        // (ISO >= 3200, EXIF rule). Also insert a tag to exercise the fixed
        // FTS5 trigger path (migration 20260424000000).
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, iso) \
             VALUES (?1, 'night.jpg', 0, 0, ?2, 0, 6400)",
        )
        .bind("e".repeat(64))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let photo_id: i64 = sqlx::query_scalar("SELECT id FROM photos WHERE sha256 = ?1")
            .bind("e".repeat(64))
            .fetch_one(&pool)
            .await
            .unwrap();

        // Tag insert exercises the fixed tags_fts_insert trigger.
        sqlx::query(
            "INSERT INTO tags (photo_id, label, kind, confidence, created_at) \
             VALUES (?1, 'nightscape', 'auto_scene', 0.9, ?2)",
        )
        .bind(photo_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap(); // Must not error after migration 20260424000000.

        let albums_updated = reevaluate_all(&pool).await.unwrap();
        assert!(albums_updated > 0, "expected at least one album updated");

        let (photo_count, cover_json): (i64, String) = sqlx::query_as(
            "SELECT photo_count, cover_photo_ids \
             FROM smart_albums WHERE name = 'Night & Low Light'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(photo_count, 1, "Night & Low Light photo_count should be 1");
        let ids: Vec<i64> = serde_json::from_str(&cover_json).unwrap();
        assert!(
            ids.contains(&photo_id),
            "photo_id {photo_id} not in Night & Low Light cover ids: {cover_json}"
        );
    }

    /// A `kind = 'rediscovery_today'` album with a stale MM-DD should have its
    /// `rule_json` rewritten to today's date after `reevaluate_all`, and a
    /// photo captured today should then appear in the album.
    #[tokio::test]
    async fn reevaluate_today_album_recomputes_mmdd() {
        let (_tmp, pool) = make_pool().await;

        let now_ts = Utc::now().to_rfc3339();
        let today_mmdd = Utc::now().format("%m-%d").to_string();

        // Manually insert a rediscovery_today album with a deliberately stale MM-DD.
        sqlx::query(
            "INSERT INTO smart_albums \
             (name, description, rule_json, kind, cover_photo_ids, photo_count, \
              tag, is_system, created_at, updated_at) \
             VALUES ('On this day', 'test', \
                     '{\"type\":\"captured_at\",\"op\":\"on_mmdd\",\"value\":\"01-01\"}', \
                     'rediscovery_today', '[]', 0, 'rediscovery', 1, ?1, ?1)",
        )
        .bind(&now_ts)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a photo captured today (same MM-DD as today's UTC date).
        let today_full = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        sqlx::query(
            "INSERT INTO photos (sha256, filename, width, height, imported_at, is_raw, captured_at) \
             VALUES (?1, 'today.jpg', 0, 0, ?2, 0, ?3)",
        )
        .bind("f".repeat(64))
        .bind(&now_ts)
        .bind(&today_full)
        .execute(&pool)
        .await
        .unwrap();

        reevaluate_all(&pool).await.unwrap();

        // rule_json must now contain today's MM-DD, not the stale "01-01".
        let rule_json: String =
            sqlx::query_scalar("SELECT rule_json FROM smart_albums WHERE name = 'On this day'")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert!(
            rule_json.contains(&today_mmdd),
            "rule_json should contain today's MM-DD ({today_mmdd}), got: {rule_json}"
        );
        assert!(
            !rule_json.contains("01-01"),
            "stale 01-01 should have been replaced, got: {rule_json}"
        );

        // The photo captured today must now be counted.
        let (photo_count,): (i64,) =
            sqlx::query_as("SELECT photo_count FROM smart_albums WHERE name = 'On this day'")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            photo_count, 1,
            "expected today's photo to match after MM-DD recompute, got {photo_count}"
        );
    }

    /// `reevaluate_all` must return the count of albums it processed.
    #[tokio::test]
    async fn reevaluate_all_returns_count() {
        let (_tmp, pool) = make_pool().await;
        seed_default_smart_albums(&pool).await.unwrap();

        let count = reevaluate_all(&pool).await.unwrap();
        // 2 static rule-based + 4 rediscovery = 6 albums seeded.
        assert_eq!(count, 6, "expected 6 albums evaluated, got {count}");
    }
}
