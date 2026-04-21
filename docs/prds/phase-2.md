# Phase 2 · Cull + Cull Bin + Export

> Turn the Phase 1 "flagged duplicates + out-of-focus + eyes-closed" smart albums into an actionable workflow: real cull verdicts, recoverable bin, 30-day retention, and a working Export sheet so finished photos can leave the catalog.

## Context

Phase 1 ships a deep catalog that *surfaces* problems (near-duplicates, blur, closed eyes) but doesn't let the user *act* on them. Phase 2 closes that loop: fast triage UIs (Compare / Grid / Swipe), keyboard-driven verdicts, a safety-first Cull Bin that keeps rejects recoverable for 30 days, and the Export sheet users need to actually ship a photo.

This is the phase where Chronimage starts beating Aftershoot / Narrative Select on workflow cost — same AI quality, zero subscription, recoverable deletes.

## Personas & stories

- **Jay (hobbyist, ~200k photos from Phase 1 import)**
  - As Jay, I can open the Cull queue and work through 300 AI-flagged near-duplicates in under 20 minutes using keyboard shortcuts.
  - As Jay, I can reject a photo and know it's recoverable for 30 days before any bytes are permanently deleted.
  - As Jay, I can export 40 selected photos as 4000px JPEG sRGB with stripped GPS + a watermark, and upload them to Google Photos in one dialog.
  - As Jay, when I reject a RAW+JPG pair, I can choose which of the two to drop (or both).

- **Priya (event photographer, 3070 GPU, 2 500-photo wedding shoot)**
  - As Priya, Swipe mode lets me triage on a laptop touchscreen; Compare gives me eye-clarity / focus numbers side by side; Grid lets me bulk-reject 40 low-aesthetic shots at once.
  - As Priya, I can Empty-Bin a specific client folder after delivery and free 40 GB without touching other rejects.

## Must-have deliverables

### 1. Cull screen (ported from `screens_cull.jsx`)
- [ ] Three modes wired to the design's `cullMode` tweak: **Compare** (pair side-by-side), **Grid** (4-col with inline issue chips), **Swipe** (single card, drag-to-verdict)
- [ ] Keyboard verdicts: `A` / `B` reject A or B, `↵` accept AI verdict, `⌃R` reject both, `Space` skip, `←` / `→` prev/next
- [ ] Filmstrip showing pair context (what's queued, what's already reviewed)
- [ ] Issue filters: Near-duplicates · Out of focus · Eyes closed · Over/under exposed · Screenshots · Low-res/web
- [ ] Session summary card: kept / rejected / time remaining / estimated minutes left
- [ ] "Review rejects before deleting" exit action that routes to Cull Bin

### 2. Cull verdict engine (Rust)
- [ ] `src-tauri/src/cull/verdict.rs` — `apply_verdict(photo_id, verdict)` where `verdict ∈ { Keep, RejectA, RejectB, RejectBoth, Skip }`
- [ ] Moves rejected photos to `cull_bin` table with `rejected_at`, `reason`, `source_copies_frozen_json` (captures each source_copies row for restore)
- [ ] RAW+JPG pair handling: RejectA/RejectB operate on the pair members; RejectBoth drops both
- [ ] Idempotent — re-applying a verdict is a no-op
- [ ] Progress event `chronimage.cull.progress` fires per verdict so the UI can animate

### 3. Cull Bin screen (ported from `screens_cullbin.jsx`)
- [ ] Ported with filters (All rejects / Near-dupes / Out of focus / Eyes closed / Screenshots)
- [ ] "Reclaimable" summary (size + count)
- [ ] Per-row `Restore` and `Delete` actions
- [ ] Multi-select: "Restore N to catalog" · "Delete N forever"
- [ ] Retention label: "Auto-empty after 30 days · nothing leaves your disk without confirmation"
- [ ] `Empty bin permanently` action with two-step confirmation

### 4. Cull Bin Rust commands
- [ ] `cull_bin_list(filter: CullFilter) -> Vec<CullBinRow>`
- [ ] `cull_bin_restore(photo_ids: Vec<i64>) -> RestoreReceipt` — moves rows back out of `cull_bin`, re-links `source_copies`
- [ ] `cull_bin_delete(photo_ids: Vec<i64>, confirm_token: String) -> DeleteReceipt` — actually removes the photo rows + thumbs + cached embeddings
- [ ] Background task: daily sweep that permanently deletes any `cull_bin` row older than `cull_bin.retention_days` (default 30, overridable in Settings)
- [ ] All deletions append to `source_deletions` log (already in Phase 1 schema)

### 5. Export sheet modal (ported from `export_sheet.jsx`)
- [ ] Opens from: Catalog selection toolbar, Catalog detail overlay, Develop (Phase 3+)
- [ ] Left pane: per-photo queue with status (queued / running / done) + progress bar + op description
- [ ] Right pane: export preset
  - Format: JPEG / HEIC / TIFF
  - Color: sRGB / P3 / AdobeRGB
  - Quality slider (0–100)
  - Long-edge slider (800–8000 px, step 200)
  - Strip GPS & metadata
  - Watermark (Phase 4 wires the image picker; Phase 2 ships a text watermark only)
  - Archive originals alongside export
  - Copy-paste edits from last developed photo (Phase 3+ — hidden until then)
  - Auto-light adjustments (Phase 3+ — hidden until then)
  - Upload to Google Photos / OneDrive (opt-in)
- [ ] GPU / ETA / Output size cards
- [ ] "Start N tasks" button

### 6. Export engine (Rust)
- [ ] `src-tauri/src/export/mod.rs` — tokio task pool (configurable concurrency, default = `num_cpus / 2`, GPU path optional)
- [ ] `src-tauri/src/export/jpeg.rs` — `image` crate → MozJPEG via `mozjpeg` crate for better file-size/quality trade-offs
- [ ] `src-tauri/src/export/heic.rs` — via `libheif-rs` (already Phase 1 dep)
- [ ] `src-tauri/src/export/tiff.rs` — via `image` crate
- [ ] Color profile handling (Phase 1 `raw/color.rs` module)
- [ ] Progress events per photo: `chronimage.export.progress { photo_id, percent, status, op }`
- [ ] Re-upload adapters:
  - `src-tauri/src/export/upload/google_photos.rs` — `mediaItems.batchCreate`
  - `src-tauri/src/export/upload/onedrive.rs` — Graph API `/me/drive/root:/Photos/...:/content`
- [ ] Pause/resume/cancel via a shared `ExportJob` handle

### 7. Aesthetic-based auto-ranking inside pair clusters
- [ ] When a burst contains N photos, NIMA score (from Phase 1 `ai/aesthetic.rs`) picks the AI suggested keeper
- [ ] Visible as a green ✓ badge on the AI pick; the loser gets an orange "Suggested reject" chip

### 8. Settings for Phase 2
- [ ] Duplicate similarity threshold slider (50–100%, default 85%)
- [ ] Sharpness cutoff slider (0–100, default 32)
- [ ] "Require final review before deleting" toggle (always-on in v1)
- [ ] Cull Bin retention days (default 30)

### 9. Vector search ANN upgrade (carries Phase 1 NFR follow-up)
- [ ] Upgrade sqlite-vec 0.1.9 → 0.1.10+ once upstream packaging stabilises (`build.rs` currently fails on a missing `sqlite-vec-diskann.c`, see commit `75a52bd`). diskann gives `O(log n)` ANN vs. the current brute-force `O(n)`.
- [ ] Phase 1 int8 brute-force measured at 607 ms p95 on 200k (threshold loosened to 750 ms in Phase 1). With diskann ANN + int8, target drops to ~30-100 ms p95 — real user-facing interactive latency.
- [ ] Fallback plan if sqlite-vec ANN remains delayed: swap to `hnsw-rs` crate + `vec_photo_embeddings_f32` for distance verification, keeping the int8 table as the scan fallback.
- [ ] Revisit `phase_1_search_latency.rs` threshold after the upgrade lands — should aim for PRD's original 500 ms with headroom.

## Non-goals

- No RAW develop (Phase 3)
- No prompt-edit (Phase 4)
- No automatic cull — user always triggers the workflow
- No delete-from-source (that's the separate **source cleanup** feature in Phase 1)
- No mobile companion for cull (future)

## Non-functional requirements

- Cull verdict round-trip (keypress → DB commit → next pair rendered) ≤ 150 ms p95
- Export throughput (JPEG @ 4000px long-edge, Q=88): ≥ 4 photos/s on CPU, ≥ 20 photos/s on GPU (3060+)
- Cull Bin `restore` + `delete` idempotent under concurrent clicks
- 30-day sweep never deletes a photo whose `cull_bin.retention_days` was just changed by the user (reads retention at sweep time, not at rejection time)

## Schema changes

New migration: `src-tauri/migrations/20260601000000_phase2_cull_export.sql`

```sql
-- Cull Bin — rejected photos pending permanent delete.
CREATE TABLE IF NOT EXISTS cull_bin (
  photo_id                   INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  rejected_at                TEXT NOT NULL,
  reason                     TEXT NOT NULL,        -- 'near-dup' | 'blur' | 'eyes-closed' | 'exposure' | 'user'
  source_copies_frozen_json  TEXT NOT NULL CHECK (json_valid(source_copies_frozen_json)),
  retention_days             INTEGER NOT NULL DEFAULT 30,
  permanent_delete_after     TEXT NOT NULL         -- denormalized for the nightly sweep
);

CREATE INDEX IF NOT EXISTS idx_cull_bin_permanent_delete ON cull_bin(permanent_delete_after);

-- Export jobs (persisted so they resume across app restarts).
CREATE TABLE IF NOT EXISTS export_jobs (
  id              INTEGER PRIMARY KEY,
  created_at      TEXT NOT NULL,
  preset_json     TEXT NOT NULL CHECK (json_valid(preset_json)),
  total_photos    INTEGER NOT NULL,
  done_count      INTEGER NOT NULL DEFAULT 0,
  error_count     INTEGER NOT NULL DEFAULT 0,
  status          TEXT NOT NULL DEFAULT 'queued',  -- 'queued' | 'running' | 'paused' | 'done' | 'cancelled'
  output_dir      TEXT NOT NULL,
  upload_targets  TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(upload_targets))
);

CREATE TABLE IF NOT EXISTS export_job_items (
  id           INTEGER PRIMARY KEY,
  job_id       INTEGER NOT NULL REFERENCES export_jobs(id) ON DELETE CASCADE,
  photo_id     INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  status       TEXT NOT NULL DEFAULT 'queued',   -- 'queued' | 'running' | 'done' | 'error'
  output_path  TEXT,
  error_msg    TEXT,
  started_at   TEXT,
  finished_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_export_job_items_job ON export_job_items(job_id);
CREATE INDEX IF NOT EXISTS idx_export_job_items_status ON export_job_items(status);

-- Bump schema_version → 3.
INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '3', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
```

## API surface (new commands)

Cull:
- `async fn cull_queue_next(filter: CullFilter) -> Result<Option<CullPair>>`
- `async fn cull_apply_verdict(pair_id: CullPairId, verdict: CullVerdict) -> Result<VerdictReceipt>`
- `async fn cull_undo_last() -> Result<Option<VerdictReceipt>>` — single-step undo within a session

Cull Bin:
- `async fn cull_bin_list(filter: CullFilter, page: Pagination) -> Result<Page<CullBinRow>>`
- `async fn cull_bin_restore(photo_ids: Vec<i64>) -> Result<RestoreReceipt>`
- `async fn cull_bin_delete(photo_ids: Vec<i64>, confirm_token: String) -> Result<DeleteReceipt>`
- `async fn cull_bin_empty(confirm_token: String) -> Result<DeleteReceipt>`
- `async fn cull_bin_set_retention(days: u32) -> Result<()>`

Export:
- `async fn export_start(photo_ids: Vec<i64>, preset: ExportPreset) -> Result<ExportJobId>`
- `async fn export_pause(job_id: ExportJobId) -> Result<()>`
- `async fn export_cancel(job_id: ExportJobId) -> Result<()>`
- `async fn export_status(job_id: ExportJobId) -> Result<ExportJobStatus>`

Events:
- `chronimage.cull.progress { kept, rejected, remaining, est_minutes_left }`
- `chronimage.export.progress { job_id, photo_id, percent, status, op }`

## Entitlements

- `Feature::LargeBatchExport` gates export jobs > 50 photos. Returns `true` in v1.
- `Feature::CloudReupload` gates the Google Photos / OneDrive toggles. Returns `true` in v1.
- No other gates in this phase.

## Exit criteria (test-bound)

- [ ] `tests/e2e/phase-2-cull-keyboard.spec.ts` — drive 300 pair decisions via keyboard in < 20 min simulated; verify each verdict lands in DB
- [ ] `src-tauri/tests/phase_2_verdict_raw_jpg_pair.rs` — RejectA on a RAW+JPG pair drops only the RAW row; JPG row remains; `cull_bin.source_copies_frozen_json` captures the RAW's source copies
- [ ] `src-tauri/tests/phase_2_cull_bin_retention_sweep.rs` — after advancing time 30 days via a mocked clock, sweep permanently deletes; photos with `retention_days > 30` are preserved
- [ ] `src-tauri/tests/phase_2_restore_round_trip.rs` — reject → restore → search for the photo → found with its original source_copies re-attached
- [ ] `tests/e2e/phase-2-export-jpeg.spec.ts` — 100 photos export to sRGB JPEG 4000px in < 25 s on the test GPU runner
- [ ] `src-tauri/tests/phase_2_export_cancel_resume.rs` — cancelling mid-job leaves `export_job_items.status='queued'` for unstarted rows; starting a new export with same preset resumes cleanly
- [ ] `tests/e2e/phase-2-google-reupload.spec.ts` (uses a mocked API) — upload-on-export creates `mediaItems.batchCreate` calls with the expected payload shape

## Open questions

- **Pair-mode undo depth**: single-step or session-wide? Single-step is simpler; session-wide requires a separate undo stack. Lean single-step for v1.
- **Export concurrency floor**: hard-cap at `num_cpus` to avoid thrashing the WebView2 host? Benchmark before deciding.
- **MozJPEG vs libjpeg-turbo**: MozJPEG produces smaller files but is slower. User preference unclear — start MozJPEG, let Settings override.
- **Cull Bin permanent-delete when source_copies also exist** (photo still on Google Photos): surface a warning that the local copy is gone but cloud source remains? Or just delete the local row? Lean surface-the-warning.

## TODO log

- [ ] Migration `20260601000000_phase2_cull_export.sql`
- [ ] `cull/` module in Rust (verdict, queue, bin_sweep)
- [ ] `export/` module in Rust (jpeg, heic, tiff, upload adapters)
- [ ] `src/screens/cull/` + `src/screens/cullbin/` + `src/components/ExportSheet/` ported from design
- [ ] Background sweep task registered in `main.rs`
- [ ] New Zustand stores: `useCullSession`, `useExportJobs`
- [ ] 7 exit-criterion test files
- [ ] Add `mozjpeg` + `libjpeg-turbo-sys` cargo deps (with `vendored` feature)
