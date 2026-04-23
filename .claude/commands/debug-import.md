---
description: Drive the real import pipeline against a folder + query Loki for stage-by-stage timing. Use to diagnose "import is slow" complaints.
argument-hint: <path-to-photos-directory>
---

# debug-import — measure real-world import pipeline performance

Runs the full Rust import pipeline (Stage 1 scan → Stage 2 hash/EXIF/thumb → Stage 3 pairs → Stage 4 NIMA+SigLIP on GPU → Stage 5 RetinaFace+ArcFace on GPU) against `$ARGUMENTS` in a throwaway DB, with the real bundled ONNX models loaded from `%LOCALAPPDATA%\app.chronimage.desktop\models`. No user data is touched.

Unlike the Playwright e2e specs under `tests/e2e/` (which require `tauri-driver` + a packaged debug binary), this path is a plain `cargo test` — works on any dev machine with the app's models installed.

## Precondition

**Stop `pnpm tauri dev` first.** The running app holds `target\debug\chronimage-app.exe` open and `cargo test` cannot re-link over it. After the test finishes you can relaunch dev.

## How to run

```bash
# Bash / Git Bash
CHRONIMAGE_DEBUG_IMPORT_SRC="$ARGUMENTS" \
  cargo test --manifest-path src-tauri/Cargo.toml --test debug_import \
    -- --ignored --nocapture debug_import_timing
```

```powershell
# PowerShell
$env:CHRONIMAGE_DEBUG_IMPORT_SRC = "$ARGUMENTS"
cargo test --manifest-path src-tauri/Cargo.toml --test debug_import `
    -- --ignored --nocapture debug_import_timing
```

## What to report

Parse the stdout summary (final "debug-import result" block) and also query Loki for the stage timings. Loki runs at `http://localhost:3101` in dev; the test tags every log line with `run=debug-import` so you can isolate it from the main app's traffic.

Useful LogQL queries (feed via `curl` to `/loki/api/v1/query_range`):

- **Stage summary:** `{app="chronimage",run="debug-import"} |= "import pipeline:"`
- **Stage 2 per-photo:** `{app="chronimage",run="debug-import"} |= "stage-2 per-photo timing"`
- **Stage 4 per-photo:** `{app="chronimage",run="debug-import"} |= "stage-4 per-photo timing"`
- **Error hunt:** `{app="chronimage",run="debug-import",level=~"warn|error"}`

After the run, summarise in this format (fill in from the log data):

```
Stage 1 (scan)        N ms
Stage 2 (hash+EXIF+thumb)   N ms total / N ms per photo
  └─ hash_ms P50/P95   N / N
  └─ meta_ms P50/P95   N / N     (pHash dominates; uses rawler for RAW)
  └─ thumb_ms P50/P95  N / N
Stage 3 (pairs)       N ms
Stage 4 (NIMA+SigLIP) N ms total / N ms per photo
  └─ nima_ms P50/P95   N / N
  └─ siglip_ms P50/P95 N / N
Stage 5 (faces)       N ms total / N ms per photo

Bottleneck: <which sub-stage dominates>
Likely cause: <e.g. OneDrive hydration, CPU fallback, cold model load>
```

Flag anything notable:
- `siglip_ms > 1000` on a machine where `providers.rs` claims DirectML EP → GPU isn't actually being used.
- `meta_ms > 500` on JPGs → pHash is re-decoding unnecessarily.
- `hash_ms > 500` on < 50 MB files → source disk is slow (OneDrive placeholder?).
- `photos_count < new_photo_count` in the final summary → some photos are being dropped silently.

## What this skill does NOT do

- It doesn't spin up a Tauri webview or Playwright browser. The pipeline is driven directly from a tokio integration test. If you need full UI automation (click "Add a folder", dismiss dialogs), that's the `tauri-driver`-based path and is separate.
- It doesn't use the user's production catalog. Every run is against a fresh tempdir DB; photos aren't persisted between runs.

## Source

- Test driver: `src-tauri/tests/debug_import.rs`
- Pipeline under test: `src-tauri/src/import/pipeline.rs`
- Timing logs: look for `import pipeline: ...` (stage-level, INFO) and `stage-2 per-photo timing` / `stage-4 per-photo timing` (per-photo, DEBUG).
