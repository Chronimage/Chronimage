//! Preset library — built-in presets (hard-coded) + user-saved presets
//! (`presets` table).
//!
//! Built-in presets stay scalar-slider only so every preset can render
//! through the Phase 3 CPU pipeline. Mask-assisted presets in the UI use
//! the prompt/SAM2 path and still feed scalar operations into the preview.

use super::ops::{Curves, Operations};
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Preset wire type — same for built-in + user-saved so the frontend
/// treats them uniformly.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Preset {
    pub id: i64,
    pub name: String,
    pub group_name: String,
    pub description: Option<String>,
    pub operations_json: String,
    pub is_system: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Preset {
    pub fn operations(&self) -> AppResult<Operations> {
        serde_json::from_str(&self.operations_json).map_err(AppError::from)
    }
}

fn curve(points: [[f32; 2]; 5]) -> Vec<[f32; 2]> {
    points.to_vec()
}

fn master_curve(points: [[f32; 2]; 5]) -> Curves {
    Curves {
        rgb: curve(points),
        ..Curves::identity()
    }
}

fn luma_curve(points: [[f32; 2]; 5]) -> Curves {
    Curves {
        l: curve(points),
        ..Curves::identity()
    }
}

/// The built-in presets available in the Develop side panel. Returns
/// `(name, group, description, ops)` tuples; the seed step materialises
/// them into the `presets` table at startup.
pub fn builtin_presets() -> Vec<(&'static str, &'static str, &'static str, Operations)> {
    vec![
        (
            "Clean up face",
            "Face",
            "Subtle skin smoothing: lifts shadows, soft contrast, warm.",
            Operations {
                exposure: 0.1,
                contrast: -8.0,
                highlights: -10.0,
                shadows: 25.0,
                temp: 8.0,
                tint: -2.0,
                vibrance: 8.0,
                saturation: 0.0,
                clarity: -12.0,
                dehaze: 0.0,
                whites: 0.0,
                blacks: 0.0,
                curves: crate::develop::ops::Curves::identity(),
            },
        ),
        (
            "Whiten teeth",
            "Face",
            "Small white-point lift with reduced yellow saturation.",
            Operations {
                exposure: 0.05,
                contrast: 4.0,
                highlights: 12.0,
                shadows: 0.0,
                whites: 24.0,
                blacks: 0.0,
                temp: -8.0,
                tint: 0.0,
                vibrance: -4.0,
                saturation: -18.0,
                clarity: 4.0,
                dehaze: 0.0,
                curves: luma_curve([
                    [0.0, 0.0],
                    [0.25, 0.27],
                    [0.5, 0.55],
                    [0.75, 0.82],
                    [1.0, 1.0],
                ]),
            },
        ),
        (
            "Eye pop",
            "Face",
            "Crisp iris detail and controlled contrast.",
            Operations {
                exposure: 0.0,
                contrast: 10.0,
                highlights: -8.0,
                shadows: 8.0,
                whites: 10.0,
                blacks: -4.0,
                temp: 0.0,
                tint: 2.0,
                vibrance: 16.0,
                saturation: 4.0,
                clarity: 28.0,
                dehaze: 4.0,
                curves: Curves::identity(),
            },
        ),
        (
            "Enhance sky",
            "Scene",
            "Deeper blues, crisper clouds, more pop in highlights.",
            Operations {
                exposure: 0.0,
                contrast: 12.0,
                highlights: -30.0,
                shadows: 5.0,
                whites: 10.0,
                blacks: -5.0,
                temp: -6.0,
                tint: 2.0,
                vibrance: 20.0,
                saturation: 8.0,
                clarity: 20.0,
                dehaze: 25.0,
                curves: crate::develop::ops::Curves::identity(),
            },
        ),
        (
            "Golden hour",
            "Scene",
            "Warm highlights with open shadows and soft contrast.",
            Operations {
                exposure: 0.12,
                contrast: 6.0,
                highlights: -24.0,
                shadows: 24.0,
                whites: 4.0,
                blacks: -6.0,
                temp: 24.0,
                tint: 6.0,
                vibrance: 18.0,
                saturation: 6.0,
                clarity: 4.0,
                dehaze: -2.0,
                curves: master_curve([
                    [0.0, 0.03],
                    [0.25, 0.28],
                    [0.5, 0.54],
                    [0.75, 0.78],
                    [1.0, 1.0],
                ]),
            },
        ),
        (
            "Urban night",
            "Scene",
            "Cool shadows, protected highlights, neon color pop.",
            Operations {
                exposure: -0.1,
                contrast: 24.0,
                highlights: -38.0,
                shadows: 18.0,
                whites: 8.0,
                blacks: 18.0,
                temp: -18.0,
                tint: 10.0,
                vibrance: 32.0,
                saturation: 8.0,
                clarity: 18.0,
                dehaze: 18.0,
                curves: master_curve([
                    [0.0, 0.0],
                    [0.25, 0.2],
                    [0.5, 0.5],
                    [0.75, 0.82],
                    [1.0, 1.0],
                ]),
            },
        ),
        (
            "Portrait relight",
            "Face",
            "Fill shadows, reign in highlights, warm skin.",
            Operations {
                exposure: 0.2,
                contrast: -5.0,
                highlights: -40.0,
                shadows: 45.0,
                whites: -10.0,
                blacks: 15.0,
                temp: 10.0,
                tint: -3.0,
                vibrance: 10.0,
                saturation: 0.0,
                clarity: 5.0,
                dehaze: 0.0,
                curves: crate::develop::ops::Curves::identity(),
            },
        ),
        (
            "Denoise low-light",
            "Quality",
            "Preview-friendly smoothing for noisy high-ISO files.",
            Operations {
                exposure: 0.05,
                contrast: -12.0,
                highlights: -18.0,
                shadows: 18.0,
                whites: -4.0,
                blacks: -6.0,
                temp: 0.0,
                tint: 0.0,
                vibrance: -2.0,
                saturation: -6.0,
                clarity: -36.0,
                dehaze: -8.0,
                curves: Curves::identity(),
            },
        ),
        (
            "Recover shadows",
            "Quality",
            "Lift blocked dark regions while holding bright detail.",
            Operations {
                exposure: 0.18,
                contrast: -8.0,
                highlights: -44.0,
                shadows: 62.0,
                whites: -8.0,
                blacks: -12.0,
                temp: 2.0,
                tint: 0.0,
                vibrance: 8.0,
                saturation: 0.0,
                clarity: 6.0,
                dehaze: 2.0,
                curves: luma_curve([
                    [0.0, 0.04],
                    [0.25, 0.34],
                    [0.5, 0.56],
                    [0.75, 0.78],
                    [1.0, 1.0],
                ]),
            },
        ),
        (
            "Moody portrait",
            "Style",
            "Lower saturation, deeper blacks, cinematic contrast.",
            Operations {
                exposure: -0.06,
                contrast: 20.0,
                highlights: -24.0,
                shadows: 10.0,
                whites: -4.0,
                blacks: 18.0,
                temp: 6.0,
                tint: 2.0,
                vibrance: -8.0,
                saturation: -18.0,
                clarity: 10.0,
                dehaze: 8.0,
                curves: master_curve([
                    [0.0, 0.0],
                    [0.25, 0.2],
                    [0.5, 0.48],
                    [0.75, 0.8],
                    [1.0, 1.0],
                ]),
            },
        ),
        (
            "Faded film",
            "Style",
            "Lifted blacks with muted color and a soft shoulder.",
            Operations {
                exposure: 0.0,
                contrast: -8.0,
                highlights: -20.0,
                shadows: 18.0,
                whites: -8.0,
                blacks: -28.0,
                temp: 6.0,
                tint: 4.0,
                vibrance: 8.0,
                saturation: -18.0,
                clarity: -4.0,
                dehaze: -6.0,
                curves: master_curve([
                    [0.0, 0.08],
                    [0.25, 0.28],
                    [0.5, 0.52],
                    [0.75, 0.75],
                    [1.0, 0.95],
                ]),
            },
        ),
        (
            "B&W film (Tri-X)",
            "Style",
            "High-contrast monochrome emulation, lifted shadows, film grain feel.",
            Operations {
                exposure: 0.0,
                contrast: 30.0,
                highlights: -20.0,
                shadows: 15.0,
                whites: 10.0,
                blacks: -15.0,
                temp: 0.0,
                tint: 0.0,
                vibrance: 0.0,
                saturation: -100.0, // full desaturate
                clarity: 25.0,
                dehaze: 10.0,
                curves: crate::develop::ops::Curves::identity(),
            },
        ),
    ]
}

/// Seed the `presets` table with built-ins. Idempotent — upsert by name.
pub async fn seed_builtins(pool: &SqlitePool) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    for (name, group, desc, ops) in builtin_presets() {
        let ops_json = serde_json::to_string(&ops)?;
        sqlx::query(
            "INSERT INTO presets (name, group_name, description, operations_json, is_system, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5) \
             ON CONFLICT(name) DO UPDATE SET \
               group_name = excluded.group_name, \
               description = excluded.description, \
               operations_json = excluded.operations_json, \
               updated_at = excluded.updated_at",
        )
        .bind(name)
        .bind(group)
        .bind(desc)
        .bind(&ops_json)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// List presets, optionally filtered by group.
pub async fn list(pool: &SqlitePool, group: Option<&str>) -> AppResult<Vec<Preset>> {
    match group {
        Some(g) => sqlx::query_as::<_, Preset>(
            "SELECT id, name, group_name, description, operations_json, is_system, \
                    created_at, updated_at \
             FROM presets WHERE group_name = ?1 ORDER BY is_system DESC, name ASC",
        )
        .bind(g)
        .fetch_all(pool)
        .await
        .map_err(AppError::from),
        None => sqlx::query_as::<_, Preset>(
            "SELECT id, name, group_name, description, operations_json, is_system, \
                    created_at, updated_at \
             FROM presets ORDER BY is_system DESC, group_name ASC, name ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(AppError::from),
    }
}

/// Load one preset by id.
pub async fn load(pool: &SqlitePool, preset_id: i64) -> AppResult<Preset> {
    sqlx::query_as::<_, Preset>(
        "SELECT id, name, group_name, description, operations_json, is_system, \
                created_at, updated_at \
         FROM presets WHERE id = ?1",
    )
    .bind(preset_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("preset {preset_id}")))
}

/// Save a user preset (is_system = 0). Name must be unique; clashes with
/// built-ins are rejected.
pub async fn save_user(
    pool: &SqlitePool,
    name: &str,
    group: &str,
    ops: &Operations,
) -> AppResult<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput(
            "preset name must not be empty".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let ops_json = serde_json::to_string(ops)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO presets (name, group_name, description, operations_json, is_system, created_at, updated_at) \
         VALUES (?1, ?2, NULL, ?3, 0, ?4, ?4) RETURNING id",
    )
    .bind(name)
    .bind(group)
    .bind(&ops_json)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};

    #[tokio::test]
    async fn seed_is_idempotent() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed_builtins(&pool).await.unwrap();
        seed_builtins(&pool).await.unwrap();
        let all = list(&pool, None).await.unwrap();
        assert_eq!(all.len(), 12);
    }

    #[tokio::test]
    async fn list_filters_by_group() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed_builtins(&pool).await.unwrap();
        let face = list(&pool, Some("Face")).await.unwrap();
        assert_eq!(face.len(), 4, "Face presets should all be seeded");
        assert!(face.iter().all(|p| p.group_name == "Face"));

        let quality = list(&pool, Some("Quality")).await.unwrap();
        assert_eq!(quality.len(), 2, "Quality presets should be live");
        assert!(quality.iter().all(|p| p.group_name == "Quality"));
    }

    #[tokio::test]
    async fn operations_roundtrip() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed_builtins(&pool).await.unwrap();
        let all = list(&pool, None).await.unwrap();
        for p in &all {
            let _ops = p.operations().expect("parse");
        }
    }

    #[tokio::test]
    async fn save_user_persists() {
        let pool = open_pool(PoolOptions::new(":memory:".into()))
            .await
            .unwrap();
        seed_builtins(&pool).await.unwrap();
        let custom = Operations {
            saturation: 50.0,
            ..Operations::identity()
        };
        let id = save_user(&pool, "My look", "Style", &custom).await.unwrap();
        let loaded = load(&pool, id).await.unwrap();
        assert_eq!(loaded.name, "My look");
        assert!(!loaded.is_system);
    }
}
