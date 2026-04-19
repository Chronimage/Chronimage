use crate::AppResult;
use sqlx::SqlitePool;

struct AlbumSeed {
    name: &'static str,
    description: &'static str,
    tag: &'static str,
    rule_json: &'static str,
}

// ── 12 design-specified system albums ────────────────────────────────────────
//
// Rediscovery albums ("On this day", "First time on new camera",
// "Unflagged favorites") are owned by catalog::rediscovery and seeded
// separately so they can carry runtime-computed values (e.g. today's MM-DD).

const SYSTEM_ALBUMS: &[AlbumSeed] = &[
    AlbumSeed {
        name: "Portraits",
        description: "People in focus",
        tag: "faces",
        rule_json: r#"{"type":"tag","value":"faces"}"#,
    },
    AlbumSeed {
        name: "Golden Hour",
        description: "Warm sunset light",
        tag: "lighting",
        rule_json: r#"{"type":"tag","value":"golden_hour"}"#,
    },
    AlbumSeed {
        name: "Kids — Ari & Leo",
        description: "Family, 2024–26",
        tag: "people",
        rule_json: r#"{"type":"cluster","value":"kids"}"#,
    },
    AlbumSeed {
        name: "Food & Kitchen",
        description: "Meals worth remembering",
        tag: "scenes",
        rule_json: r#"{"type":"tag","value":"food"}"#,
    },
    AlbumSeed {
        name: "Japan · Autumn '25",
        description: "Kyoto → Tokyo",
        tag: "place",
        rule_json: r#"{"type":"tag","value":"japan"}"#,
    },
    AlbumSeed {
        name: "Loop 2 · product shots",
        description: "Studio catalog",
        tag: "work",
        rule_json: r#"{"type":"tag","value":"product"}"#,
    },
    AlbumSeed {
        name: "Night & Low Light",
        description: "ISO ≥ 3200",
        tag: "lighting",
        rule_json: r#"{"type":"exif","field":"iso","op":"gte","value":3200}"#,
    },
    AlbumSeed {
        name: "Weddings & Events",
        description: "Client deliveries",
        tag: "event",
        rule_json: r#"{"type":"tag","value":"event"}"#,
    },
    AlbumSeed {
        name: "Milo (golden retriever)",
        description: "On-device recognition",
        tag: "people",
        rule_json: r#"{"type":"cluster","value":"pets"}"#,
    },
    AlbumSeed {
        name: "Screenshots & Docs",
        description: "Auto-archived",
        tag: "utility",
        rule_json: r#"{"type":"tag","value":"screenshot"}"#,
    },
    AlbumSeed {
        name: "Burst & Duplicates",
        description: "Flagged by similarity",
        tag: "cull",
        rule_json: r#"{"type":"tag","value":"duplicate"}"#,
    },
    AlbumSeed {
        name: "Out-of-focus",
        description: "Low sharpness score",
        tag: "cull",
        rule_json: r#"{"type":"quality","field":"sharpness","op":"lt","value":0.3}"#,
    },
];

/// Insert the 12 design-specified system smart albums if they haven't been
/// seeded yet, then seed the four rediscovery albums via
/// [`crate::catalog::rediscovery::seed_rediscovery_albums`].
///
/// Safe to call on every startup — the system-album guard skips when
/// `is_system` rows already exist; `seed_rediscovery_albums` is always called
/// so the re-evaluator keeps the MM-DD current on each boot.
pub async fn seed_default_smart_albums(pool: &SqlitePool) -> AppResult<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM smart_albums WHERE is_system = 1")
        .fetch_one(pool)
        .await?;

    if count == 0 {
        let now = chrono::Utc::now().to_rfc3339();
        for album in SYSTEM_ALBUMS {
            sqlx::query(
                "INSERT OR IGNORE INTO smart_albums
                 (name, description, rule_json, cover_photo_ids, tag, is_system, created_at, updated_at)
                 VALUES (?1, ?2, ?3, '[]', ?4, 1, ?5, ?5)",
            )
            .bind(album.name)
            .bind(album.description)
            .bind(album.rule_json)
            .bind(album.tag)
            .bind(&now)
            .execute(pool)
            .await?;
        }
        tracing::info!(count = SYSTEM_ALBUMS.len(), "seeded system smart albums");
    }

    // Always run — keeps kind='rediscovery_today' rows current on every boot.
    crate::catalog::rediscovery::seed_rediscovery_albums(pool).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db::{open_pool, PoolOptions};
    use tempfile::TempDir;

    #[tokio::test]
    async fn seeds_12_system_albums_on_empty_catalog() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = open_pool(PoolOptions::new(tmp.path().join("c.db")))
            .await
            .expect("pool");

        seed_default_smart_albums(&pool).await.expect("seed");

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM smart_albums WHERE is_system = 1")
                .fetch_one(&pool)
                .await
                .expect("count");
        // 12 static + 4 rediscovery = 16 total system albums.
        assert_eq!(count, 16);
    }

    #[tokio::test]
    async fn seed_is_idempotent() {
        let tmp = TempDir::new().expect("tempdir");
        let pool = open_pool(PoolOptions::new(tmp.path().join("c.db")))
            .await
            .expect("pool");

        seed_default_smart_albums(&pool).await.expect("first");
        seed_default_smart_albums(&pool).await.expect("second");

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM smart_albums WHERE is_system = 1")
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count, 16);
    }
}
