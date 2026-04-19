// Chronimage — Tauri app entrypoint.
//
// All interesting logic lives in the library crate (`chronimage`). This file
// only bootstraps the Tauri runtime and registers plugins + commands.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chronimage::{
    catalog::{
        db::{open_pool, PoolOptions},
        seed_default_smart_albums,
    },
    commands,
    state::AppState,
    util::paths::catalog_db_path,
};
use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn install_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("chronimage=info,tauri=info,sqlx=warn"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).compact())
        .try_init();
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
        .plugin(tauri_plugin_log::Builder::default().build())
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
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_window_effects(&window);
            }
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "chronimage starting");

            let db_path = catalog_db_path()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match open_pool(PoolOptions::new(db_path)).await {
                    Ok(pool) => {
                        if let Err(e) = seed_default_smart_albums(&pool).await {
                            tracing::warn!(error = %e, "smart album seed failed (non-fatal)");
                        }
                        handle.manage(AppState { pool });
                    }
                    Err(e) => tracing::error!(error = %e, "failed to open catalog pool"),
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .inspect_err(|e| tracing::error!(error = %e, "tauri runtime exited with error"))
        .ok();
}
