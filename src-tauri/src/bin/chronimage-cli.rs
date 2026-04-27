//! Chronimage CLI — developer + power-user entry point.
//!
//! Every subcommand is a thin adapter over the `chronimage` library
//! crate: the same migrations, the same import pipeline, the same
//! license verifier that the Tauri shell uses. This file does not
//! re-implement business logic — it just parses argv, opens the pool,
//! and prints results in a human-friendly shape.
//!
//! Subcommands:
//! - `chronimage doctor`                           — environment + catalog report
//! - `chronimage migrate`                          — apply pending SQLite migrations
//! - `chronimage scan <path>`                      — preview a scan (dry-run, no writes)
//! - `chronimage import --source <name> <path>`    — run the real import pipeline
//! - `chronimage ai audit`                         — inventory every AI model
//! - `chronimage catalog stats`                    — photo / tag / face counts + disk
//! - `chronimage license show|import|clear`        — manage the Insider license
//! - `chronimage trips recompute`                  — recompute GPS trip clusters
//! - `chronimage xmp rescan`                       — re-scan for `.xmp` sidecars

use chronimage::{
    ai::{
        budget,
        download::{ModelSpec, KNOWN_MODELS},
    },
    catalog::db::{open_pool, PoolOptions},
    import::{
        pipeline::{run_pipeline_headless, ImportProgress, ImportResult},
        scan_dir, ScanOptions,
    },
    license,
    map::{geocode, trips},
    util::paths::{catalog_db_path, models_dir, thumbnails_dir},
};
use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Debug, Parser)]
#[command(
    name = "chronimage",
    version,
    about = "Chronimage CLI — power-user + ops tooling",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Print environment + catalog + model inventory.
    Doctor,

    /// Apply pending SQLite migrations to the user catalog.
    Migrate,

    /// Dry-run scan a directory; reports counts by extension + RAW+JPG pair count.
    Scan {
        /// Root directory to walk.
        path: PathBuf,
        /// Limit the output table to the top N extensions.
        #[arg(long, default_value_t = 20)]
        top: usize,
    },

    /// Run the real import pipeline against a directory. Writes to the
    /// catalog, computes thumbnails, extracts EXIF + AI signals.
    Import {
        /// Root directory to import.
        path: PathBuf,
        /// Human-readable name for the source row. Created if missing.
        #[arg(long)]
        source: String,
        /// Source kind tag — `local`, `nas`, `card`, etc.
        #[arg(long, default_value = "local")]
        kind: String,
    },

    /// AI model operations.
    Ai {
        #[command(subcommand)]
        sub: AiCmd,
    },

    /// Catalog queries.
    Catalog {
        #[command(subcommand)]
        sub: CatalogCmd,
    },

    /// License state (Phase 5 §7).
    License {
        #[command(subcommand)]
        sub: LicenseCmd,
    },

    /// Map + geocoder operations.
    Trips {
        #[command(subcommand)]
        sub: TripsCmd,
    },

    /// XMP sidecar operations (Phase 4 §7).
    Xmp {
        #[command(subcommand)]
        sub: XmpCmd,
    },
}

#[derive(Debug, Subcommand)]
enum AiCmd {
    /// List every known AI model + its install state on disk.
    Audit,
}

#[derive(Debug, Subcommand)]
enum CatalogCmd {
    /// Photo / tag / face counts + disk sizes.
    Stats,
}

#[derive(Debug, Subcommand)]
enum LicenseCmd {
    /// Print the current `license_state` row.
    Show,
    /// Import a signed `license.json`.
    Import {
        /// Path to the license file.
        path: PathBuf,
    },
    /// Revert to the `community` plan.
    Clear,
}

#[derive(Debug, Subcommand)]
enum TripsCmd {
    /// Recompute GPS trip clusters from scratch.
    Recompute,
    /// Print the recent trips (newest first).
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Debug, Subcommand)]
enum XmpCmd {
    /// Re-scan every photo for an adjacent `.xmp` sidecar.
    Rescan,
}

// ── entry point ───────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()?;

    rt.block_on(async move {
        match cli.cmd {
            Cmd::Doctor => run_doctor().await,
            Cmd::Migrate => run_migrate().await,
            Cmd::Scan { path, top } => run_scan(path, top),
            Cmd::Import { path, source, kind } => run_import(path, source, kind).await,
            Cmd::Ai { sub } => match sub {
                AiCmd::Audit => run_ai_audit().await,
            },
            Cmd::Catalog { sub } => match sub {
                CatalogCmd::Stats => run_catalog_stats().await,
            },
            Cmd::License { sub } => run_license(sub).await,
            Cmd::Trips { sub } => run_trips(sub).await,
            Cmd::Xmp { sub } => match sub {
                XmpCmd::Rescan => run_xmp_rescan().await,
            },
        }
    })
}

// ── helpers ───────────────────────────────────────────────────────────────

async fn with_pool<F, Fut, T>(f: F) -> Result<T, Box<dyn std::error::Error>>
where
    F: FnOnce(SqlitePool) -> Fut,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>>,
{
    let db_path = catalog_db_path()?;
    let pool = open_pool(PoolOptions::new(db_path)).await?;
    let res = f(pool.clone()).await;
    pool.close().await;
    res
}

fn fmt_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut idx = 0usize;
    let mut v = bytes as f64;
    while v >= 1024.0 && idx + 1 < UNITS.len() {
        v /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[idx])
    }
}

fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

// ── doctor ────────────────────────────────────────────────────────────────

async fn run_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("Chronimage {}", env!("CARGO_PKG_VERSION"));
    println!(
        "  os       {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    // Hardware.
    let hw = budget::detect();
    println!("  tier     {:?}", hw.tier);
    println!("  adapter  {}", hw.adapter_name);
    println!("  vram     {} MB", hw.vram_mb);

    // Paths + disk usage.
    let db_path = catalog_db_path()?;
    let thumbs = thumbnails_dir().ok();
    let models = models_dir().ok();
    println!();
    println!("Paths");
    println!("  catalog  {}", db_path.display());
    if let Some(t) = thumbs.as_ref() {
        println!("  thumbs   {}  ({})", t.display(), fmt_bytes(dir_size(t)));
    }
    if let Some(m) = models.as_ref() {
        println!("  models   {}  ({})", m.display(), fmt_bytes(dir_size(m)));
    }

    // Catalog stats (best-effort — if the DB isn't initialised yet, skip).
    if db_path.exists() {
        let pool = open_pool(PoolOptions::new(db_path.clone())).await?;
        let (photos, tags, faces, trips_n): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
                (SELECT COUNT(*) FROM photos), \
                (SELECT COUNT(*) FROM tags), \
                (SELECT COUNT(*) FROM faces), \
                (SELECT COUNT(*) FROM trips)",
        )
        .fetch_one(&pool)
        .await?;
        let (migration_count, latest_migration): (i64, Option<i64>) =
            sqlx::query_as("SELECT COUNT(*), MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&pool)
                .await?;
        let license = license::load(&pool).await?;
        pool.close().await;
        println!();
        println!("Catalog");
        println!(
            "  schema   {} migrations, latest {}",
            migration_count,
            latest_migration
                .map(|version| version.to_string())
                .unwrap_or_else(|| "(none)".to_string())
        );
        println!("  photos   {photos}");
        println!("  tags     {tags}");
        println!("  faces    {faces}");
        println!("  trips    {trips_n}");
        println!();
        println!("License");
        println!("  plan     {}", license.plan);
        if let Some(e) = license.email.as_deref() {
            println!("  email    {e}");
        }
        if let Some(x) = license.expires_at.as_deref() {
            println!("  expires  {x}");
        }
    } else {
        println!();
        println!("Catalog    (not initialised — run `chronimage migrate`)");
    }

    // Model count (fast — just file existence checks).
    if let Some(m) = models.as_ref() {
        let mut installed = 0usize;
        for spec in KNOWN_MODELS {
            if m.join(spec.filename).exists() {
                installed += 1;
            }
        }
        println!();
        println!("Models     {installed} / {} installed", KNOWN_MODELS.len());
    }

    Ok(())
}

// ── migrate ───────────────────────────────────────────────────────────────

async fn run_migrate() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = catalog_db_path()?;
    println!("Migrating {}", db_path.display());
    let pool = open_pool(PoolOptions::new(db_path)).await?;
    let (migration_count, latest_migration): (i64, Option<i64>) =
        sqlx::query_as("SELECT COUNT(*), MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&pool)
            .await?;
    pool.close().await;
    println!(
        "  schema now {} migrations, latest {}",
        migration_count,
        latest_migration
            .map(|version| version.to_string())
            .unwrap_or_else(|| "(none)".to_string())
    );
    Ok(())
}

// ── scan (dry-run) ────────────────────────────────────────────────────────

fn run_scan(path: PathBuf, top: usize) -> Result<(), Box<dyn std::error::Error>> {
    let t0 = Instant::now();
    let entries = scan_dir(&ScanOptions::new(path.clone()))?;
    let elapsed = t0.elapsed();

    let total_size: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let mut by_ext: std::collections::BTreeMap<String, (usize, u64)> = Default::default();
    for e in &entries {
        let slot = by_ext.entry(e.ext_lowercase.clone()).or_default();
        slot.0 += 1;
        slot.1 += e.size_bytes;
    }

    // Rough RAW+JPG pairing: shared stem between files whose extensions
    // are known RAWs vs `jpg`/`jpeg`.
    let mut stems: std::collections::HashMap<String, (bool, bool)> = Default::default();
    for e in &entries {
        let stem = e
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned().to_lowercase())
            .unwrap_or_default();
        let slot = stems.entry(stem).or_insert((false, false));
        if chronimage::import::is_raw_extension(&e.ext_lowercase) {
            slot.0 = true;
        } else if matches!(e.ext_lowercase.as_str(), "jpg" | "jpeg") {
            slot.1 = true;
        }
    }
    let raw_jpg_pairs = stems.values().filter(|(r, j)| *r && *j).count();

    println!("Scan {}", path.display());
    println!("  total    {}  ({})", entries.len(), fmt_bytes(total_size));
    println!("  pairs    {raw_jpg_pairs} RAW+JPG pairs");
    println!("  elapsed  {:.2}s", elapsed.as_secs_f64());
    println!();
    println!("By extension (top {top})");
    let mut rows: Vec<(&String, &(usize, u64))> = by_ext.iter().collect();
    rows.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for (ext, (count, size)) in rows.into_iter().take(top) {
        println!("  {:>6}  {:>6}  {}", ext, count, fmt_bytes(*size));
    }

    Ok(())
}

// ── import (real) ─────────────────────────────────────────────────────────

async fn get_or_create_source_id(
    pool: &SqlitePool,
    name: &str,
    kind: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE name = ?1 AND kind = ?2")
            .bind(name)
            .bind(kind)
            .fetch_optional(pool)
            .await?
    {
        return Ok(id);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, created_at) VALUES (?1, ?2, 'idle', ?3) RETURNING id",
    )
    .bind(name)
    .bind(kind)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn run_import(
    path: PathBuf,
    source: String,
    kind: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = open_pool(PoolOptions::new(catalog_db_path()?)).await?;
    let source_id = get_or_create_source_id(&pool, &source, &kind).await?;
    println!(
        "Import {} (source '{source}' kind '{kind}', id={source_id})",
        path.display()
    );

    let on_progress: Arc<dyn Fn(ImportProgress) + Send + Sync> = Arc::new(|p: ImportProgress| {
        if p.total > 0 && p.done.is_multiple_of(25) {
            eprintln!(
                "  [{:>5}/{:<5}] {}",
                p.done,
                p.total,
                std::path::Path::new(&p.current_file)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
        }
    });

    let t0 = Instant::now();
    let ImportResult {
        import_id,
        imported_count,
        skipped_count,
        error_count,
    } = run_pipeline_headless(source_id, path, pool.clone(), on_progress).await?;
    let elapsed = t0.elapsed();

    pool.close().await;
    println!();
    println!(
        "Done (import_id={import_id}) in {:.2}s",
        elapsed.as_secs_f64()
    );
    println!("  imported {imported_count}");
    println!("  skipped  {skipped_count}");
    println!("  errors   {error_count}");
    Ok(())
}

// ── ai audit ──────────────────────────────────────────────────────────────

async fn run_ai_audit() -> Result<(), Box<dyn std::error::Error>> {
    let models_root = models_dir()?;
    println!("AI model inventory  (root: {})", models_root.display());
    println!();
    println!("  {:<30} {:<18} {:>9}   state", "name", "kind", "size");
    for spec in KNOWN_MODELS {
        let state = model_state(&models_root, spec);
        let size = models_root
            .join(spec.filename)
            .metadata()
            .ok()
            .map(|m| fmt_bytes(m.len()))
            .unwrap_or_else(|| "-".into());
        println!(
            "  {:<30} {:<18} {:>9}   {}",
            spec.name, spec.kind, size, state
        );
    }
    Ok(())
}

fn model_state(root: &Path, spec: &ModelSpec) -> &'static str {
    if root.join(spec.filename).exists() {
        if spec.bundled {
            "bundled"
        } else {
            "installed"
        }
    } else if spec.bundled {
        "missing (bundled)"
    } else {
        "not installed"
    }
}

// ── catalog stats ─────────────────────────────────────────────────────────

async fn run_catalog_stats() -> Result<(), Box<dyn std::error::Error>> {
    with_pool(|pool| async move {
        let (photos, raw_photos, tags, user_tags, faces, clusters, trips_n, edits): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            "SELECT \
                (SELECT COUNT(*) FROM photos), \
                (SELECT COUNT(*) FROM photos WHERE is_raw = 1), \
                (SELECT COUNT(*) FROM tags), \
                (SELECT COUNT(*) FROM tags WHERE kind = 'user'), \
                (SELECT COUNT(*) FROM faces), \
                (SELECT COUNT(*) FROM clusters), \
                (SELECT COUNT(*) FROM trips), \
                (SELECT COUNT(*) FROM edits)",
        )
        .fetch_one(&pool)
        .await?;
        let bytes: Option<i64> =
            sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes), 0) FROM photos")
                .fetch_one(&pool)
                .await
                .ok();

        println!("Catalog stats");
        println!("  photos       {photos}  ({raw_photos} RAW)");
        println!(
            "  bytes        {}",
            fmt_bytes(bytes.unwrap_or(0).max(0) as u64)
        );
        println!("  tags         {tags}  ({user_tags} user-authored)");
        println!("  faces        {faces}  ({clusters} clusters)");
        println!("  trips        {trips_n}");
        println!("  edits        {edits}");
        Ok(())
    })
    .await
}

// ── license ───────────────────────────────────────────────────────────────

async fn run_license(sub: LicenseCmd) -> Result<(), Box<dyn std::error::Error>> {
    match sub {
        LicenseCmd::Show => {
            with_pool(|pool| async move {
                let s = license::load(&pool).await?;
                println!("plan        {}", s.plan);
                if let Some(e) = s.email.as_deref() {
                    println!("email       {e}");
                }
                if let Some(x) = s.issued_at.as_deref() {
                    println!("issued_at   {x}");
                }
                if let Some(x) = s.expires_at.as_deref() {
                    println!("expires_at  {x}");
                }
                if let Some(x) = s.verified_at.as_deref() {
                    println!("verified_at {x}");
                }
                println!("valid_insider {}", s.is_valid_insider());
                Ok(())
            })
            .await
        }
        LicenseCmd::Import { path } => {
            let raw = std::fs::read_to_string(&path)?;
            with_pool(|pool| async move {
                let s =
                    license::import_from_json(&pool, &raw, license::INSIDER_PUBKEY_BYTES).await?;
                println!("imported; plan is now {}", s.plan);
                Ok(())
            })
            .await
        }
        LicenseCmd::Clear => {
            with_pool(|pool| async move {
                license::clear(&pool).await?;
                println!("license cleared; plan is now community");
                Ok(())
            })
            .await
        }
    }
}

// ── trips ─────────────────────────────────────────────────────────────────

async fn run_trips(sub: TripsCmd) -> Result<(), Box<dyn std::error::Error>> {
    match sub {
        TripsCmd::Recompute => {
            with_pool(|pool| async move {
                let r = trips::recompute_trips(&pool).await?;
                println!(
                    "recomputed {} trips covering {} photos in {} ms",
                    r.trip_count, r.photo_count, r.elapsed_ms
                );
                Ok(())
            })
            .await
        }
        TripsCmd::List { limit } => {
            with_pool(|pool| async move {
                let rows = trips::list_trips(&pool).await?;
                let shown = rows.len().min(limit);
                println!(
                    "Trips (showing {shown} of {}, geocoder: {})",
                    rows.len(),
                    if geocode::extended_cities_available() {
                        format!("extended {} cities", geocode::extended_cities_count())
                    } else {
                        format!("bundled {} cities", geocode::CITIES.len())
                    }
                );
                for t in rows.into_iter().take(limit) {
                    println!(
                        "  {:>4}  {}  ..  {}   {:>5} photos   {}",
                        t.id,
                        &t.start_at[..10.min(t.start_at.len())],
                        &t.end_at[..10.min(t.end_at.len())],
                        t.photo_count,
                        t.name.as_deref().unwrap_or("(no label)")
                    );
                }
                Ok(())
            })
            .await
        }
    }
}

// ── xmp ───────────────────────────────────────────────────────────────────

async fn run_xmp_rescan() -> Result<(), Box<dyn std::error::Error>> {
    with_pool(|pool| async move {
        let r = chronimage::xmp::rescan_all(&pool).await?;
        println!(
            "xmp rescan: scanned {}  applied {}  errors {}",
            r.scanned, r.applied, r.error_count
        );
        for e in r.errors.iter().take(10) {
            eprintln!("  warn: {e}");
        }
        Ok(())
    })
    .await
}
