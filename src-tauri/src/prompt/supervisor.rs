//! Phase 4 §2 — user-configured generative sidecar process supervisor.
//!
//! Chronimage ships as a **client**: the user runs Flux-dev / SDXL-
//! inpaint / comfyui / diffusers themselves and points the Prompt tab
//! at its HTTP endpoint. This module adds a lightweight convenience
//! layer on top — store the launch command in a KV setting, spawn/kill
//! the child from the app UI, and show live status in Settings so the
//! user doesn't have to tab to a terminal every time they restart the
//! app.
//!
//! It deliberately does **not** bundle an `externalBin`, download Flux
//! weights, or auto-restart on crash without user consent. Those are
//! packaging/policy decisions that belong in the release workstream
//! (Phase 5+), not v1 behaviour that surprises users.

use crate::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::{process::Child, sync::Mutex};

pub const SIDECAR_CMD_KEY: &str = "ai.prompt_sidecar_command";

pub async fn get_sidecar_command(pool: &SqlitePool) -> AppResult<Option<String>> {
    let row: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(SIDECAR_CMD_KEY)
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

pub async fn set_sidecar_command(pool: &SqlitePool, cmd: Option<&str>) -> AppResult<()> {
    let val = cmd.unwrap_or("").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(SIDECAR_CMD_KEY)
    .bind(&val)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarProcStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub command: Option<String>,
    pub started_at: Option<String>,
    pub last_exit_code: Option<i32>,
    pub configured: bool,
}

struct Handle {
    pid: u32,
    command: String,
    started_at: DateTime<Utc>,
    child: Child,
}

#[derive(Default)]
pub struct Supervisor {
    inner: Arc<Mutex<Option<Handle>>>,
    last_exit: Arc<Mutex<Option<i32>>>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the user-configured sidecar command. `shell=true` on
    /// Windows so commands like `python -m comfyui` or `.\\run.bat`
    /// resolve against PATH. No shell expansion on Unix — we split on
    /// whitespace and pass argv directly to keep surprises out.
    pub async fn start(&self, pool: &SqlitePool) -> AppResult<u32> {
        let mut guard = self.inner.lock().await;
        if let Some(h) = guard.as_ref() {
            return Err(AppError::InvalidInput(format!(
                "sidecar already running (pid {})",
                h.pid
            )));
        }
        let cmd = get_sidecar_command(pool).await?.ok_or_else(|| {
            AppError::InvalidInput(
                "sidecar command not configured. Open Settings → Prompt sidecar → Launch command."
                    .into(),
            )
        })?;

        let child = spawn_child(&cmd)?;
        let pid = child.id().unwrap_or_default();
        tracing::info!(%cmd, pid, "sidecar started");
        *guard = Some(Handle {
            pid,
            command: cmd,
            started_at: Utc::now(),
            child,
        });
        // Fresh start → clear any prior exit trace.
        *self.last_exit.lock().await = None;
        Ok(pid)
    }

    pub async fn stop(&self) -> AppResult<()> {
        let mut guard = self.inner.lock().await;
        if let Some(mut h) = guard.take() {
            tracing::info!(pid = h.pid, "sidecar stop requested");
            // Best-effort kill; wait to reap so we don't leave zombies.
            if let Err(e) = h.child.kill().await {
                tracing::warn!(error = %e, "sidecar kill failed");
            }
            let exit = h.child.wait().await.ok().and_then(|s| s.code());
            *self.last_exit.lock().await = exit;
        }
        Ok(())
    }

    pub async fn status(&self, pool: &SqlitePool) -> AppResult<SidecarProcStatus> {
        let mut guard = self.inner.lock().await;
        let configured = get_sidecar_command(pool).await?.is_some();
        // Poll the child — if it has exited, clear the handle and
        // record the exit code so the UI can surface it.
        let running = if let Some(h) = guard.as_mut() {
            match h.child.try_wait()? {
                Some(status) => {
                    *self.last_exit.lock().await = status.code();
                    *guard = None;
                    false
                }
                None => true,
            }
        } else {
            false
        };
        let last_exit = *self.last_exit.lock().await;
        Ok(SidecarProcStatus {
            running,
            pid: guard.as_ref().map(|h| h.pid),
            command: guard.as_ref().map(|h| h.command.clone()),
            started_at: guard.as_ref().map(|h| h.started_at.to_rfc3339()),
            last_exit_code: last_exit,
            configured,
        })
    }
}

fn spawn_child(cmd: &str) -> AppResult<Child> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let Some(program) = parts.first() else {
        return Err(AppError::InvalidInput("empty sidecar command".into()));
    };
    let args: &[&str] = if parts.len() > 1 { &parts[1..] } else { &[] };

    let mut builder = tokio::process::Command::new(program);
    builder.args(args);
    builder.kill_on_drop(true);
    // Pipe stdout/stderr so the child doesn't inherit our tty on
    // Windows (avoids the "new console window" pop when running via
    // double-click-launched MSIs).
    builder.stdout(std::process::Stdio::piped());
    builder.stderr(std::process::Stdio::piped());

    builder
        .spawn()
        .map_err(|e| AppError::Internal(format!("sidecar spawn failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};

    #[tokio::test]
    async fn command_kv_round_trips() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        assert!(get_sidecar_command(&pool).await.unwrap().is_none());
        set_sidecar_command(&pool, Some("python -m comfyui"))
            .await
            .unwrap();
        assert_eq!(
            get_sidecar_command(&pool).await.unwrap().as_deref(),
            Some("python -m comfyui")
        );
        set_sidecar_command(&pool, None).await.unwrap();
        assert!(get_sidecar_command(&pool).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn start_without_command_returns_actionable_error() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        let sup = Supervisor::new();
        let err = sup.start(&pool).await.expect_err("should fail");
        assert!(
            err.to_string().contains("sidecar command not configured"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn status_reflects_configured_flag() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        let sup = Supervisor::new();
        let s = sup.status(&pool).await.unwrap();
        assert!(!s.running);
        assert!(!s.configured);

        set_sidecar_command(&pool, Some("echo hello"))
            .await
            .unwrap();
        let s = sup.status(&pool).await.unwrap();
        assert!(s.configured);
        assert!(!s.running);
    }
}
