// Chronimage — Tauri app entrypoint.
//
// All interesting logic lives in the library crate (`chronimage`). This file
// only bootstraps the Tauri runtime and registers plugins + commands.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chronimage::{
    ai::{faces::init_global_faces_session, siglip::init_global_siglip_session},
    catalog::{
        db::{open_pool, PoolOptions},
        seed_default_smart_albums,
    },
    commands,
    develop::sam::init_global_sam_session,
    state::AppState,
    util::paths::{bundled_models_dir, catalog_db_path, models_dir},
};
use tauri::Manager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn install_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("chronimage=debug,tauri=info,sqlx=warn"));

    let console_layer = fmt::layer().with_target(true).compact();
    let log_dir = match chronimage::util::paths::logs_dir() {
        Ok(dir) => dir,
        Err(e) => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(console_layer)
                .try_init();
            tracing::warn!(error = %e, "file logging unavailable; using console only");
            return None;
        }
    };

    let file_appender = tracing_appender::rolling::daily(&log_dir, "chronimage.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .compact();

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .try_init();
    tracing::info!(log_dir = %log_dir.display(), "file logging enabled");
    Some(guard)
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
    // Load .env.local (and .env) early so downstream code reading via
    // std::env::var (e.g. CHRONIMAGE_GPHOTOS_CLIENT_SECRET) sees values
    // the dev put in the gitignored files. Silent failure if absent —
    // production installs don't ship these files.
    let _ = dotenvy::from_filename(".env.local");
    let _ = dotenvy::dotenv();

    let _log_guard = install_tracing();

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
            commands::frontend_log,
            commands::app_version,
            commands::current_channel,
            commands::import_dry_run,
            commands::start_import,
            commands::list_imports,
            commands::list_albums,
            commands::list_photos,
            commands::list_sources,
            commands::create_source,
            commands::check_source_overlap,
            commands::delete_source,
            commands::on_this_day,
            commands::unseen_photos,
            commands::cleanup_dry_run,
            commands::refresh_smart_albums,
            commands::import_google_takeout,
            commands::detect_icloud_path,
            commands::list_iphone_devices,
            commands::gphotos_begin_oauth_flow,
            commands::gphotos_poll_oauth_flow,
            commands::gphotos_cancel_oauth_flow,
            commands::gphotos_auth_status,
            commands::gphotos_sign_out,
            commands::gphotos_account_info,
            commands::gphotos_ensure_source_row,
            commands::gphotos_manual_cleanup_instructions,
            commands::gphotos_create_picker_session,
            commands::gphotos_poll_picker_session,
            commands::gphotos_delete_picker_session,
            commands::import_google_photos,
            commands::gphotos_upload_scope_ok,
            commands::gphotos_upload,
            commands::onedrive_begin_oauth_flow,
            commands::onedrive_poll_oauth_flow,
            commands::onedrive_cancel_oauth_flow,
            commands::onedrive_auth_status,
            commands::onedrive_sign_out,
            commands::onedrive_account_info,
            commands::onedrive_upload,
            #[cfg(debug_assertions)]
            commands::__test_generate_fixture,
            #[cfg(debug_assertions)]
            commands::__test_seed_source_copies,
            #[cfg(debug_assertions)]
            commands::__test_seed_dated_photos,
            #[cfg(debug_assertions)]
            commands::__test_seed_embeddings,
            commands::detect_hardware,
            commands::embed_image,
            commands::score_aesthetic,
            commands::download_models,
            commands::find_duplicates,
            commands::search_photos,
            commands::search_suggestions,
            commands::get_thumbnail,
            commands::list_tags,
            commands::generate_ai_tags,
            commands::photo_quality,
            commands::photo_location,
            commands::list_photos_for_cluster,
            commands::first_time_on_new_camera,
            commands::unflagged_favorites,
            commands::ai_models_status,
            commands::cleanup_execute,
            commands::lift_shift_dry_run,
            commands::lift_shift_execute,
            commands::face_clusters_list,
            commands::face_cluster_name,
            commands::face_cluster_merge,
            commands::list_faces_for_photo,
            commands::face_assign_cluster,
            commands::face_create_person_from_face,
            commands::face_unassign,
            commands::record_photo_view,
            commands::ai_reindex,
            commands::get_default_catalog_path,
            commands::get_disk_info,
            commands::source_deletion_preview,
            commands::remove_photos_preview,
            commands::remove_photos_from_catalog,
            commands::recycle_source_copies,
            commands::recycle_source_files_after_copy,
            commands::recluster_faces,
            commands::rebuild_thumbnails,
            commands::cull_apply_verdict,
            commands::rate_photo,
            commands::flag_photo,
            commands::cull_bin_list,
            commands::cull_bin_summary,
            commands::cull_bin_restore,
            commands::cull_bin_delete_forever,
            commands::cull_bin_sweep,
            commands::export_enqueue,
            commands::export_run_next,
            commands::export_list_jobs,
            commands::list_user_tags,
            commands::add_user_tag,
            commands::remove_user_tag,
            commands::rename_user_tag,
            commands::develop_open,
            commands::develop_apply,
            commands::develop_save,
            commands::develop_snapshot_save,
            commands::develop_history_list,
            commands::develop_reset,
            commands::develop_copy_edits,
            commands::develop_paste_edits,
            commands::develop_preset_apply,
            commands::develop_adaptive_preset_apply,
            commands::presets_list,
            commands::preset_save,
            commands::develop_masks_list,
            commands::develop_mask_create,
            commands::develop_mask_generate,
            commands::develop_mask_update,
            commands::develop_mask_delete,
            commands::develop_mask_apply_preview,
            commands::ai_edit_status,
            commands::ai_edit_refresh,
            commands::merge_job_create,
            commands::merge_jobs_list,
            commands::tether_source_add,
            commands::tether_sources_list,
            commands::map_recompute_trips,
            commands::map_list_trips,
            commands::map_photos_in_trip,
            commands::xmp_rescan,
            commands::xmp_write_on_change_get,
            commands::xmp_write_on_change_set,
            commands::xmp_export_all,
            commands::license_load,
            commands::license_import,
            commands::license_clear,
            commands::telemetry_get,
            commands::telemetry_opt_in,
            commands::prompt_sidecar_get,
            commands::prompt_sidecar_set,
            commands::prompt_sidecar_model_get,
            commands::prompt_sidecar_model_set,
            commands::prompt_sidecar_ping,
            commands::prompt_edit,
            commands::mask_from_prompt,
            commands::prompt_edit_list,
            commands::prompt_edit_accept,
            commands::prompt_edit_reject,
            commands::backfill_place_labels,
            commands::map_tile,
            commands::geonames_status,
            commands::prompt_sidecar_command_get,
            commands::prompt_sidecar_command_set,
            commands::prompt_sidecar_proc_start,
            commands::prompt_sidecar_proc_stop,
            commands::prompt_sidecar_proc_status,
            commands::shortcuts_list,
            commands::shortcuts_set,
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
                if let Err(e) = chronimage::develop::presets::seed_builtins(&pool).await {
                    tracing::warn!(error = %e, "develop preset seed failed (non-fatal)");
                }
            });

            // Spawn the Cull Bin daily sweep (fires once at boot + every 24 h).
            // Best-effort — failures log but don't crash the app.
            #[cfg(not(test))]
            {
                let sweep_pool = pool.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        if let Err(e) = chronimage::cull::bin::sweep_expired(&sweep_pool).await {
                            tracing::warn!(error = %e, "cull_bin sweep failed");
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24)).await;
                    }
                });
            }

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

            app.manage(AppState {
                pool,
                sidecar_proc: chronimage::prompt::supervisor::Supervisor::new(),
                develop_decode_cache: std::sync::Arc::new(
                    chronimage::develop::DevelopDecodeCache::new(),
                ),
            });

            // Memoise the SCRFD + ArcFace `FacesSession` for pipeline stage-5
            // (see docs/prds/phase-1.md §5). This runs in a background blocking
            // task so setup() returns immediately — ort loads ~190 MB through
            // `commit_from_file` on first use, which we don't want on the
            // webview critical path.
            //
            // Resolution order matches ADR 0003: bundled installer dir first,
            // user data dir second. Missing-in-both → session init returns
            // `None` and stage-5 becomes a no-op for this process.
            let handle_for_faces = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let bundled = bundled_models_dir(&handle_for_faces);
                let user = models_dir().ok();
                let resolve = |filename: &str| -> Option<std::path::PathBuf> {
                    if let Some(b) = &bundled {
                        let p = b.join(filename);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                    if let Some(u) = &user {
                        let p = u.join(filename);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                    None
                };
                let scrfd = resolve("det_10g.onnx");
                let arcface = resolve("w600k_r50.onnx");
                init_global_faces_session(scrfd.as_deref(), arcface.as_deref());

                // Memoise SigLIP-2 (image + text + tokenizer) for search_photos
                // and pipeline stage-4. Resolution order matches ADR 0003.
                let siglip_image = resolve("siglip2-b16-image.onnx");
                let siglip_text = resolve("siglip2-b16-text.onnx");
                let siglip_tok = resolve("siglip2-b16-tokenizer.json");
                init_global_siglip_session(
                    siglip_image.as_deref(),
                    siglip_text.as_deref(),
                    siglip_tok.as_deref(),
                );

                // Memoise the default SAM2.1 encoder + decoder for Develop
                // masks. SAM3 remains an optional Settings install.
                let sam_enc = resolve("sam2.1_hiera_large.encoder.onnx");
                let sam_dec = resolve("sam2.1_hiera_large.decoder.onnx");
                init_global_sam_session(sam_enc.as_deref(), sam_dec.as_deref());
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .inspect_err(|e| tracing::error!(error = %e, "tauri runtime exited with error"))
        .ok();
}
