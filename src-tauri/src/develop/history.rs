//! Edit history — CRUD over the `edits` table + the
//! `photos.current_edit_id` pointer.
//!
//! Every `develop_save` appends one row with a `parent_edit_id` equal to
//! the photo's previous `current_edit_id`. Undo walks to parent; redo
//! walks to the child by `saved_at` ASC. This keeps the graph simple
//! enough that a naïve SQL walk handles it — no materialised path.

use super::ops::{Operations, PastedReceipt};
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EditRow {
    pub id: i64,
    pub photo_id: i64,
    pub parent_edit_id: Option<i64>,
    pub operations_json: String,
    pub saved_at: String,
    pub is_snapshot: bool,
    pub label: Option<String>,
}

impl EditRow {
    pub fn operations(&self) -> AppResult<Operations> {
        serde_json::from_str(&self.operations_json).map_err(AppError::from)
    }
}

/// Load the photo's currently-pointed-to operations, defaulting to identity
/// when no edits exist yet.
pub async fn load_current(pool: &SqlitePool, photo_id: i64) -> AppResult<Operations> {
    let current_id: Option<i64> =
        sqlx::query_scalar("SELECT current_edit_id FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    let Some(id) = current_id else {
        return Ok(Operations::identity());
    };

    let row: Option<EditRow> = sqlx::query_as::<_, EditRow>(
        "SELECT id, photo_id, parent_edit_id, operations_json, saved_at, is_snapshot, label \
         FROM edits WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => r.operations(),
        None => Ok(Operations::identity()),
    }
}

/// Append a new edit row + update the photo's current pointer. Returns
/// the new edit id.
pub async fn save(
    pool: &SqlitePool,
    photo_id: i64,
    ops: &Operations,
    label: Option<String>,
) -> AppResult<i64> {
    save_with_snapshot_flag(pool, photo_id, ops, label, false).await
}

pub async fn save_snapshot(
    pool: &SqlitePool,
    photo_id: i64,
    ops: &Operations,
    label: Option<String>,
) -> AppResult<i64> {
    save_with_snapshot_flag(pool, photo_id, ops, label, true).await
}

async fn save_with_snapshot_flag(
    pool: &SqlitePool,
    photo_id: i64,
    ops: &Operations,
    label: Option<String>,
    is_snapshot: bool,
) -> AppResult<i64> {
    // Validate photo exists.
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM photos WHERE id = ?1")
        .bind(photo_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("photo {photo_id}")));
    }

    let parent_id: Option<i64> =
        sqlx::query_scalar("SELECT current_edit_id FROM photos WHERE id = ?1")
            .bind(photo_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    let ops_json = serde_json::to_string(ops)?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO edits (photo_id, parent_edit_id, operations_json, saved_at, is_snapshot, label) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
    )
    .bind(photo_id)
    .bind(parent_id)
    .bind(&ops_json)
    .bind(&now)
    .bind(is_snapshot)
    .bind(label)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE photos SET current_edit_id = ?1 WHERE id = ?2")
        .bind(new_id)
        .bind(photo_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(new_id)
}

/// Reset: delete all edits for a photo, clear the current pointer. The
/// photo reverts to "as imported".
pub async fn reset(pool: &SqlitePool, photo_id: i64) -> AppResult<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE photos SET current_edit_id = NULL WHERE id = ?1")
        .bind(photo_id)
        .execute(&mut *tx)
        .await?;
    let affected = sqlx::query("DELETE FROM edits WHERE photo_id = ?1")
        .bind(photo_id)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    tx.commit().await?;
    Ok(affected)
}

/// Serialise the photo's current operations — for Copy Edits.
pub async fn copy_edits(pool: &SqlitePool, photo_id: i64) -> AppResult<Operations> {
    load_current(pool, photo_id).await
}

/// Paste `ops` onto each photo id: append a new edit row chained from the
/// photo's current edit, update the pointer. Returns counts.
pub async fn paste_edits(
    pool: &SqlitePool,
    photo_ids: &[i64],
    ops: &Operations,
) -> AppResult<PastedReceipt> {
    if photo_ids.is_empty() {
        return Ok(PastedReceipt {
            pasted_photo_count: 0,
            skipped: Vec::new(),
        });
    }
    let mut pasted = 0usize;
    let mut skipped: Vec<i64> = Vec::new();
    for pid in photo_ids {
        match save(pool, *pid, ops, Some("pasted".into())).await {
            Ok(_) => pasted += 1,
            Err(AppError::NotFound(_)) => skipped.push(*pid),
            Err(e) => return Err(e),
        }
    }
    Ok(PastedReceipt {
        pasted_photo_count: pasted,
        skipped,
    })
}

/// List edits for a photo, newest first. Used by the UI's undo/redo log.
pub async fn list_for_photo(pool: &SqlitePool, photo_id: i64) -> AppResult<Vec<EditRow>> {
    sqlx::query_as::<_, EditRow>(
        "SELECT id, photo_id, parent_edit_id, operations_json, saved_at, is_snapshot, label \
         FROM edits WHERE photo_id = ?1 ORDER BY saved_at DESC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn seed(pool: &SqlitePool) {
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, '1111111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 100, 100, '2026-08-01T00:00:00Z', 0), \
                    (2, '2222222222222222222222222222222222222222222222222222222222222222', 'b.jpg', 100, 100, '2026-08-01T00:00:00Z', 0)",
        )
        .await
        .expect("seed");
    }

    #[tokio::test]
    async fn load_current_is_identity_when_no_edits() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool).await;
        let ops = load_current(&pool, 1).await.unwrap();
        assert!(ops.is_identity());
    }

    #[tokio::test]
    async fn save_updates_pointer_and_chains_parent() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool).await;

        let ops1 = Operations {
            exposure: 1.0,
            ..Operations::identity()
        };
        let id1 = save(&pool, 1, &ops1, None).await.unwrap();

        let ops2 = Operations {
            exposure: 1.0,
            saturation: 30.0,
            ..Operations::identity()
        };
        let id2 = save(&pool, 1, &ops2, None).await.unwrap();

        // Pointer advanced to id2.
        let cur: Option<i64> =
            sqlx::query_scalar("SELECT current_edit_id FROM photos WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cur, Some(id2));

        // id2's parent is id1.
        let parent: Option<i64> =
            sqlx::query_scalar("SELECT parent_edit_id FROM edits WHERE id = ?1")
                .bind(id2)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(parent, Some(id1));
    }

    #[tokio::test]
    async fn reset_clears_all_history() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool).await;
        save(&pool, 1, &Operations::identity(), None).await.unwrap();
        save(
            &pool,
            1,
            &Operations {
                exposure: 1.0,
                ..Operations::identity()
            },
            None,
        )
        .await
        .unwrap();

        let count = reset(&pool, 1).await.unwrap();
        assert_eq!(count, 2);

        let cur: Option<i64> =
            sqlx::query_scalar("SELECT current_edit_id FROM photos WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(cur.is_none());
    }

    #[tokio::test]
    async fn paste_edits_across_photos_creates_one_row_each() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed(&pool).await;

        let ops = Operations {
            saturation: 20.0,
            ..Operations::identity()
        };
        let receipt = paste_edits(&pool, &[1, 2, 999], &ops).await.unwrap();
        assert_eq!(receipt.pasted_photo_count, 2);
        assert_eq!(receipt.skipped, vec![999]);

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edits")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 2);
    }
}
