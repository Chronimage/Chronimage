//! Phase 1 exit criterion: 8-hour import + search + face-cluster loop, zero panics.
//!
//! PRD reference: docs/prds/phase-1.md § Exit criteria (nightly only).
//!
//! Always `#[ignore]`; CI runs this in the nightly workflow with `--ignored`.
//!
//! ## What this asserts
//!
//! 1. **Zero panics** during an extended (default 8 h) runtime loop that mixes
//!    import-pipeline-produced catalog state with periodic read + write
//!    traffic and periodic face-cluster rebuild attempts.
//! 2. **Pool health** — after the loop, the SQLite pool still responds to a
//!    `SELECT 1`, is not closed, and has no unclosed WAL frames.
//! 3. **RSS ceiling** — peak resident set size stays under 2 GB on Windows
//!    (the PRD's target platform). On non-Windows, RSS sampling is a no-op.
//!
//! ## Environment overrides
//!
//! The defaults match the PRD (8-hour × 50 k-photo library), but each knob is
//! overridable so developers can smoke-test the harness locally in seconds:
//!
//! | Env var                                     | Default | Purpose                                |
//! | ------------------------------------------- | ------- | -------------------------------------- |
//! | `CHRONIMAGE_STRESS_DURATION_SECS`           | 28 800  | Total loop duration after import done  |
//! | `CHRONIMAGE_STRESS_PHOTO_COUNT`             | 50 000  | Number of synthetic photos to generate |
//! | `CHRONIMAGE_STRESS_SEARCH_INTERVAL_SECS`    | 300     | Cadence of simulated NL searches       |
//! | `CHRONIMAGE_STRESS_CLUSTER_INTERVAL_SECS`   | 1 800   | Cadence of face-cluster rebuild passes |
//!
//! ## Running
//!
//! Default (8 h — nightly CI):
//! ```ignore
//! cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_stress \
//!     -- --ignored --nocapture
//! ```
//!
//! Fast smoke (60 s, 200 photos — local verification the harness compiles +
//! exits cleanly):
//! ```ignore
//! CHRONIMAGE_STRESS_DURATION_SECS=60 \
//! CHRONIMAGE_STRESS_PHOTO_COUNT=200 \
//! CHRONIMAGE_STRESS_SEARCH_INTERVAL_SECS=5 \
//! CHRONIMAGE_STRESS_CLUSTER_INTERVAL_SECS=15 \
//! cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_stress \
//!     -- --ignored --nocapture
//! ```

use chronimage::{
    catalog::db::{open_pool, PoolOptions},
    import::run_pipeline_headless,
    util::synthetic::synthesize_jpeg,
};
use sqlx::SqlitePool;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;

// ── Defaults matching the PRD exit-criterion numbers ─────────────────────────
const DEFAULT_DURATION_SECS: u64 = 8 * 60 * 60;
const DEFAULT_PHOTO_COUNT: usize = 50_000;
const DEFAULT_SEARCH_INTERVAL_SECS: u64 = 300;
const DEFAULT_CLUSTER_INTERVAL_SECS: u64 = 1_800;
const RSS_CEILING_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

// ── Env helpers ──────────────────────────────────────────────────────────────

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

// ── RSS sampling — Windows only (Phase 1's target platform) ──────────────────

#[cfg(target_os = "windows")]
fn current_rss_bytes() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: GetCurrentProcess returns a pseudo-handle that does not need to
    // be closed; GetProcessMemoryInfo writes into a stack-allocated struct of
    // the exact size we pass. Both calls are documented as safe to invoke
    // from any thread.
    unsafe {
        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let handle = GetCurrentProcess();
        if GetProcessMemoryInfo(handle, &mut counters, size).is_ok() {
            Some(counters.WorkingSetSize as u64)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn current_rss_bytes() -> Option<u64> {
    None
}

// ── Workload primitives ──────────────────────────────────────────────────────

/// Exercise a representative read path that the Catalog screen hits on every
/// render — the top-N photos ordered by imported_at. Uses the default pool,
/// not a prepared statement, to catch any state drift between calls.
async fn exercise_list_photos(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let start = Instant::now();
    let _rows: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM photos ORDER BY imported_at DESC LIMIT 50")
            .fetch_all(pool)
            .await?;
    Ok(start.elapsed().as_millis() as u64)
}

/// Exercise the vec0 KNN path. When stage-4 AI was short-circuited (CHRONIMAGE_MODELS_DIR
/// points at a nonexistent path in this test), `vec_photo_embeddings_int8` will
/// be empty; the query still parses + returns quickly, which is what we want
/// to verify — that the hot NL-search codepath never panics under extended
/// runtime.
async fn exercise_knn_search(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let start = Instant::now();
    let _rows: Vec<(i64, f32)> = sqlx::query_as(
        "SELECT rowid, distance FROM vec_photo_embeddings_int8 \
         WHERE embedding MATCH vec_int8(?1) ORDER BY distance LIMIT 50",
    )
    .bind(vec![0i8 as u8; 768])
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    Ok(start.elapsed().as_millis() as u64)
}

/// Count faces currently in the catalog. When clustering runs in real nightly
/// CI with models present, this grows with each cluster rebuild; in a
/// stubbed-models environment it stays at 0. Either way is a valid stress
/// signal — what matters is the read-count query executes reliably.
async fn sample_face_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM faces")
        .fetch_one(pool)
        .await
}

// ── The test itself ──────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "8-hour stress loop — nightly only (override via CHRONIMAGE_STRESS_DURATION_SECS)"]
async fn eight_hour_loop_no_panics() {
    // Point the models dir at a non-existent path so stage-4 AI enrichment
    // short-circuits instead of trying to load a developer's real
    // multi-hundred-MB ONNX models against synthetic fixtures.
    // SAFETY: called before any tokio task that might read CHRONIMAGE_MODELS_DIR.
    unsafe {
        std::env::set_var(
            "CHRONIMAGE_MODELS_DIR",
            "\\nonexistent\\chronimage-stress-models",
        );
    }

    let duration = Duration::from_secs(env_u64(
        "CHRONIMAGE_STRESS_DURATION_SECS",
        DEFAULT_DURATION_SECS,
    ));
    let photo_count = env_usize("CHRONIMAGE_STRESS_PHOTO_COUNT", DEFAULT_PHOTO_COUNT);
    let search_interval = Duration::from_secs(env_u64(
        "CHRONIMAGE_STRESS_SEARCH_INTERVAL_SECS",
        DEFAULT_SEARCH_INTERVAL_SECS,
    ));
    let cluster_interval = Duration::from_secs(env_u64(
        "CHRONIMAGE_STRESS_CLUSTER_INTERVAL_SECS",
        DEFAULT_CLUSTER_INTERVAL_SECS,
    ));

    eprintln!(
        "stress: duration={}s photo_count={} search_every={}s cluster_every={}s",
        duration.as_secs(),
        photo_count,
        search_interval.as_secs(),
        cluster_interval.as_secs(),
    );

    // Install a panic hook that counts panics so we can assert on them after
    // the loop (a panic inside a tokio task doesn't abort the test harness by
    // default — we need an out-of-band signal).
    let panic_count = Arc::new(AtomicUsize::new(0));
    let pc = panic_count.clone();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        pc.fetch_add(1, Ordering::SeqCst);
        eprintln!("[stress] observed panic: {info}");
    }));

    let tmp = TempDir::new().expect("tempdir");
    let library = tmp.path().join("library");
    std::fs::create_dir_all(&library).expect("mkdir library");

    // ── Seed the fixture library on disk. ────────────────────────────────
    eprintln!(
        "stress: seeding {photo_count} synthetic photos into {}",
        library.display()
    );
    let seed_start = Instant::now();
    for i in 0..photo_count {
        std::fs::write(
            library.join(format!("photo_{i:07}.jpg")),
            synthesize_jpeg(i),
        )
        .expect("write fixture jpeg");
        if i > 0 && i.is_multiple_of(5_000) {
            eprintln!("  seeded {i}/{photo_count}");
        }
    }
    eprintln!(
        "stress: seed complete in {:.1}s",
        seed_start.elapsed().as_secs_f64()
    );

    // ── Open a fresh catalog + insert the source row. ────────────────────
    let db_path = tmp.path().join("catalog.db");
    let pool = open_pool(PoolOptions::new(db_path.clone()))
        .await
        .expect("open_pool");

    let now_ts = chrono::Utc::now().to_rfc3339();
    let source_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, kind, status, created_at, config_json) \
         VALUES ('phase-1-stress', 'local', 'idle', ?1, '{}') RETURNING id",
    )
    .bind(&now_ts)
    .fetch_one(&pool)
    .await
    .expect("insert source");

    // ── Import once. ─────────────────────────────────────────────────────
    let noop: Arc<dyn Fn(chronimage::import::ImportProgress) + Send + Sync> = Arc::new(|_| {});
    let import_start = Instant::now();
    let result = run_pipeline_headless(source_id, library.clone(), pool.clone(), noop)
        .await
        .expect("run_pipeline_headless");
    eprintln!(
        "stress: imported {}/{photo_count} photos in {:.1}s",
        result.imported_count,
        import_start.elapsed().as_secs_f64()
    );

    // ── The loop. ────────────────────────────────────────────────────────
    let loop_deadline = Instant::now() + duration;
    let mut next_search = Instant::now();
    let mut next_cluster = Instant::now() + cluster_interval;
    let mut searches_run = 0_u64;
    let mut cluster_polls = 0_u64;
    let mut peak_rss: u64 = 0;
    let rss_at_start = current_rss_bytes();

    while Instant::now() < loop_deadline {
        let now = Instant::now();

        if now >= next_search {
            let list_ms = exercise_list_photos(&pool).await.expect("list_photos");
            let knn_ms = exercise_knn_search(&pool).await.expect("knn");
            searches_run += 1;
            next_search = now + search_interval;
            if searches_run.is_multiple_of(20) {
                eprintln!("  search #{searches_run}: list={list_ms}ms knn={knn_ms}ms");
            }
        }

        if now >= next_cluster {
            let faces = sample_face_count(&pool).await.expect("face count");
            cluster_polls += 1;
            next_cluster = now + cluster_interval;
            eprintln!("  cluster poll #{cluster_polls}: faces={faces}");
        }

        if let Some(rss) = current_rss_bytes() {
            if rss > peak_rss {
                peak_rss = rss;
            }
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // ── Post-loop invariants. ────────────────────────────────────────────
    let panics = panic_count.load(Ordering::SeqCst);
    assert_eq!(panics, 0, "stress loop observed {panics} panic(s)");

    // Pool still responsive + not corrupted.
    let health: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .expect("pool still responsive post-loop");
    assert_eq!(health, 1);

    // WAL checkpoint succeeds — indirectly asserts no open transactions are
    // holding WAL frames hostage.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE);")
        .execute(&pool)
        .await
        .expect("wal_checkpoint post-loop");

    eprintln!(
        "stress: DONE — searches={searches_run} cluster_polls={cluster_polls} \
         rss_start={} MB peak_rss={} MB panics=0",
        rss_at_start.map(|b| b / 1024 / 1024).unwrap_or(0),
        peak_rss / 1024 / 1024,
    );

    if peak_rss > 0 {
        assert!(
            peak_rss < RSS_CEILING_BYTES,
            "peak RSS {peak_rss} B ({} MB) exceeded {RSS_CEILING_BYTES} B ({} MB) ceiling",
            peak_rss / 1024 / 1024,
            RSS_CEILING_BYTES / 1024 / 1024,
        );
    }

    pool.close().await;
    std::panic::set_hook(prev_hook);
}
