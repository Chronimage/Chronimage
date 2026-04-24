//! Phase 5 §4 acceptance test: fresh catalog has telemetry disabled
//! by default. The user must explicitly opt in via the first-run
//! dialog or Settings → Privacy.

use chronimage::catalog::db::{open_pool, PoolOptions};

#[tokio::test]
async fn telemetry_disabled_on_fresh_install() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");
    let row: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'telemetry.enabled'")
            .fetch_optional(&pool)
            .await
            .expect("query");
    // Migration sets the row to '0' explicitly; it must never be missing
    // and never be truthy on first install.
    assert_eq!(
        row.as_deref(),
        Some("0"),
        "telemetry must default off on first install"
    );
}

#[tokio::test]
async fn telemetry_opt_in_persists() {
    let pool = open_pool(PoolOptions::new(":memory:".into()))
        .await
        .expect("pool");

    sqlx::query(
        "INSERT INTO settings(key, value, updated_at) VALUES ('telemetry.enabled', '1', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .expect("update");

    let row: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'telemetry.enabled'")
            .fetch_optional(&pool)
            .await
            .expect("query");
    assert_eq!(row.as_deref(), Some("1"));
}
