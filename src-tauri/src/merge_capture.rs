//! Merge jobs and watched-folder tether MVP.

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MergeJobRow {
    pub id: i64,
    pub kind: String,
    pub photo_ids_json: String,
    pub options_json: String,
    pub state: String,
    pub output_photo_id: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeJobCreateRequest {
    pub kind: String,
    pub photo_ids: Vec<i64>,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TetherSourceRow {
    pub id: i64,
    pub name: String,
    pub folder_path: String,
    pub vendor: Option<String>,
    pub enabled: bool,
    pub options_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TetherSourceCreateRequest {
    pub name: String,
    pub folder_path: String,
    pub vendor: Option<String>,
    pub options: serde_json::Value,
}

pub async fn create_merge_job(
    pool: &SqlitePool,
    req: MergeJobCreateRequest,
) -> AppResult<MergeJobRow> {
    validate_kind(&req.kind)?;
    if req.photo_ids.len() < 2 {
        return Err(AppError::InvalidInput(
            "merge jobs require at least two photos".into(),
        ));
    }
    let photo_ids_json = serde_json::to_string(&req.photo_ids)?;
    let options_json = serde_json::to_string(&req.options)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO merge_jobs (kind, photo_ids_json, options_json, state, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'queued', ?4, ?4) RETURNING id",
    )
    .bind(req.kind)
    .bind(photo_ids_json)
    .bind(options_json)
    .bind(now)
    .fetch_one(pool)
    .await?;
    get_merge_job(pool, id).await
}

pub async fn list_merge_jobs(pool: &SqlitePool) -> AppResult<Vec<MergeJobRow>> {
    sqlx::query_as::<_, MergeJobRow>(
        "SELECT id, kind, photo_ids_json, options_json, state, output_photo_id, error, created_at, updated_at \
         FROM merge_jobs ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

pub async fn add_tether_source(
    pool: &SqlitePool,
    req: TetherSourceCreateRequest,
) -> AppResult<TetherSourceRow> {
    let name = req.name.trim();
    let folder_path = req.folder_path.trim();
    if name.is_empty() || folder_path.is_empty() {
        return Err(AppError::InvalidInput(
            "tether source name and folder path are required".into(),
        ));
    }
    let options_json = serde_json::to_string(&req.options)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tether_sources (name, folder_path, vendor, enabled, options_json, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5) \
         ON CONFLICT(folder_path) DO UPDATE SET \
           name = excluded.name, vendor = excluded.vendor, enabled = 1, \
           options_json = excluded.options_json, updated_at = excluded.updated_at \
         RETURNING id",
    )
    .bind(name)
    .bind(folder_path)
    .bind(req.vendor)
    .bind(options_json)
    .bind(now)
    .fetch_one(pool)
    .await?;
    get_tether_source(pool, id).await
}

pub async fn list_tether_sources(pool: &SqlitePool) -> AppResult<Vec<TetherSourceRow>> {
    sqlx::query_as::<_, TetherSourceRow>(
        "SELECT id, name, folder_path, vendor, enabled, options_json, created_at, updated_at \
         FROM tether_sources ORDER BY enabled DESC, name ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn get_merge_job(pool: &SqlitePool, id: i64) -> AppResult<MergeJobRow> {
    sqlx::query_as::<_, MergeJobRow>(
        "SELECT id, kind, photo_ids_json, options_json, state, output_photo_id, error, created_at, updated_at \
         FROM merge_jobs WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("merge job {id}")))
}

async fn get_tether_source(pool: &SqlitePool, id: i64) -> AppResult<TetherSourceRow> {
    sqlx::query_as::<_, TetherSourceRow>(
        "SELECT id, name, folder_path, vendor, enabled, options_json, created_at, updated_at \
         FROM tether_sources WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("tether source {id}")))
}

fn validate_kind(kind: &str) -> AppResult<()> {
    match kind {
        "hdr" | "panorama" => Ok(()),
        _ => Err(AppError::InvalidInput(
            "kind must be hdr or panorama".into(),
        )),
    }
}
