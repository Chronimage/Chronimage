//! Tracking for generated AI edit artifacts.
//!
//! The expensive generators (denoise, remove, upscale, lens blur depth) can
//! store their outputs here and compare `source_edit_hash` against the current
//! global operations + mask layer state before reuse.

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiEditRow {
    pub id: i64,
    pub photo_id: i64,
    pub feature: String,
    pub model_id: String,
    pub source_edit_hash: String,
    pub output_path: Option<String>,
    pub output_b64: Option<String>,
    pub state: String,
    pub params_json: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEditRefreshReceipt {
    pub photo_id: i64,
    pub feature: String,
    pub source_edit_hash: String,
    pub state: String,
}

pub async fn status(pool: &SqlitePool, photo_id: i64) -> AppResult<Vec<AiEditRow>> {
    mark_stale(pool, photo_id).await?;
    sqlx::query_as::<_, AiEditRow>(
        "SELECT id, photo_id, feature, model_id, source_edit_hash, output_path, output_b64, \
                state, params_json, error, created_at, updated_at \
         FROM ai_edits WHERE photo_id = ?1 ORDER BY feature ASC, updated_at DESC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn refresh(
    pool: &SqlitePool,
    photo_id: i64,
    feature: &str,
) -> AppResult<AiEditRefreshReceipt> {
    validate_feature(feature)?;
    let source_edit_hash = current_source_hash(pool, photo_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let existing_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM ai_edits WHERE photo_id = ?1 AND feature = ?2")
            .bind(photo_id)
            .bind(feature)
            .fetch_optional(pool)
            .await?;

    match existing_id {
        Some(id) => {
            sqlx::query(
                "UPDATE ai_edits SET source_edit_hash = ?1, state = 'current', \
                   error = NULL, updated_at = ?2 WHERE id = ?3",
            )
            .bind(&source_edit_hash)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO ai_edits \
                 (photo_id, feature, model_id, source_edit_hash, state, params_json, created_at, updated_at) \
                 VALUES (?1, ?2, 'local-placeholder', ?3, 'current', '{}', ?4, ?4)",
            )
            .bind(photo_id)
            .bind(feature)
            .bind(&source_edit_hash)
            .bind(&now)
            .execute(pool)
            .await?;
        }
    }

    Ok(AiEditRefreshReceipt {
        photo_id,
        feature: feature.into(),
        source_edit_hash,
        state: "current".into(),
    })
}

pub async fn mark_stale(pool: &SqlitePool, photo_id: i64) -> AppResult<u64> {
    let source_edit_hash = current_source_hash(pool, photo_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE ai_edits SET state = 'stale', updated_at = ?1 \
         WHERE photo_id = ?2 AND state = 'current' AND source_edit_hash <> ?3",
    )
    .bind(now)
    .bind(photo_id)
    .bind(source_edit_hash)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn current_source_hash(pool: &SqlitePool, photo_id: i64) -> AppResult<String> {
    let ops_json: Option<String> = sqlx::query_scalar(
        "SELECT e.operations_json FROM photos p \
         LEFT JOIN edits e ON e.id = p.current_edit_id \
         WHERE p.id = ?1",
    )
    .bind(photo_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let mask_rows: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, mask_payload, operations_json, updated_at, visible \
         FROM develop_masks WHERE photo_id = ?1 ORDER BY order_index ASC, id ASC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await?;

    let mut hasher = Sha256::new();
    hasher.update(photo_id.to_le_bytes());
    hasher.update(ops_json.unwrap_or_else(|| "{}".into()).as_bytes());
    for (id, payload, operations, updated_at, visible) in mask_rows {
        hasher.update(id.to_le_bytes());
        hasher.update(payload.as_bytes());
        hasher.update(operations.as_bytes());
        hasher.update(updated_at.as_bytes());
        hasher.update(visible.to_le_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn validate_feature(feature: &str) -> AppResult<()> {
    let trimmed = feature.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::InvalidInput(
            "ai edit feature must be 1-64 characters".into(),
        ));
    }
    if trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "ai edit feature must be lowercase kebab/snake case".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn seeded_pool() -> SqlitePool {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        pool.execute(
            "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
             VALUES (1, 'ai11111111111111111111111111111111111111111111111111111111111111', 'a.jpg', 100, 100, '2026-12-02T00:00:00Z', 0)",
        )
        .await
        .expect("seed photo");
        pool
    }

    #[tokio::test]
    async fn refresh_creates_status_row() {
        let pool = seeded_pool().await;
        let receipt = refresh(&pool, 1, "denoise").await.expect("refresh");
        assert_eq!(receipt.state, "current");

        let rows = status(&pool, 1).await.expect("status");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].feature, "denoise");
    }

    #[tokio::test]
    async fn status_marks_changed_hash_stale() {
        let pool = seeded_pool().await;
        refresh(&pool, 1, "upscale").await.expect("refresh");
        pool.execute(
            "INSERT INTO develop_masks \
             (photo_id, name, source, mode, visible, order_index, payload_storage, mask_payload, operations_json, created_at, updated_at) \
             VALUES (1, 'Subject', 'subject', 'normal', 1, 0, 'inline', '{\"kind\":\"subject\"}', '{\"exposure\":0.5}', 'now', 'now')",
        )
        .await
        .expect("mask");

        let rows = status(&pool, 1).await.expect("status");
        assert_eq!(rows[0].state, "stale");
    }
}
