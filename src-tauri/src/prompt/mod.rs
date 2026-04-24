//! Phase 4 §1/§2 — generative edit sidecar client.
//!
//! Chronimage does not bundle or run the diffusion model itself. A
//! user-provided sidecar (Flux-dev via `diffusers`, SDXL-Inpaint via
//! `comfyui`, or any other OpenAI-style JSON service) listens on a
//! configurable URL and returns a rendered image. This module:
//!
//! 1. stores the sidecar URL in the `settings` KV table
//!    (`ai.prompt_sidecar_url`);
//! 2. exposes an `is_configured` / `ping` flow for the UI to light up
//!    the Prompt tab;
//! 3. submits `{ photo_bytes, prompt, strength, constraints, mask? }`
//!    and returns a job receipt the frontend can poll.
//!
//! The actual generative backend, first-run model download, and PID
//! supervision stay week-4+ work. This module is the interface — every
//! generative command lives behind it and fails clean with an
//! actionable error when the sidecar is absent.
//!
//! Network traffic is gated behind commands prefixed with
//! `user_initiated_` (the PRD's rule from CLAUDE.md § Security).

use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Duration;

pub const SIDECAR_URL_KEY: &str = "ai.prompt_sidecar_url";
pub const SIDECAR_MODEL_KEY: &str = "ai.prompt_sidecar_model";

/// Read the configured sidecar base URL. Missing / empty returns
/// `Ok(None)` so call sites can switch on "configured?" without a
/// special-cased error.
pub async fn get_sidecar_url(pool: &SqlitePool) -> AppResult<Option<String>> {
    let row: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(SIDECAR_URL_KEY)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }))
}

pub async fn set_sidecar_url(pool: &SqlitePool, url: Option<&str>) -> AppResult<()> {
    let val = url.unwrap_or("").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(SIDECAR_URL_KEY)
    .bind(&val)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_preferred_model(pool: &SqlitePool) -> AppResult<Option<String>> {
    let row: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(SIDECAR_MODEL_KEY)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }))
}

pub async fn set_preferred_model(pool: &SqlitePool, model: Option<&str>) -> AppResult<()> {
    let val = model.unwrap_or("").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(SIDECAR_MODEL_KEY)
    .bind(&val)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarStatus {
    pub configured: bool,
    pub url: Option<String>,
    pub reachable: bool,
    pub model: Option<String>,
    pub error: Option<String>,
}

/// Probe the sidecar's `/health` endpoint (or `/v1/models` for
/// OpenAI-compatible backends). Short timeout so the UI stays
/// responsive when the sidecar is dead. User-initiated — only runs
/// when the user opens the Prompt tab or clicks "Test connection".
pub async fn user_initiated_ping_sidecar(pool: &SqlitePool) -> AppResult<SidecarStatus> {
    let Some(url) = get_sidecar_url(pool).await? else {
        return Ok(SidecarStatus {
            configured: false,
            url: None,
            reachable: false,
            model: None,
            error: None,
        });
    };
    let preferred = get_preferred_model(pool).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .user_agent("Chronimage/0.1")
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let probe_url = format!("{}/v1/models", url.trim_end_matches('/'));
    let resp = client.get(&probe_url).send().await;
    match resp {
        Ok(r) if r.status().is_success() => Ok(SidecarStatus {
            configured: true,
            url: Some(url),
            reachable: true,
            model: preferred,
            error: None,
        }),
        Ok(r) => Ok(SidecarStatus {
            configured: true,
            url: Some(url),
            reachable: false,
            model: preferred,
            error: Some(format!("sidecar responded {}", r.status())),
        }),
        Err(e) => Ok(SidecarStatus {
            configured: true,
            url: Some(url),
            reachable: false,
            model: preferred,
            error: Some(e.to_string()),
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptEditRequest {
    pub photo_id: i64,
    pub prompt: String,
    pub strength: u8,
    pub constraints: Vec<String>,
    pub mask_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptEditResult {
    /// Base64-encoded rendered image, PNG.
    pub image_b64: String,
    pub latency_ms: u64,
    pub model_id: String,
    pub seed: i64,
}

/// Submit a generative edit. Reads the photo's working path from
/// `source_copies`, base64-encodes the bytes, posts the composite
/// payload, and returns the rendered image. Fails clean with
/// `AppError::SidecarMissing` when the sidecar URL is not set.
///
/// User-initiated — wired to the Prompt tab's Generate button.
pub async fn user_initiated_prompt_edit(
    pool: &SqlitePool,
    req: PromptEditRequest,
) -> AppResult<PromptEditResult> {
    let Some(url) = get_sidecar_url(pool).await? else {
        return Err(AppError::InvalidInput(
            "Prompt sidecar not configured. Open Settings → AI models → Prompt sidecar.".into(),
        ));
    };

    let photo_path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
    )
    .bind(req.photo_id)
    .fetch_optional(pool)
    .await?;
    let Some(path) = photo_path else {
        return Err(AppError::InvalidInput(format!(
            "photo {} has no on-disk copy to render from",
            req.photo_id
        )));
    };
    let bytes = std::fs::read(&path)?;
    use base64::Engine;
    let image_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let model = get_preferred_model(pool)
        .await?
        .unwrap_or_else(|| "flux-dev".into());

    let body = serde_json::json!({
        "model": model,
        "prompt": req.prompt,
        "strength": (req.strength as f32 / 100.0).clamp(0.0, 1.0),
        "image_b64": image_b64,
        "mask_b64": req.mask_b64,
        "constraints": req.constraints,
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("Chronimage/0.1")
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let endpoint = format!("{}/v1/edit", url.trim_end_matches('/'));
    let start = std::time::Instant::now();
    let resp = client
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("sidecar POST failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "sidecar returned {}",
            resp.status()
        )));
    }
    let parsed: SidecarResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("sidecar JSON parse failed: {e}")))?;
    Ok(PromptEditResult {
        image_b64: parsed.image_b64,
        latency_ms: parsed
            .latency_ms
            .unwrap_or_else(|| start.elapsed().as_millis() as u64),
        model_id: parsed.model_id.unwrap_or(model),
        seed: parsed.seed.unwrap_or(0),
    })
}

#[derive(Debug, Deserialize)]
struct SidecarResponse {
    image_b64: String,
    latency_ms: Option<u64>,
    model_id: Option<String>,
    seed: Option<i64>,
}

// ── SAM2 mask-from-prompt (Phase 4 §3) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskFromPromptRequest {
    pub photo_id: i64,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskFromPromptResult {
    /// Base64-encoded PNG mask, white = selected, black = unselected.
    pub mask_b64: String,
    pub confidence: f32,
    pub latency_ms: u64,
}

#[derive(Debug, Deserialize)]
struct MaskSidecarResponse {
    mask_b64: String,
    confidence: Option<f32>,
    latency_ms: Option<u64>,
}

/// Ask the configured sidecar for a SAM2+CLIP mask of the region the
/// prompt describes ("the sky", "the dog"). Uses the same sidecar URL
/// as generative edits; sidecar is expected to expose `/v1/mask`.
///
/// User-initiated — wired to the Prompt tab's Mask button.
pub async fn user_initiated_mask_from_prompt(
    pool: &SqlitePool,
    req: MaskFromPromptRequest,
) -> AppResult<MaskFromPromptResult> {
    let Some(url) = get_sidecar_url(pool).await? else {
        return Err(AppError::InvalidInput(
            "Mask sidecar not configured. Open Settings → AI models → Prompt sidecar.".into(),
        ));
    };

    let photo_path: Option<String> = sqlx::query_scalar(
        "SELECT path FROM source_copies WHERE photo_id = ?1 AND path IS NOT NULL LIMIT 1",
    )
    .bind(req.photo_id)
    .fetch_optional(pool)
    .await?;
    let Some(path) = photo_path else {
        return Err(AppError::InvalidInput(format!(
            "photo {} has no on-disk copy",
            req.photo_id
        )));
    };
    let bytes = std::fs::read(&path)?;
    use base64::Engine;
    let image_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let body = serde_json::json!({
        "model": "sam2",
        "prompt": req.prompt,
        "image_b64": image_b64,
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Chronimage/0.1")
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let endpoint = format!("{}/v1/mask", url.trim_end_matches('/'));
    let start = std::time::Instant::now();
    let resp = client
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("mask POST failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "mask sidecar returned {}",
            resp.status()
        )));
    }
    let parsed: MaskSidecarResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("mask JSON parse failed: {e}")))?;
    Ok(MaskFromPromptResult {
        mask_b64: parsed.mask_b64,
        confidence: parsed.confidence.unwrap_or(0.0),
        latency_ms: parsed
            .latency_ms
            .unwrap_or_else(|| start.elapsed().as_millis() as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};

    #[tokio::test]
    async fn sidecar_url_defaults_to_none_and_round_trips() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        assert!(get_sidecar_url(&pool).await.unwrap().is_none());
        set_sidecar_url(&pool, Some("http://localhost:17183"))
            .await
            .unwrap();
        assert_eq!(
            get_sidecar_url(&pool).await.unwrap().as_deref(),
            Some("http://localhost:17183")
        );
        // Clearing with empty string treats as None.
        set_sidecar_url(&pool, Some("   ")).await.unwrap();
        assert!(get_sidecar_url(&pool).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn ping_when_unconfigured_returns_not_configured() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        let status = user_initiated_ping_sidecar(&pool).await.unwrap();
        assert!(!status.configured);
        assert!(!status.reachable);
    }

    #[tokio::test]
    async fn prompt_edit_without_sidecar_is_actionable() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        let err = user_initiated_prompt_edit(
            &pool,
            PromptEditRequest {
                photo_id: 1,
                prompt: "lift shadows".into(),
                strength: 50,
                constraints: vec![],
                mask_b64: None,
            },
        )
        .await
        .expect_err("should error without sidecar");
        let msg = err.to_string();
        assert!(
            msg.contains("Prompt sidecar not configured"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn mask_from_prompt_without_sidecar_is_actionable() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        let err = user_initiated_mask_from_prompt(
            &pool,
            MaskFromPromptRequest {
                photo_id: 1,
                prompt: "the sky".into(),
            },
        )
        .await
        .expect_err("should error without sidecar");
        assert!(
            err.to_string().contains("Mask sidecar not configured"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn preferred_model_round_trips() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .expect("pool");
        assert!(get_preferred_model(&pool).await.unwrap().is_none());
        set_preferred_model(&pool, Some("sdxl-inpaint"))
            .await
            .unwrap();
        assert_eq!(
            get_preferred_model(&pool).await.unwrap().as_deref(),
            Some("sdxl-inpaint")
        );
    }
}
