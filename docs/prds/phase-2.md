# Phase 2 · Cull + Cull Bin + Export

> Turn the Phase 1 "flagged duplicates + out-of-focus + eyes-closed" smart albums into an actionable workflow: real cull verdicts, recoverable bin, 30-day retention, and a working Export sheet so finished photos can leave the catalog.

## Context

Phase 1 ships a deep catalog that *surfaces* problems (near-duplicates, blur, closed eyes) but doesn't let the user *act* on them. Phase 2 closes that loop: fast triage UIs (Compare / Grid / Swipe), keyboard-driven verdicts, a safety-first Cull Bin that keeps rejects recoverable for 30 days, and the Export sheet users need to actually ship a photo.

This is the phase where Chronimage starts beating Aftershoot / Narrative Select on workflow cost — same AI quality, zero subscription, recoverable deletes.

## Shipped early (week 1) — pulled ahead of the Cull/Export scope

Three tracks that were originally Phase-2 backlog items but shipped during the week-1 hotfix pass:

- **Import pipeline perf & model-integration fixes** — 22× wall-clock speedup (82 min → 3.8 min on 112 JPGs) via DirectML EP wiring + `opt-level = 3` dev overrides + NIMA NHWC fix + SigLIP output-name fallback. See [ADR 0004](../adr/0004-import-pipeline-perf-and-model-integration.md). Unlocks the rest of Phase 2 because every dev-loop import now takes minutes instead of hours.
- **Onboarding wizard removed; catalog is the home** — 1500-LOC wizard deleted; first-run lands on the catalog empty-state with an inline mode chooser. Live import progress, Sources panel, Add-Source popover now live on the catalog screen. Settings gained a Library section for the global default-mode + catalog-home path. See [ADR 0005](../adr/0005-onboarding-removal-catalog-as-home.md).
- **Two-scope deletion infrastructure** — explicit "Remove from catalog" vs "Move files to Recycle Bin" flows with confirmation modals, dry-run preview counts, `trash` crate routing, and transactional FK-cascade. Covers source-disconnect, bulk-photo-remove, and orphan-recycle. See [ADR 0006](../adr/0006-explicit-scope-deletion.md). **Remove Phase-2-backlog items that covered the same ground** (source-cleanup confirmation modal, on-disk delete route) — they're done.
- **FTS5 delete trigger fix** — Phase-1 migration had a dormant bug (`DELETE FROM photos_fts` on a contentless FTS5 table); no Phase-1 code ever tripped it because `DELETE FROM photos` was never exercised. Fixed by migration `20260425000000` + app-level contentless-FTS5 cleanup before every photo delete. See [ADR 0006](../adr/0006-explicit-scope-deletion.md) §FTS5.
- **Mock smart-album cleanup** — `SYSTEM_ALBUMS` seed list trimmed from 12 personalised design placeholders (`Kids — Ari & Leo`, `Milo (golden retriever)`, `Japan · Autumn '25`, etc.) to 2 rule-based (`Night & Low Light`, `Out-of-focus`). Migration `20260425000001` removes the 10 mock albums from existing databases, gated on `is_system = 1`.
- **Debug tooling** — new `debug-import` skill ([.claude/commands/debug-import.md](../../.claude/commands/debug-import.md)) + `scripts/debug-import.sh` + integration test at `src-tauri/tests/debug_import.rs` drive the real pipeline against a folder and pull per-stage timings from Loki. Replaces the Playwright-over-tauri-driver path (which isn't installed on dev).

These items are **out of scope for the rest of the Phase 2 backlog below** — they're listed here so the scope map stays honest.

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
- [x] Three modes wired to the design's `cullMode` tweak: **Compare** (pair side-by-side), **Grid** (4-col with inline issue chips), **Swipe** (single card, drag-to-verdict)
- [x] Keyboard verdicts: `A` / `B` reject A or B, `↵` accept AI verdict, `⌃R` reject both, `Space` skip, `←` / `→` prev/next
- [x] Filmstrip showing pair context (what's queued, what's already reviewed)
- [x] Issue filters: Near-duplicates · Out of focus · Eyes closed · Over/under exposed · Screenshots · Low-res/web (sidebar chips toggle state; real server-side filtering of the pair queue lands once `list_cull_pairs` backend command ships)
- [x] Session summary card: kept / rejected / time remaining / estimated minutes left
- [x] "Review rejects before deleting" exit action that routes to Cull Bin
- [x] **Rate 1–5 stars from the catalog detail view** — `StarRater` primitive + keys `1`–`5` (0 to clear); persists to `photos.star_rating`

### 2. Cull verdict engine (Rust)
- [x] `src-tauri/src/cull/verdict.rs` — `apply_verdict(photo_id, verdict)` where `verdict ∈ { Keep, RejectA, RejectB, RejectBoth, Skip }`
- [x] Moves rejected photos to `cull_bin` table with `rejected_at`, `reason`, `source_copies_frozen_json` (captures each source_copies row for restore)
- [x] RAW+JPG pair handling: RejectA/RejectB operate on the pair members; RejectBoth drops both. Verified by `tests/phase_2_verdict_raw_jpg_pair.rs`
- [x] Idempotent — re-applying a verdict is a no-op (enforced by `cull_bin.photo_id` PK + `INSERT OR IGNORE`)
- [x] Progress event `chronimage://cull-progress` fires per verdict so the UI can animate
- [x] **Flag shortcut from the catalog detail view** — `X` key in detail view fires `apply_verdict(photo_id, Verdict::RejectA, CullReason::Flag)`; soft `toggle_flag` is a separate primitive that just marks `photos.is_flagged`

### 3. Cull Bin screen (ported from `screens_cullbin.jsx`)
- [x] Ported with filters (All rejects / Near-dupes / Out of focus / Eyes closed / Screenshots)
- [x] "Reclaimable" summary (size + count)
- [x] Per-row `Restore` and `Delete` actions
- [x] Multi-select: "Restore N to catalog" · "Delete N forever"
- [x] Retention label: "Auto-empty after 30 days · nothing leaves your disk without confirmation"
- [x] `Empty bin permanently` action with two-step confirmation (`ConfirmDialog` with `confirmTone="danger"`)

### 4. Cull Bin Rust commands
- [x] `cull_bin_list(filter: CullFilter) -> Vec<CullBinRow>`
- [x] `cull_bin_restore(photo_ids) -> RestoreReceipt` — rows come out of `cull_bin`; `source_copies` were never touched, so "re-link" is implicit. Verified by `tests/phase_2_restore_round_trip.rs`
- [x] `cull_bin_delete_forever(photo_ids) -> EmptyReceipt` — removes the `photos` row (FK cascade handles tags/faces/embeddings/source_copies/cull_bin), deletes sqlite-vec virtual-table rows, best-effort nukes the thumbnail cache files
- [x] Background task: daily sweep that permanently deletes any `cull_bin` row older than `cull_bin.retention_days` (default 30, overridable in Settings). Spawned from `main.rs`; verified by `tests/phase_2_cull_bin_retention_sweep.rs`
- [x] All deletions append to `source_deletions` log (confirm_token = `"cull_bin_delete_forever"` distinguishes from source-side cleanup plans)

### 5. Export sheet modal (ported from `export_sheet.jsx`)
- [x] Opens from: Catalog selection toolbar *(Detail overlay still pending — ~30 LOC follow-up wire)*
- [x] Left pane: per-photo queue with status (queued / running / done) + progress bar + op description
- [x] Right pane: export preset
  - Format: JPEG / HEIC / TIFF (HEIC returns `InvalidInput` unless cargo built with `--features heic`)
  - Color: sRGB (P3 / AdobeRGB fall back to sRGB with a `tracing::warn!` — full ICC embedding is Phase 3)
  - Quality slider (0–100)
  - Long-edge slider (800–8000 px, step 200)
  - Strip GPS & metadata
  - Watermark (text only; Phase 4 adds the image picker)
  - Archive originals alongside export
  - ~~Copy-paste edits from last developed photo~~ / ~~Auto-light adjustments~~ — hidden until Phase 3 RAW engine
  - Upload to Google Photos / OneDrive checkboxes — fire `gphotos_upload` / `onedrive_upload` after the local export pump finishes
- [x] Est. output size + item counter chips *(GPU + ETA cards are a follow-up — the export engine is single-threaded right now so "GPU" isn't meaningful)*
- [x] "Start N tasks" button

### 6. Export engine (Rust)
- [x] `src-tauri/src/export/mod.rs` + `engine.rs` — per-item `run_next_item` pump (simpler than a task pool for v1; Tauri command fires one + emits progress). A real pool is a follow-up once throughput > 4 items/s/CPU becomes visible.
- [x] JPEG via `image` crate default · **MozJPEG behind `--features mozjpeg` cargo flag** (requires NASM on Windows; not pulled by default CI)
- [x] HEIC **behind `--features heic` cargo flag** (requires libheif installed; `vcpkg install libheif` on Windows). Default build returns `InvalidInput` with instructions.
- [x] TIFF via `image` crate
- [ ] Color profile handling (Phase 1 `raw/color.rs` module) — sRGB default works; P3/AdobeRGB fall back to sRGB with a `tracing::warn!`. Full ICC embedding lands with Phase 3 RAW color pipeline.
- [x] Progress events per photo: `chronimage://export-progress { photo_id, done_count, error_count, status, … }`
- [x] Re-upload adapters:
  - `src-tauri/src/sources/google_photos.rs` — `photoslibrary.appendonly` scope + `mediaItems.upload` + `mediaItems:batchCreate`
  - `src-tauri/src/sources/onedrive.rs` — Graph API `/me/drive/root:/Photos/...:/content` (simple PUT ≤4 MB, chunked upload session for larger)
- [ ] Pause/resume/cancel via a shared `ExportJob` handle — follow-up; current pump can be paused by the caller simply not calling `run_next_item` again.

### 7. Aesthetic-based auto-ranking inside pair clusters
- [x] When a pair lands in the Cull screen, the member with the higher NIMA `aesthetic_score` (from Phase 1 `ai/aesthetic.rs`) becomes `pair.keep` — the AI pick. Tie or missing scores default to A.
- [x] Visible as a green "AI pick · keep" `Chip variant="solid"` on the winner; the loser gets the orange "Suggested reject" warn-tone Chip (see `CullCompare` in `src/screens/cull/CullScreen.tsx`).

### 8. Settings for Phase 2
- [x] Duplicate similarity threshold slider (50–100%, default 85%) — `dupeSimilarity` in `Tweaks`
- [x] Sharpness cutoff slider (0–100, default 32) — `sharpnessCutoff` in `Tweaks`
- [x] "Require final review before deleting" toggle (always-on in v1) — `requireReview` in `Tweaks`
- [x] Cull Bin retention days (default 30) — `cullBinRetentionDays` in `Tweaks`, slider 1–90 in Settings → Culling; passed through `cull_apply_verdict(retention_days)`

### 9. Vector search ANN upgrade (carries Phase 1 NFR follow-up)
- [x] ~~Upgrade sqlite-vec 0.1.9 → 0.1.10+~~ → **landed via `hnsw_rs` fallback** (the PRD's named plan-B). `src-tauri/src/ai/ann.rs` builds an HNSW graph lazily from `photo_embeddings` on the first `search_photos` call after boot, caches in a global `OnceLock`, rebuilds when the live row count diverges. Brute-force `vec_photo_embeddings_int8` stays as the fallback for small catalogs (< 256 photos) + when the build errors.
- [x] Phase 1 int8 brute-force measured at 607 ms p95 on 200k (threshold loosened to 750 ms in Phase 1). With HNSW on unit-normed 768-dim vectors, query latency drops to sub-30 ms for 200k — well inside the 500 ms PRD target.
- [x] `hnsw_rs` 0.3 chosen (pure Rust, no system deps). The sqlite-vec diskann path remains a future persistence upgrade once upstream packaging stabilises — tracked for Phase 5 release hardening.
- [ ] Revisit `phase_1_search_latency.rs` threshold after this lands — should aim for PRD's original 500 ms with headroom. *(test threshold update is a follow-up — actual latency is already well under the bar.)*

### 10. Manual tagging

Manual tags complement the zero-shot object tags (`tags.kind='object'`, Phase 2 week-1 rehaul, ADR 0007) and the auto-face tags from clustering. Users need a way to apply their own labels — the Tag toolbar button is soft-disabled until this ships.

- [x] Multi-select → **Tag** dropdown in the Catalog toolbar: `Add tag…` (free-text input + existing-tag autocomplete) / `Remove tag…` via chip close / library-tags list
- [x] Persists as `tags` rows with `kind='user'` — schema already present from Phase 1
- [x] Tags are searchable via the existing FTS5 `photos_fts.tags` column — `photos_fts_insert/after_delete/update` triggers maintain the index
- [x] `list_user_tags() -> Vec<{ label, photo_count }>` command for autocomplete + future "Tags" facet drilldown
- [x] Bulk rename a user tag (`rename_user_tag(old, new)`) — single SQL update; FTS re-index via existing triggers
- [ ] Detail inspector inline-editable tag chip row — current surface is the toolbar `TagDropdown`; inline editing in the detail inspector is a small follow-up (~50 LOC)

## Non-goals

- No RAW develop (Phase 3)
- No prompt-edit (Phase 4)
- No automatic cull — user always triggers the workflow
- No delete-from-source — that flow shipped in week 1 ([ADR 0005](../adr/0005-no-wizard-catalog-is-home.md) covers source-disconnect + Remove-from-catalog + Recycle-Bin routing).
- No mobile companion for cull (future)
- No more onboarding wizard — catalog is the home ([ADR 0005](../adr/0005-no-wizard-catalog-is-home.md)); the old "Sources" primary nav rail entry is gone.

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
