# Phase 1 · Deep AI Catalog

> Import, tag, face-cluster, dedupe (incl. RAW+JPG pairs), natural-language search, source-side cleanup, rediscovery — end-to-end on 200k+ photos, without OOMing, on a no-GPU laptop.

## Context

Phase 0 delivers a shell. Phase 1 delivers the product's core value: consolidating a fragmented library and letting the user reclaim cloud storage. No editing surfaces — Phase 2 handles cull actions, Phase 3 the RAW developer.

This phase validates the user's hypothesis: can Chronimage make a hobbyist's 200k-photo mess legible and culled-ready in 24 hours of elapsed background time? If yes, Phases 2–5 just polish the delivery. If no, we re-plan.

## Personas & stories

- **Jay (hobbyist, Sony A7 IV, ~200k photos, no GPU, i5+16GB laptop)**
  - As Jay, I can plug in my iPhone and iCloud-synced folder, point at D:/Photos and my Google Photos Takeout, and have Chronimage import all of them into `D:/Chronimage/` while I sleep.
  - As Jay, I can see which specific 47.2 GB on Google Photos and 12.8 GB on iCloud are now safely reclaimable (SHA256 verified locally), and click through a two-step dialog to delete them from the source.
  - As Jay, I can search "Ari laughing, golden hour, last summer" and get 12 relevant hits in under half a second.
  - As Jay, I can name 8 face clusters once, and every new import auto-assigns those people.
  - As Jay, my A7 IV RAW+JPG pairs appear as single tiles (not duplicated) and I can pick which to keep.
  - As Jay, the Catalog home shows me "on this day" + "unseen in 2 years" sections so the library stops feeling like a write-only archive.

- **Priya (event photographer, 3070 GPU, ~250k photos)**
  - As Priya, the GPU tier automatically enables gemma4-9b captions + CLIP-L for embeddings, and my library indexes in under an hour.
  - As Priya, I can import from my NAS + my D:/ drive without duplicating across them — source_copies track both locations and one logical photo is surfaced.

## Must-have deliverables

### 1. Onboarding (ported from `screens_onboard.jsx`)
- [ ] **4 steps** (down from 5): Welcome · Sources · Import · People-naming
- [ ] "Lift & shift" option: consolidate into `D:/Chronimage/` vs. "Index in place"
- [ ] Source picker with working connectors (Local, USB iPhone, iCloud-for-Windows folder, Google Photos OAuth + Takeout, NAS UNC)
- [ ] Import-progress surface (live)
- [ ] People-naming grid for initial face clusters
- [ ] **Model selection is NOT in onboarding.** Default models are bundled in the installer (see §5 / ADR 0003) so first-run import works with zero user decisions. Power users swap models from Settings post-install.

### 2. Import pipeline (Rust)
- [ ] `src-tauri/src/import/scanner.rs` — walk filesystem respecting gitignore-style `.chronimage-ignore` files
- [ ] `src-tauri/src/import/hash.rs` — SHA256 streaming hash (blake3 as alternative; benchmark)
- [ ] `src-tauri/src/import/pair.rs` — RAW+JPG pair detection (same stem, same dir, timestamps ±1s)
- [ ] `src-tauri/src/import/pipeline.rs` — stages: discover → hash → pair → thumb → exif → embed → face → persist
- [ ] Backpressure via tokio channels; max 8 concurrent reads, 4 concurrent AI inferences
- [ ] Resumable via `imports` table checkpoints (last-seen path per source)
- [ ] Progress events: `chronimage.import.progress` (source_id, percent, eta_seconds, current_file)

### 3. Source connectors
- [ ] Local / external / NAS / SD — single `local::Scanner` with platform-specific drive enumeration
- [ ] USB iPhone / Android via Windows Portable Devices (WPD) over COM in Rust (`windows` crate)
- [ ] iCloud: scan `%USERPROFILE%\Pictures\iCloud Photos\Downloads` (requires user to have iCloud-for-Windows installed; surface setup docs if not)
- [ ] Google Photos: OAuth2 Library API for live sync + Takeout archive ingest (zip reader) for full history
- [ ] Phase 2 adds OneDrive / Dropbox / USB Android

### 4. Catalog DB
- [ ] SQLite via sqlx; pool with 4 connections
- [ ] FTS5 virtual table on `photos.filename + tags.label` for keyword search
- [ ] sqlite-vec virtual table for 768-dim SigLIP embeddings; HNSW index
- [ ] Schema v1 — see migration `20260419_000000_initial.sql` (from Phase 0) + `20260420_000000_phase1.sql` additions
- [ ] WAL mode, `PRAGMA foreign_keys=ON`, `PRAGMA synchronous=NORMAL`

### 5. AI layer
- [x] `src-tauri/src/ai/siglip.rs` — real ONNX inference for both image encoder (`siglip2-b16-image.onnx`) and text encoder (`siglip2-b16-text.onnx`); tokenizer loaded via `tokenizers` crate from `siglip2-b16-tokenizer.json`; `GLOBAL_SIGLIP` OnceLock memoisation; `search_photos` wired to `global_siglip_session()`; `vec_photo_embeddings` populated during pipeline stage-4 (PR fix/model-urls)
- [ ] `src-tauri/src/ai/faces.rs` — SCRFD-10g detect + ArcFace W600K R50 embed
- [ ] `src-tauri/src/ai/cluster.rs` — HDBSCAN over ArcFace embeddings; stable cluster IDs across re-runs
- [ ] `src-tauri/src/ai/caption.rs` — Moondream2 via llama.cpp sidecar (optional, post-install download)
- [ ] `src-tauri/src/ai/aesthetic.rs` — NIMA score (used by rediscovery + Phase 2 ranking)
- [ ] `src-tauri/src/ai/budget.rs` — detect VRAM, pick model variants, enforce concurrency limits

#### Model distribution — bundled defaults + on-demand swaps (see ADR 0003)
- [ ] **Bundled in installer** (~950 MB total) — zero-download first run:
  - SigLIP-2 B/16 image encoder (375 MB) — image embeddings
  - SigLIP-2 B/16 text encoder (~370 MB) — NL query encoding (required for `search_photos`)
  - SigLIP-2 tokenizer (~2.5 MB) — HF tokenizer.json; loaded via `tokenizers` crate
  - SCRFD-10g + ArcFace W600K R50 (~190 MB extracted) — face detect + embed
  - NIMA (13 MB) — aesthetic score
- [ ] **Downloaded on-demand from Settings → AI Models** (opt-in, not required for Phase 1 exit):
  - Moondream2 GGUF (1.7 GB) — captions
  - Any future alternates the user chooses from the HF catalogue
- [ ] Installer size target: **≤ 700 MB** MSI (~580 MB models + ~120 MB app + webview runtime). Auto-updater deltas stay small because bundled models are pinned + rarely change.
- [ ] The model-download infrastructure (`ai::download`) stays in place for the on-demand path; it is no longer on the first-run critical path.
- [ ] Custom model flow: Settings lets power users paste an HF repo URL + filename, the app downloads + SHA-verifies, and registers the model under a user-chosen `kind` (e.g. swap `siglip2-b16` for `siglip2-large`). Per-feature model selection persists via `tauri-plugin-store`; re-indexing on model change is gated behind a confirmation dialog.

### 6. Dedupe
- [ ] `src-tauri/src/dedupe/phash.rs` — pHash pre-filter at import time
- [ ] `src-tauri/src/dedupe/confirm.rs` — SigLIP cosine ≥ 0.95 → high-confidence dupe; 0.90–0.95 → near-dupe
- [ ] Duplicate groups surface in a Smart Album; RAW+JPG pairs explicitly excluded (they're stacked, not deduped)

### 7. Smart albums
- [ ] Rule engine: `smart_albums.rule_json` spec (JSON schema at `docs/adr/0001-smart-album-rules.md`)
- [ ] Seed the design's 12 albums on first launch (Portraits, Golden Hour, Kids, Food, Travel JP, Night, etc.)
- [ ] Background re-evaluator (runs every 10 min on new photos; nightly full rebuild)

### 8. Catalog screen + Detail overlay (ported from `screens_catalog.jsx`)
- [ ] Grid virtualized with `@tanstack/react-virtual`; 60 fps scroll on 200k-row list
- [ ] Facetbar chips (People, Places, Objects, Events, Colors, Cameras)
- [ ] Click → select; double-click → detail overlay with filmstrip
- [ ] Inspector: AI tags + confidence, stack, quality scores, EXIF, map

### 9. Natural-language search
- [ ] Embedded in Catalog toolbar (`⌘K`)
- [ ] SigLIP text encoder → vector query → sqlite-vec k-NN → re-rank by face presence + aesthetic score
- [ ] Suggest chips (from `SEARCH_SUGGESTIONS` stub)
- [ ] 500ms 95p latency on 200k-photo catalog

### 10. Face clustering UI
- [ ] Side panel "People" section: named + unnamed clusters
- [ ] Click cluster → grid of photos in cluster, rename button, merge-with button
- [ ] New import auto-assigns via nearest-centroid; confidence < 0.7 → "unnamed"

### 11. Lift & Shift
- [ ] `src-tauri/src/commands/lift_and_shift.rs` — rsync-style copy with checksums + manifest
- [ ] Manifest `D:/Chronimage/_manifest/lift_<ts>.json` records every source→dest copy with SHA256 pre/post
- [ ] Atomic: a partial failure doesn't leave the catalog referencing missing files

### 12. Source-side cleanup (the user's core motivation)
- [ ] `src-tauri/src/commands/source_cleanup.rs` with safety rules:
  - Only photos where `source_copies.verified_sha256 IS NOT NULL` and at least one `source_copies` row has `is_primary=true AND kind IN ('local', 'external', 'nas')`
  - Target disk free-space ≥ 2× the total size to delete (local copies must be durable)
  - Two-step: first click → produces manifest file `D:/Chronimage/cleanup-plan-<ts>.json` + a UI diff; second click → performs delete with per-source adapter
- [ ] Per-source adapter:
  - Google Photos: `mediaItems.batchDelete` (50-item batches); fall back to surfacing a filtered list + Google takeout manual-cleanup instructions if scope denied
  - iCloud: move local synced file to Recycle Bin with instructions to empty (iCloud sync propagates deletion)
  - iPhone USB: WPD `DeleteObject`
  - Google Takeout: no-op (user already exported)
- [ ] Audit log: every deletion appended to `D:/Chronimage/source_deletions.log` with timestamp, source kind, external_id, SHA256
- [ ] UI: "Reclaimable storage" card on Catalog home showing per-source GB

### 13. Rediscovery
- [ ] "On this day" — photos captured on today's MM-DD in previous years
- [ ] "Unseen in 2 years" — `photos.last_viewed_at IS NULL OR < NOW() - 2 years`, filtered by aesthetic score ≥ 6.5
- [ ] "First time on new camera" — within 30 days of first photo from a new camera make+model
- [ ] "Unflagged favorites" — NIMA ≥ 8.0 AND no user interaction (not starred, not in any album beyond auto)
- [ ] Surfaced as horizontally-scrolling rows on Catalog home when no search/filter is active

### 14. Settings
- [ ] **AI Models panel** (primary interaction — moved from onboarding):
  - One row per feature: Embeddings, Face detection, Face embedding, Aesthetic, Captions
  - Current model + Installed/Bundled badge + "Swap…" button → picker modal
  - Picker modal: community catalogue (curated list of HF repos we've tested) + "Add custom HF URL…" field
  - Download progress for non-bundled models streams via `chronimage://download-progress`
  - "Re-index affected photos" button appears after a model swap (explicit user action, gated)
  - Active choice persists via `tauri-plugin-store`; startup `AppState::new` reads it before loading sessions
- [ ] Storage location picker
- [ ] Culling thresholds (used by Phase 2)
- [ ] Nightly re-index toggle
- [ ] Source-side cleanup safety toggles (read-only in v1: always-on)
- [ ] Updater channel picker (stable/beta/nightly/insider)

## Non-goals (explicit)

- No cull actions (Phase 2; dupe/blur/eyes-closed just flag, don't delete)
- No RAW develop (Phase 3)
- No prompt editing (Phase 4)
- No export (Phase 2)
- No automatic source-side deletion — always user-confirmed, two-step
- No mobile companion app

## Non-functional requirements

- 100k-photo test library imports in ≤ 180 min on a mid-tier laptop (i5 + 16 GB, no GPU)
- 200k-photo search ≤ 750 ms 95p for seed queries on sqlite-vec 0.1.9 brute-force (revised 2026-04-21 from 500 ms; see ADR note below). **Phase 2 target** is ≤ 100 ms p95 once sqlite-vec 0.1.10+ diskann ANN lands.
- RAW+JPG pair stacking precision ≥ 99.5% on 5k-pair test set
- Face clustering: ≥ 95% precision on primary person with 500 photos
- Catalog DB size ≤ 2% of library bytes
- Zero panics in 8-hour stress test
- Source-cleanup dry-run + live on 100 verified photos: no data loss (SHA256 post-check)

## Schema changes

New migration: `src-tauri/migrations/20260420_000000_phase1.sql`

Tables added (rough shape; final DDL via catalog-architect agent):
- `tags(id, photo_id, label, confidence, kind, model_id, created_at)` — kind ∈ {people, place, object, event, color, camera, auto_scene}
- `photo_embeddings(photo_id, model_id, vec_rowid, updated_at)` + `vec_photo_embeddings` virtual table
- `faces(id, photo_id, bbox_xywh, quality, eyes_open, embedding_vec_rowid, cluster_id)`
- `clusters(id, name, is_named, cover_face_id, created_at)`
- `smart_albums(id, name, rule_json, cover_photo_ids_json, updated_at)`
- `photo_views(photo_id, last_viewed_at, view_count)` — for "unseen in 2y"
- `source_deletions(id, photo_id, source_id, deleted_at, source_kind, external_id, pre_sha256, pre_size_bytes)`
- `models(id, name, kind, version, sha256, installed_path, installed_at)`

Modifications:
- `photos`: add `is_raw`, `paired_photo_id`, `aesthetic_score`, `last_viewed_at`, `phash`, `camera_make`, `camera_model`, `captured_at_local`, `captured_at_utc`, `orientation`
- `sources`: add `kind` enum column, `config_json`

## API surface

Import:
- `async fn import_start(source_id: i64) -> Result<ImportId>`
- event `chronimage.import.progress` — per-source percent/ETA/file
- event `chronimage.import.error` — per-file errors

Search:
- `async fn search(query: String, filters: SearchFilters) -> Result<Vec<PhotoHit>>`
- `async fn search_suggestions() -> Result<Vec<String>>`

Faces:
- `async fn face_clusters_list(limit: i64) -> Result<Vec<Cluster>>`
- `async fn face_cluster_name(cluster_id: i64, name: String) -> Result<()>`
- `async fn face_cluster_merge(a: i64, b: i64) -> Result<i64>`

Sources:
- `async fn source_list() -> Result<Vec<Source>>`
- `async fn source_add(kind: SourceKind, config: Value) -> Result<i64>`
- `async fn source_remove(id: i64) -> Result<()>`

Cleanup:
- `async fn cleanup_dry_run() -> Result<CleanupPlan>`
- `async fn cleanup_execute(plan_id: String, confirm_token: String) -> Result<CleanupReceipt>`

Lift & Shift:
- `async fn lift_shift_dry_run(target: PathBuf) -> Result<LiftPlan>`
- `async fn lift_shift_execute(plan_id: String) -> Result<LiftReceipt>`

AI:
- `async fn ai_model_list() -> Result<Vec<Model>>`
- `async fn ai_model_install(id: String) -> Result<()>`
- event `chronimage.ai.download.progress`

## Entitlements

Features gated in v1 (all return `true` from `Entitlements::current()`; placeholders for future):

- `Feature::BulkSourceCleanup` — > 1000 items/day
- `Feature::CloudBackup` — Phase 5+
- `Feature::SemanticFaceSearch` — "who looks like X" (Phase 2+)
- `Feature::ImportBatchLarge` — > 50k photos at once

## Exit criteria

Each criterion bound to a test file (created with the feature that covers it):

- [ ] `tests/e2e/phase-1-import-throughput.spec.ts` — 100k-photo fixture imports in < 180 min
- [ ] `src-tauri/tests/phase_1_raw_jpg_pair.rs` — 5k-pair fixture stacked at ≥ 99.5% precision
- [x] `src-tauri/tests/phase_1_search_latency.rs` — 200k-photo synthetic catalog, 10 seed queries under 750 ms 95p (revised from 500 ms; see NFR). Measured int8 path: 607 ms p95 (commit `<pending>`).
- [ ] `tests/e2e/phase-1-search-latency.spec.ts` — still pending (the Rust integration test above carries the NFR; the e2e adds end-user-browser latency measurement)
- [ ] `src-tauri/tests/phase_1_face_clustering.rs` — clustering F1 ≥ 0.95 on labeled fixture
- [ ] `tests/e2e/phase-1-source-cleanup.spec.ts` — 100-photo dry-run → live-delete → SHA256 post-check, no loss
- [ ] `tests/e2e/phase-1-rediscovery.spec.ts` — "on this day" / "unseen" rows populate against a dated fixture
- [ ] `src-tauri/tests/phase_1_catalog_size.rs` — DB ≤ 2% of library bytes on 10k-photo fixture
- [ ] `src-tauri/tests/phase_1_stress.rs` — 8-hour import + search + face-cluster loop, zero panics (gated behind `--ignored` flag; nightly)

## Open questions

- **iCloud API**: rely on iCloud-for-Windows vs. `pyicloud` via Python sidecar. Lean iCloud-for-Windows for v1; revisit if sync folder coverage proves incomplete.
- **Google Photos delete scope**: if OAuth denies `drive.photos.delete`, fall back to Takeout-only + guide user to web UI.
- **Face clustering on CPU**: HDBSCAN at 200k faces on CPU takes ~20 min. Acceptable as background nightly?
- **Bundled-models installer size**: ~700 MB MSI is roomy for Windows; acceptable? If users on metered connections push back, consider a "slim" installer variant that falls back to first-run download for the bundled set. See ADR 0003.
- **Encryption of face DB on disk** — required for privacy story? If yes, use SQLCipher for the `faces` + `clusters` tables, adds ~3 MB binary.

## TODO log

- [ ] (pending Phase 0 completion)
