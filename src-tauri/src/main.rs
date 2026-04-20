// Chronimage — Tauri app entrypoint.
//
// All interesting logic lives in the library crate (`chronimage`). This file
// only bootstraps the Tauri runtime and registers plugins + commands.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chronimage::{
    ai::{budget, caption::CaptionSession, faces::FacesSession},
    catalog::{
        db::{open_pool, PoolOptions},
        seed_default_smart_albums,
    },
    commands,
    state::AppState,
    util::paths::{bundled_models_dir, catalog_db_path, models_dir},
};
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn install_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("chronimage=debug,tauri=info,sqlx=warn"));

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).compact());

    // Ship logs to Loki when available (dev only). Silently skip if Loki is not running.
    #[cfg(debug_assertions)]
    {
        let loki_url =
            std::env::var("LOKI_URL").unwrap_or_else(|_| "http://localhost:3101".to_string());
        let builder_result = tracing_loki::builder()
            .label("app", "chronimage")
            .and_then(|b| b.label("env", "dev"))
            .and_then(|b| b.extra_field("pid", std::process::id().to_string()))
            .and_then(|b| {
                b.build_url(
                    tracing_loki::url::Url::parse(&format!("{loki_url}/loki/api/v1/push"))
                        .expect("loki url"),
                )
            });
        match builder_result {
            Ok((loki_layer, task)) => {
                tauri::async_runtime::spawn(task);
                let _ = registry.with(loki_layer).try_init();
                tracing::info!(loki_url, "loki log shipping enabled");
            }
            Err(e) => {
                let _ = registry.try_init();
                tracing::warn!(error = %e, "loki layer init failed, stdout only");
            }
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = registry.try_init();
    }
}

#[cfg(target_os = "windows")]
fn apply_window_effects(window: &tauri::WebviewWindow) {
    use window_vibrancy::apply_mica;
    // Mica may fail on <Win11 22H2 — fall back silently.
    let _ = apply_mica(window, Some(true));
}

#[cfg(not(target_os = "windows"))]
fn apply_window_effects(_window: &tauri::WebviewWindow) {}

fn main() {
    install_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::app_version,
            commands::current_channel,
            commands::import_dry_run,
            commands::start_import,
            commands::list_imports,
            commands::list_albums,
            commands::list_photos,
            commands::list_sources,
            commands::create_source,
            commands::delete_source,
            commands::on_this_day,
            commands::unseen_photos,
            commands::cleanup_dry_run,
            commands::refresh_smart_albums,
            commands::import_google_takeout,
            commands::detect_icloud_path,
            commands::list_iphone_devices,
            commands::detect_hardware,
            commands::embed_image,
            commands::score_aesthetic,
            commands::download_models,
            commands::find_duplicates,
            commands::search_photos,
            commands::ai_models_status,
            commands::cleanup_execute,
            commands::lift_shift_dry_run,
            commands::lift_shift_execute,
            commands::face_clusters_list,
            commands::face_cluster_name,
            commands::face_cluster_merge,
            commands::record_photo_view,
            commands::ai_reindex,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_window_effects(&window);
            }
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "chronimage starting");

            // Open the pool + build AI sessions SYNCHRONOUSLY before setup
            // returns, so `AppState` is managed before the webview loads and
            // the frontend can issue commands immediately. A previous
            // `async_runtime::spawn` registration caused a race where
            // `create_source` / other commands would fire before state was
            // managed, surfacing as "state not managed for field `state`".
            let db_path = catalog_db_path()?;
            let handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(async move {
                open_pool(PoolOptions::new(db_path)).await
            })
            .map_err(|e| {
                tracing::error!(error = %e, "failed to open catalog pool");
                Box::new(e) as Box<dyn std::error::Error>
            })?;
            tauri::async_runtime::block_on(async {
                if let Err(e) = seed_default_smart_albums(&pool).await {
                    tracing::warn!(error = %e, "smart album seed failed (non-fatal)");
                }
            });

            // Spawn the background re-evaluator (10-minute cadence).
            // spawn_reevaluator calls tokio::spawn internally, which requires a
            // running Tokio reactor on the current thread. Setup runs on the
            // main thread (no reactor); async_runtime::block_on enters Tauri's
            // runtime context just long enough for tokio::spawn to succeed.
            #[cfg(not(test))]
            {
                use chronimage::albums::reevaluator::spawn_reevaluator;
                let reeval_pool = pool.clone();
                tauri::async_runtime::block_on(async move {
                    // JoinHandle dropped intentionally — the task runs until process exit.
                    std::mem::drop(spawn_reevaluator(reeval_pool));
                });
            }

            // Build AI sessions (stub when model files absent).
            // Resolution order: bundled installer dir first, then user data
            // dir (downloaded on-demand). Missing paths fall through to stub.
            let bundled = bundled_models_dir(&handle);
            let md = models_dir().ok();

            /// Resolve a model filename: bundled dir first, user data dir
            /// second. Returns None when absent in both.
            fn resolve_model(
                bundled: &Option<std::path::PathBuf>,
                user: &Option<std::path::PathBuf>,
                filename: &str,
            ) -> Option<std::path::PathBuf> {
                if let Some(b) = bundled {
                    let p = b.join(filename);
                    if p.exists() {
                        tracing::debug!(path = %p.display(), "model resolved from bundled dir");
                        return Some(p);
                    }
                }
                if let Some(u) = user {
                    let p = u.join(filename);
                    if p.exists() {
                        tracing::debug!(path = %p.display(), "model resolved from user data dir");
                        return Some(p);
                    }
                }
                None
            }

            let retina_path = resolve_model(&bundled, &md, "det_10g.onnx");
            let arcface_path = resolve_model(&bundled, &md, "w600k_r50.onnx");
            let faces = Arc::new(FacesSession::load_or_stub(
                retina_path.as_deref(),
                arcface_path.as_deref(),
            ));

            // Moondream2 is never bundled — user data dir only.
            let gguf_path = md
                .as_deref()
                .map(|d| d.join("moondream2-text-model-f16.gguf"))
                .filter(|p| p.exists());
            let tier = budget::detect().tier;
            let caption = Arc::new(CaptionSession::load_or_stub(
                gguf_path.as_deref(),
                // TODO(cc): resolve sidecar binary path from bundled dir in phase-1b
                None,
                tier,
            ));

            app.manage(AppState {
                pool,
                faces,
                caption,
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .inspect_err(|e| tracing::error!(error = %e, "tauri runtime exited with error"))
        .ok();
}
