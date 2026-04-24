//! Phase 4 §1 — prompt-edit history persistence.
//!
//! Every successful `prompt_edit` submission writes a `prompt_edits`
//! row with the rendered image + prompt + seed so the user can scroll
//! past generations inside the Prompt tab and accept or reject each.
//!
//! Accept = mark `state='accepted'`. Reject = mark `state='rejected'`.
//! Pending rows render alongside the live Generate result; accepting
//! one implicitly rejects any other pending rows for the same photo.

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PromptEditRow {
    pub id: i64,
    pub photo_id: i64,
    pub prompt: String,
    pub strength: i64,
    pub constraints_json: String,
    pub mask_b64: Option<String>,
    pub rendered_b64: String,
    pub model_id: String,
    pub seed: i64,
    pub latency_ms: i64,
    pub state: String,
    pub created_at: String,
}

/// Append a new generation row. Returns the new id.
#[allow(clippy::too_many_arguments)]
pub async fn save(
    pool: &SqlitePool,
    photo_id: i64,
    prompt: &str,
    strength: u8,
    constraints: &[String],
    mask_b64: Option<&str>,
    rendered_b64: &str,
    model_id: &str,
    seed: i64,
    latency_ms: u64,
) -> AppResult<i64> {
    let constraints_json = serde_json::to_string(constraints)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO prompt_edits (photo_id, prompt, strength, constraints_json, mask_b64, \
          rendered_b64, model_id, seed, latency_ms, state, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10) RETURNING id",
    )
    .bind(photo_id)
    .bind(prompt)
    .bind(strength as i64)
    .bind(&constraints_json)
    .bind(mask_b64)
    .bind(rendered_b64)
    .bind(model_id)
    .bind(seed)
    .bind(latency_ms as i64)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn list_for_photo(pool: &SqlitePool, photo_id: i64) -> AppResult<Vec<PromptEditRow>> {
    sqlx::query_as::<_, PromptEditRow>(
        "SELECT id, photo_id, prompt, strength, constraints_json, mask_b64, rendered_b64, \
                model_id, seed, latency_ms, state, created_at \
         FROM prompt_edits WHERE photo_id = ?1 ORDER BY created_at DESC",
    )
    .bind(photo_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn accept(pool: &SqlitePool, edit_id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let photo_id: Option<i64> =
        sqlx::query_scalar("SELECT photo_id FROM prompt_edits WHERE id = ?1")
            .bind(edit_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(photo_id) = photo_id else {
        return Err(AppError::NotFound(format!("prompt edit {edit_id}")));
    };
    // Reject every other pending row on the same photo.
    sqlx::query(
        "UPDATE prompt_edits SET state = 'rejected' \
         WHERE photo_id = ?1 AND state = 'pending' AND id != ?2",
    )
    .bind(photo_id)
    .bind(edit_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE prompt_edits SET state = 'accepted' WHERE id = ?1")
        .bind(edit_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn reject(pool: &SqlitePool, edit_id: i64) -> AppResult<()> {
    let res = sqlx::query("UPDATE prompt_edits SET state = 'rejected' WHERE id = ?1")
        .bind(edit_id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("prompt edit {edit_id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use sqlx::Executor;

    async fn seed_photo(pool: &SqlitePool, id: i64) {
        let sha = format!("{:064}", id);
        pool.execute(
            format!(
                "INSERT INTO photos (id, sha256, filename, width, height, imported_at, is_raw) \
                 VALUES ({id}, '{sha}', 'p.jpg', 100, 100, '2026-10-01T00:00:00Z', 0)"
            )
            .as_str(),
        )
        .await
        .expect("seed");
    }

    #[tokio::test]
    async fn save_then_list_returns_row() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        seed_photo(&pool, 1).await;
        let id = save(
            &pool,
            1,
            "lift shadows",
            65,
            &["keep faces sharp".to_string()],
            None,
            "<b64>",
            "flux-dev",
            42,
            900,
        )
        .await
        .unwrap();
        assert!(id > 0);
        let rows = list_for_photo(&pool, 1).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, "pending");
        assert_eq!(rows[0].model_id, "flux-dev");
    }

    #[tokio::test]
    async fn accept_rejects_other_pending_rows() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        seed_photo(&pool, 1).await;
        let a = save(&pool, 1, "v1", 60, &[], None, "<a>", "flux-dev", 1, 100)
            .await
            .unwrap();
        let b = save(&pool, 1, "v2", 60, &[], None, "<b>", "flux-dev", 2, 100)
            .await
            .unwrap();
        accept(&pool, b).await.unwrap();
        let rows = list_for_photo(&pool, 1).await.unwrap();
        let state_a = rows.iter().find(|r| r.id == a).unwrap().state.clone();
        let state_b = rows.iter().find(|r| r.id == b).unwrap().state.clone();
        assert_eq!(state_a, "rejected");
        assert_eq!(state_b, "accepted");
    }

    #[tokio::test]
    async fn reject_marks_row_without_touching_siblings() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        seed_photo(&pool, 1).await;
        let a = save(&pool, 1, "v1", 60, &[], None, "<a>", "flux-dev", 1, 100)
            .await
            .unwrap();
        let b = save(&pool, 1, "v2", 60, &[], None, "<b>", "flux-dev", 2, 100)
            .await
            .unwrap();
        reject(&pool, a).await.unwrap();
        let rows = list_for_photo(&pool, 1).await.unwrap();
        let state_a = rows.iter().find(|r| r.id == a).unwrap().state.clone();
        let state_b = rows.iter().find(|r| r.id == b).unwrap().state.clone();
        assert_eq!(state_a, "rejected");
        assert_eq!(state_b, "pending");
    }
}
