//! End-to-end pipeline diagnostic driver.
//!
//! Drives the real import pipeline against a real folder on disk so we can see
//! stage + per-photo timings without needing tauri-driver / Playwright. The
//! pipeline's own `tracing::info!/debug!` logs fire into stdout via
//! `tracing_subscriber::fmt` and — when Loki is running at :3101 — also get
//! shipped there via the same `util::loki` layer the app uses in dev builds.
//!
//! ## How to run
//!
//! ```
//! # From the repo root
//! CHRONIMAGE_DEBUG_IMPORT_SRC='C:\Users\jayas\OneDrive\Pictures\ugadi 2026' \
//!   cargo test --manifest-path src-tauri/Cargo.toml --test debug_import \
//!   -- --ignored --nocapture debug_import_timing
//! ```
//!
//! Env knobs:
//!   - `CHRONIMAGE_DEBUG_IMPORT_SRC` (required) — absolute path to the folder
//!     of photos to import.
//!   - `CHRONIMAGE_MODELS_DIR` (optional) — override model dir. Defaults to
//!     the real app data dir (`%LOCALAPPDATA%\app.chronimage.desktop\models`)
//!     so DML-backed SigLIP / NIMA / RetinaFace / ArcFace all load with real
//!     weights — the whole point of this test is to measure them.
//!
//! The test is `#[ignore]` by default so the standard `cargo test` run stays
//! fast and hermetic. Pass `-- --ignored` to opt in.

use chronimage::{
    ai::{faces::init_global_faces_session, siglip::init_global_siglip_session},
    catalog::db::{open_pool, PoolOptions},
    import::pipeline::{run_pipeline_headless, ImportProgress},
    util::paths::models_dir,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
use tempfile::TempDir;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialise tracing once for the test process so pipeline logs stream to
/// stdout AND (if Loki is up) to the local Loki instance on port 3101.
fn install_tracing_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("chronimage=debug,sqlx=warn"));
        let fmt_layer = fmt::layer().with_target(true).compact();

        // Best-effort Loki shipping — same URL as the app uses in dev.
        let loki_url =
            std::env::var("LOKI_URL").unwrap_or_else(|_| "http://localhost:3101".to_string());
        let push_url = format!("{loki_url}/loki/api/v1/push");
        let labels: chronimage::util::loki::Labels = vec![
            ("app".into(), "chronimage".into()),
            ("env".into(), "dev".into()),
            ("layer".into(), "backend".into()),
            ("run".into(), "debug-import".into()),
            ("pid".into(), std::process::id().to_string()),
        ];
        let loki_layer = chronimage::util::loki::LokiLayer::spawn(push_url, labels);

        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(loki_layer)
            .try_init();
    });
}

/// Resolve a model file by name from the configured models dir.
fn resolve_model(models: &std::path::Path, filename: &str) -> Option<PathBuf> {
    let p = models.join(filename);
    p.exists().then_some(p)
}

#[tokio::test]
#[ignore = "opt-in — requires real source folder on disk + real model files"]
async fn debug_import_timing() {
    install_tracing_once();

    let src = std::env::var("CHRONIMAGE_DEBUG_IMPORT_SRC")
        .expect("CHRONIMAGE_DEBUG_IMPORT_SRC must point to the folder to import");
    let src_path = PathBuf::from(&src);
    assert!(
        src_path.is_dir(),
        "CHRONIMAGE_DEBUG_IMPORT_SRC is not a directory: {src}"
    );

    // Load real models from the user's app-data dir so we exercise the same
    // code path as the production app. This is intentional: we want timing
    // that reflects real DML inference + real RAW decode paths.
    let models = models_dir().expect("models_dir");
    assert!(
        models.join("siglip2-b16-image.onnx").exists(),
        "SigLIP image model missing at {}",
        models.display()
    );

    tracing::info!(
        src = %src_path.display(),
        models = %models.display(),
        "debug-import: resolved paths"
    );

    init_global_faces_session(
        resolve_model(&models, "det_10g.onnx").as_deref(),
        resolve_model(&models, "w600k_r50.onnx").as_deref(),
    );
    init_global_siglip_session(
        resolve_model(&models, "siglip2-b16-image.onnx").as_deref(),
        resolve_model(&models, "siglip2-b16-text.onnx").as_deref(),
        resolve_model(&models, "siglip2-b16-tokenizer.json").as_deref(),
    );

    // Throwaway DB — we never touch the user's real catalog.
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("debug-import.db");
    let pool = open_pool(PoolOptions::new(db_path))
        .await
        .expect("open_pool");

    // Seed a source row. `root_path` lives in `config_json` as `{"root": ...}`
    // — see `commands::create_source`.
    let now = chrono::Utc::now().to_rfc3339();
    let config = serde_json::json!({ "root": src_path.to_string_lossy() }).to_string();
    let source_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, config_json, created_at)
         VALUES ('debug-import', 'local', 'idle', ?1, ?2) RETURNING id",
    )
    .bind(&config)
    .bind(&now)
    .fetch_one(&pool)
    .await
    .expect("insert source");

    tracing::info!(source_id, "debug-import: source row created");

    let emitted = Arc::new(AtomicUsize::new(0));
    let emitted_cb = Arc::clone(&emitted);
    let on_progress: Arc<dyn Fn(ImportProgress) + Send + Sync> =
        Arc::new(move |p: ImportProgress| {
            let n = emitted_cb.fetch_add(1, Ordering::Relaxed);
            // Log every 10th tick so the per-photo detail stays readable.
            if n.is_multiple_of(10) {
                tracing::info!(
                    done = p.done,
                    total = p.total,
                    eta_seconds = p.eta_seconds.unwrap_or(0),
                    file = %p.current_file,
                    "debug-import: progress tick"
                );
            }
        });

    let t0 = Instant::now();
    let result = run_pipeline_headless(source_id, src_path, pool.clone(), on_progress)
        .await
        .expect("pipeline");
    let wall_ms = t0.elapsed().as_millis();

    tracing::info!(
        import_id = result.import_id,
        imported = result.imported_count,
        skipped = result.skipped_count,
        errors = result.error_count,
        wall_ms = wall_ms as u64,
        "debug-import: pipeline returned"
    );

    // Quick post-run sanity: how many photos actually got embeddings?
    let embed_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photo_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let faces_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM faces")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let photos_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM photos")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let aesthetic_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM photos WHERE aesthetic_score IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

    tracing::info!(
        photos_count,
        embed_count,
        aesthetic_count,
        faces_count,
        "debug-import: catalog post-run sanity"
    );

    println!(
        "\n────────────────────────────────────────\n\
         debug-import result:\n\
         photos imported        {}\n\
         siglip embeddings      {}\n\
         nima aesthetic scores  {}\n\
         face detections        {}\n\
         wall clock             {} ms\n\
         per photo (wall)       {} ms\n\
         ────────────────────────────────────────\n",
        photos_count,
        embed_count,
        aesthetic_count,
        faces_count,
        wall_ms,
        if photos_count > 0 {
            wall_ms / photos_count as u128
        } else {
            0
        },
    );
}
