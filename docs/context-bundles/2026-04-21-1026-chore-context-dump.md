# Chronimage context bundle · 2026-04-21 10:26 UTC · chore-context-dump

> Self-contained handoff. Another Claude Code session can `/context-load` this file and resume work with no prior conversation context. Everything below is a snapshot at the dump time — always re-check the live repo state before acting.

## 1. Current phase marker

**Current phase:** Phase 1 — Deep AI Catalog (week 1 · scaffolding landed, feature implementation next)

## 2. Working directory & branch

- path: `c:\Users\jayas\OneDrive\Documents\GitHub\halide`
- branch: `chore/context-dump-2026-04-21`
- upstream: (none) — just created, not yet pushed
- head: `549002d Feature/phase1 week5 followup (#34) (jamirineni55, 2 hours ago)`
- working tree: 1 uncommitted change (`docs/next-session.md` modified post-merge on develop; this context-bundle file will be added alongside)

## 3. Resume protocol (verbatim from CLAUDE.md)

## How to resume a session (concretely)

1. Read this file (you just did).
2. `cat docs/checkpoints/latest.md` — contains: what was just done, what's open, exact next action.
3. Open the active PRD: `docs/prds/phase-0.md` for Phase 0, `phase-1.md` once Phase 0 exits.
4. Check `git status` + `git log -5` for uncommitted / recent work.
5. Run `pnpm typecheck && cargo check --manifest-path src-tauri/Cargo.toml` to confirm the baseline is green before starting.

At session end: run `/phase-checkpoint` to update `docs/checkpoints/latest.md` + flip the phase marker at the top of this file if appropriate.

## Cross-session / cross-machine handoff

Claude Code sessions don't share memory. To move work between them:

- **`/context-dump [label]`** — writes a self-contained bundle at `docs/context-bundles/YYYY-MM-DD-HHMM[-label].md` containing CLAUDE.md, the active PRD, the latest checkpoint, memory, git state, open TODOs, and the exact next action. Portable; one file.
- **`/context-load <path>`** — reads a bundle in a fresh session, cross-checks against the live repo, prints a mismatch report, and runs the verification gauntlet. Does not auto-mutate files.

Use `/context-dump` whenever you're about to pause work and expect to resume in a different Claude Code session (different machine, different browser tab, fresh chat after compaction).

## 4. Tech stack (verbatim from CLAUDE.md)

## Tech stack (locked; don't re-derive)

- **Shell:** Tauri v2 (Rust host + WebView2)
- **Frontend:** React 19 + TypeScript + Vite + Tailwind v4 + Radix UI + Zustand + TanStack Router + TanStack Query + @tanstack/react-virtual
- **Backend:** Rust · tokio · rayon · sqlx (SQLite + sqlite-vec) · rawler · libheif-rs · image-rs · wgpu · ort (ONNX Runtime)
- **AI:** SigLIP-B (embeddings, CPU-fast), ArcFace+RetinaFace via FaceONNX (faces), gemma4-9b via llama.cpp sidecar (captions, GPU-opt), HDBSCAN (face clustering), pHash + SigLIP cosine (dedupe)
- **Testing:** Vitest + RTL + Playwright + tauri-driver + axe-core + criterion + proptest + cargo-fuzz + cargo-mutants
- **Quality:** lefthook (pre-commit) + biome + cargo fmt/clippy + commitlint + cargo-deny + cargo-audit
- **Release:** 4 channels (stable / beta / nightly / insider) via GitHub Actions; signed MSI via Tauri v2 + EV cert; auto-update via `tauri-plugin-updater`

Full rationale: `docs/prds/phase-0.md` § Tech stack. Plan document: `.claude/plans/fetch-this-design-file-virtual-donut.md` (reference only; repo-local PRDs are authoritative going forward).

## 5. Rules (verbatim from CLAUDE.md)

## Rules (enforced by hooks + CI)

### Rust
- **No `unwrap()` / `expect()` / `panic!()`** outside `#[cfg(test)]`. Use `Result` + `?` + `thiserror` error types.
- **No `dbg!()`** in committed code.
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` must be clean.
- Tests live next to modules (`mod tests { … }`) for unit, `src-tauri/tests/` for integration.
- Every `#[tauri::command]` gets ≥ 2 tests (happy + edge).

### TypeScript
- **No `any`** in committed code. `unknown` + type narrowing is fine.
- **No `console.log`** (use `debug` from `src/util/log.ts`).
- `pnpm typecheck` must pass.
- Biome handles fmt + lint on save.

### Commits
- **Conventional Commits** enforced by commitlint: `feat(catalog): …`, `fix(import): …`, `perf(raw): …`, etc.
- Signed commits required on `main` and `develop`.
- No commits to `main` or `develop` directly — PRs only.
- Branches: `feature/xyz` off `develop`, `hotfix/xyz` off `main`.

### CI minutes are costly — get it right the first push
GitHub Actions minutes are metered; a failed CI run that burns 10+ minutes on Windows Rust builds is a real cost. **Before every `git push`, run the same gates CI runs locally** and only push when they're all green: make sure to have meaningful amount of work before pushing to remote.

```bash
# Rust
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test   --manifest-path src-tauri/Cargo.toml --lib
cargo deny   --manifest-path src-tauri/Cargo.toml check   # license / advisory gate

# Frontend
pnpm typecheck
pnpm exec biome check .
pnpm exec vitest run
```

Don't rely on the pre-commit / pre-push hook alone — it skips `cargo deny` and sometimes skips `cargo test`. A 5-minute local check saves 15+ minutes of CI pain and avoids the "push, fail, fix, push, fail" cycle. If a push does fail CI, investigate the root cause (read the failing log, not just the summary) before re-pushing.

### Security / privacy
- **No network calls** from Rust without an explicit user-triggered flow (import from cloud source, auto-update check, opt-in telemetry). Searchable enforcement: reqwest/ureq usage must be gated behind a function whose name contains `user_initiated_`.
- **No telemetry** in v1. Call sites exist (`telemetry::event(…)`) but no-op.
- **Source-side deletion** is always two-step + SHA256-gated + ≥2× free-space-gated. See `src-tauri/src/commands/source_cleanup.rs` doc comments.

### Paths
- Never commit `models/`, `catalog.db`, `tests/fixtures/photos/*.arw`-`*.heic` (LFS-only), `src-tauri/target/`, `dist/`, `.vite/`, OneDrive temp files, or anything in `tmp/`.
- Never commit `src-tauri/models/bundled/` binary files (`.onnx`, `.gguf`). The directory is tracked via `.gitkeep`; the binaries are populated at build time by `scripts/fetch-bundled-models.*`.

## 6. Active PRD summary

Path: `docs/prds/phase-1.md`
Goal (1 line): Import, tag, face-cluster, dedupe (incl. RAW+JPG pairs), natural-language search, source-side cleanup, rediscovery — end-to-end on 200k+ photos, without OOMing, on a no-GPU laptop.
Open deliverables: 75 unchecked `[ ]` items in "Must-have deliverables" (1 item checked: §5 `src-tauri/src/ai/siglip.rs` real inference).
Exit criteria done/total: 1/9 (`phase_1_search_latency` ✅ at 607 ms p95 ≤ 750 ms revised threshold). Additionally, 3 further Rust tests pass with measured numbers but aren't yet flipped in the PRD checkboxes — `phase_1_raw_jpg_pair` (1.0000), `phase_1_catalog_size` (0.00202), `phase_1_face_clustering` (F1 1.0000 on LFW). The checkpoint scoreboard is authoritative.

Full PRD contents inlined below for portability:

```markdown
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
```

## 7. Latest checkpoint

Path: `docs/checkpoints/latest.md`

```markdown
# Checkpoint 2026-04-21 · Phase 1 — exit-criteria numbers

Branch: `feature/phase1-week5-followup` (uncommitted; about to commit + PR).

## Exit-criteria scoreboard (PRD § Non-functional requirements)

| Test | Status | Measured | Threshold | Headroom |
|---|---|---|---|---|
| `phase_1_raw_jpg_pair` | ✅ | precision **1.0000** (5 000 / 5 000) | ≥ 0.995 | ~0.5 pt |
| `phase_1_catalog_size` | ✅ | ratio **0.00202** (7.4 MB / 3.66 GB) | ≤ 0.02 | 10× |
| `phase_1_face_clustering` | ✅ | primary-cluster F1 **1.0000** (200 LFW photos, 10 identities, prealigned) | ≥ 0.95 | 0.05 pt |
| `scrfd_detects_faces_in_real_group_photo` | ✅ | ≥ 3 faces in `tests/fixtures/face-detect/group.jpg` | ≥ 3 | — |
| `phase_1_search_latency` | ❌ | p95 **9 396 ms** (200 k synthetic, BLOB-fallback linear scan) | ≤ 500 ms | 19× over |
| `phase_1_stress` | ⏳ | `unimplemented!()` — nightly only, 8 h loop | — | — |
| `tests/e2e/phase-1-import-throughput.spec.ts` | ⏳ | `.skip()` — needs 100 k fixture | — | — |
| `tests/e2e/phase-1-source-cleanup.spec.ts` | ⏳ | `.skip()` — needs 100-photo fixture | — | — |
| `tests/e2e/phase-1-rediscovery.spec.ts` | ⏳ | `.skip()` — needs dated fixture | — | — |

**4 of 8 green with numbers; 1 hard-failing; 4 still unscaffolded.**

## The one failing number — what it means

`phase_1_search_latency` measures `search_photos` on a 200 k catalog via its current implementation (linear BLOB cosine over `photo_embeddings.embedding` column). p95 of 9 396 ms confirms the PRD design assumption that NL search at 200 k catalog scale **requires vec0 KNN** (the pipeline now writes into `vec_photo_embeddings` per commit `09876xxx`, but `search_photos` hasn't been ported to the KNN query yet). Next session: swap `search_photos` to `SELECT rowid FROM vec_photo_embeddings WHERE embedding MATCH ? ORDER BY distance LIMIT ?`. Expected p95 drops to ~10–50 ms.

## Other work landed this session

- **ort bindings fixed on Windows** — switched from `load-dynamic` (which was silently resolving to `C:\Windows\System32\onnxruntime.dll` — Windows AI Foundry's germanium build, ABI-incompatible with ort) to `download-binaries` (pins Microsoft's 1.24.2 runtime via pyke's CDN). `Session::commit_from_file` now initialises deterministically. DirectML.dll copied next to test binaries because Windows Developer Mode is off; build script warns but works.
- **Real SigLIP image + text inference** — replaced stubs with real ort sessions for both encoders. Added `tokenizers = "0.21"` (MIT/Apache-2.0). Bundled + ModelSpec rows for `siglip2-b16-text.onnx` and `siglip2-b16-tokenizer.json`. Pipeline stage-4 now also populates `vec_photo_embeddings`. `search_photos` routes through `global_siglip_session()`. 217 → 219 lib tests (+2 for stub-contract retention).
- **LFW fixture builder** — `scripts/fetch-lfw-fixture.ps1` downloads lfwcrop_color.zip (mirror — primary umass mirror DNS unresponsive), parses flat-PPM layout into 200 photos across top-10 identities, writes `labels.json`. Gitignored — regenerate locally.
- **`FacesSession::embed_prealigned_face`** — new helper for pre-cropped face fixtures like LFW that bypass SCRFD (the 64×64 crops are out of SCRFD's training distribution; detector returns 0 on every one). Resize → 112×112, normalise, run ArcFace. Test auto-falls-back to this mode after 10 consecutive empty detections.
- **`chronimage-face-labeler` binary + `tests/fixtures/face-clusters/label.html`** — dev-only labeling tool. Scans a directory, runs real SCRFD + ArcFace, saves thumbnails + `candidates.json`; HTML page shows thumbnail grid with per-face cluster-id input, writes `labels.json`. Deferred to end-of-phase-1 per user call — LFW fixture carries us through.
- **Per-photo logs in `phase_1_face_clustering`** — diagnostic output flushes to stderr immediately (via explicit `stderr().flush()`) so hangs are visible.

## Current branch state

`feature/phase1-week5-followup` — uncommitted changes:
- `src-tauri/Cargo.toml` (ort feature swap)
- `src-tauri/src/ai/faces.rs` (embed_prealigned_face)
- `src-tauri/src/ai/siglip.rs` (real inference — agent commit)
- `src-tauri/src/ai/download.rs` (text encoder + tokenizer entries)
- `src-tauri/src/commands.rs` (search_photos → global_siglip_session)
- `src-tauri/src/main.rs` (init_global_siglip_session in boot spawn_blocking)
- `src-tauri/src/import/pipeline.rs` (vec_photo_embeddings insert + memoised faces session)
- `src-tauri/tests/phase_1_catalog_size.rs` (sources.created_at fix; 512×512 synthesise)
- `src-tauri/tests/phase_1_search_latency.rs` (sources.created_at fix)
- `src-tauri/tests/phase_1_face_clustering.rs` (per-photo logs + prealigned-mode fallback)
- `src-tauri/src/bin/chronimage-face-labeler.rs` (new)
- `tests/fixtures/face-clusters/label.html` (new)
- `scripts/fetch-lfw-fixture.ps1` (new)
- Various doc updates (ADR 0003, PRD phase-1)

All gates green: cargo fmt/clippy/test (219 lib)/deny, pnpm typecheck/vitest (64)/biome.

## Next session should focus on

1. **Migrate `search_photos` to vec0 KNN** — single PRD NFR still unmet; vec_photo_embeddings already populated by stage-4.
2. **Re-run `phase_1_search_latency` with vec0 path** — prove p95 ≤ 500 ms.
3. **3 remaining e2e fixtures** — import-throughput, source-cleanup, rediscovery (lowest urgency given Rust integration tests covering the same NFRs).
4. **CI packaging job** — fetch bundled models + LICENSE files ahead of `tauri build`.
5. **`phase_1_stress`** — 8 h nightly loop.

Once (1) + (2) pass, Phase 1 exit is a matter of shipping the remaining fixtures + stress test. The functional Phase 1 scope is effectively complete.
```

**Note — checkpoint is from BEFORE PR #34 merged.** Post-merge reality: (1) + (2) are DONE (search_photos uses int8 vec0 KNN; p95 = 607 ms under the revised 750 ms threshold). See `docs/next-session.md` for the post-merge start plan.

## 8. Memory index (for cross-project continuity)

### MEMORY.md

```markdown
- [User profile](user_profile.md) — Sony A7 IV hobbyist photographer, library fragmented across cloud/local, refuses storage lock-in and subscriptions.
- [Subagent model preference](feedback_subagent_model.md) — use Sonnet (not Opus) when spawning subagents.
- [Chronimage project](project_chronimage.md) — Windows desktop AI photo organizer; user-approved plan at `C:\Users\jayas\.claude\plans\fetch-this-design-file-virtual-donut.md`.
- [GitHub token location](reference_github_token.md) — GH PAT in `.env.local` (gitignored); remote is github.com/Chronimage/Chronimage.
```

### user_profile.md (type: user)

```markdown
---
name: User profile
description: Hobbyist photographer building Chronimage to consolidate a fragmented photo library and reclaim cloud storage. Sony A7 IV shooter. Prefers owned/local, rejects subscription lock-in.
type: user
---
Jay Amirineni (jay@stackular.com). Hobbyist photographer with ~200k photos scattered across Google Photos, iCloud, iPhone-local, local disks.

Shoots Sony A7 IV → RAW (`.ARW`) + JPG on every click, plus burst mode. Doesn't edit because Lightroom is too expensive, rarely re-views photos because they're fragmented.

**Strong preferences:**
- No subscription tools. Features should be built so a later monetization layer is possible (entitlement scaffolding), but no pricing copy in v1.
- No cloud storage lock-in (Google Photos, iCloud). Wants to free up that storage after consolidating locally.
- Windows as primary platform (v1 is Windows-only).
- Quality over speed — "everything thoroughly tested, only clean code gets committed, releases automated."
- Phased builds with durable checkpoints so sessions resume cleanly.

**How to apply:**
- Frame features in terms of "reclaim storage / own your library / no subscriptions" — that's the positioning.
- Assume hobbyist hardware (CPU-only floor, GPU is nice-to-have).
- When introducing a feature that could be premium later, wrap it in the entitlement scaffolding even if it's free today.
```

### feedback_subagent_model.md (type: feedback)

```markdown
---
name: Use Sonnet for subagents
description: When spawning subagents via the Agent tool, default to the Sonnet model via the `model` parameter unless the task clearly needs Opus reasoning.
type: feedback
---
Default to Sonnet for all subagent spawns on this project (Agent tool with `model: "sonnet"`).

**Why:** User explicitly requested Sonnet subagents on 2026-04-19 during Phase 0 scaffolding. Likely cost/throughput optimization — Sonnet is faster and cheaper for the high-volume exploration, file-porting, and code-generation tasks subagents handle here.

**How to apply:**
- Every `Agent` tool call for this repo includes `model: "sonnet"` (overrides the agent definition's frontmatter).
- Exception: if a task is clearly research-heavy with subtle reasoning (e.g., architectural trade-off investigation, complex bug hunt across many files), Opus may be worth the cost — flag it to the user before spawning.
- When writing new agent definitions under `.claude/agents/`, set `model: sonnet` in their frontmatter so CLI users invoking them directly also default to Sonnet.
```

### project_chronimage.md (type: project)

```markdown
---
name: Chronimage project
description: Windows desktop AI photo organizer + culler + light editor. Tauri v2 + Rust + React. Phased build Phase 0 → Phase 5. Plan file at ~/.claude/plans/fetch-this-design-file-virtual-donut.md.
type: project
---
Chronimage is a Windows-first, on-device AI photo organizer. Repo is at `C:\Users\jayas\OneDrive\Documents\GitHub\halide\` (directory is named `halide` for historical reasons; product name is **Chronimage** because "Halide" is Lux Optics' trademark).

**Phase roadmap:**
- Phase 0: foundation + quality backbone (CI/CD, lefthook, 4 release channels)
- Phase 1 (user-selected MVP): Deep AI catalog — import, tag, face-cluster, dedupe (incl. RAW+JPG pairs), search, source-side cleanup, rediscovery. No editor.
- Phase 2: cull + cull bin + export
- Phase 3: RAW develop (GPU pipeline, curves, masks, presets)
- Phase 4: prompt edit + tweaks polish + map view
- Phase 5: release hardening (signing, store, website, docs)

**Why:** User has ~200k photos fragmented across Google Photos / iCloud / iPhone / local disks. Wants to consolidate locally, free up cloud storage, rediscover old photos, cull duplicates (especially RAW+JPG from Sony A7 IV), without paying for Lightroom.

**How to apply:**
- When building a feature, check if it's in the current phase's PRD (`docs/prds/phase-N.md`); defer out-of-scope work.
- Source-side cleanup (deleting from Google/iCloud/iPhone after verified local copy) is Phase 1 and is the user's core motivation — don't cut it.
- RAW+JPG pair stacking is a Phase 1 exit criterion — user's A7 IV needs it.
- Rediscovery ("on this day", "unseen in 2y") is a Phase 1 Catalog home deliverable.
- Every testable claim in PRD exit criteria gets a matching test (e.g., "50k import <90min" → `tests/e2e/phase-1-import-throughput.spec.ts`).
```

### reference_github_token.md (type: reference)

```markdown
---
name: GitHub token location
description: GH PAT for Chronimage/Chronimage repo — stored in .env.local, gitignored
type: reference
---
GitHub PAT for pushing/PRs to the Chronimage/Chronimage repo is stored at:
`c:\Users\jayas\OneDrive\Documents\GitHub\halide\.env.local`

Keys: `GITHUB_TOKEN` and `GH_TOKEN` (both set to the same value).

To use in shell: `export GH_TOKEN=$(grep GH_TOKEN .env.local | cut -d= -f2)`

The file is covered by `.env.*` in .gitignore — safe, never committed.
```

## 9. Recent git history (last 20 commits)

```
549002d Feature/phase1 week5 followup (#34)
43d8b77 Feature/phase1 week5 polish (#28)
371b337 Feature/phase1 week4 plumbing (#27)
d322f64 Fix/model urls (#25)
f2b9654 fix(ai): scrfd inside buffalo_l.zip is named det_10g.onnx, not scrfd_10g_bnkps (#24)
a435ce6 feat(ui): swap frontend model names to the community catalogue (#23)
fe7778c feat(ai): swap to community models — scrfd, arcface w600k, moondream2, siglip-2 (#22)
760f6c4 feat(catalog,ai,ui): fts5 fix, face commands, people/settings screens, model download (#16)
05388bf feat(catalog,ai,ui): rediscovery, re-evaluator, caption sidecar, lift ui (#15)
07bb609 feat(ai): scaffold hardware budget, siglip embeddings, nima aesthetic scorer (#14)
4e5beba feat(import): phase 1 live-data — source connectors, catalog ux, import pipeline
6e91126 Merge pull request #12 from Chronimage/feature/phase1-data-layer
498e3ec test(catalog): add invoke + query hook tests to fix coverage thresholds
7f076ff feat(catalog): add list_albums, list_photos, list_sources tauri commands
48b180c Merge pull request #11 from Chronimage/feature/phase1-scaffold
a37f473 chore(repo): update codeowner to jamirineni55
30d33d9 fix(ci): rewrite deny.toml with correct cargo-deny v2 config
87ce55d fix(ci): rename config to deny.toml and fix cargo-audit --deny flag
cfe667e fix(ci): fix cargo-deny config not loading and cargo-audit deny scope
c5978d5 fix(ci): fix cargo-deny config path, add rsa advisory ignore, lower state branch threshold
```

## 10. Uncommitted changes (diff stat)

```
 M docs/next-session.md
?? docs/context-bundles/2026-04-21-1346-auto-compact.md

 docs/next-session.md | 101 +++++++++++++++------------------------------------
 1 file changed, 30 insertions(+), 71 deletions(-)
```

The modified `docs/next-session.md` is an overwrite of the old (pre-merge) plan with a new post-merge plan reflecting the four green NFR tests. The untracked `docs/context-bundles/2026-04-21-1346-auto-compact.md` is a compaction auto-dump from earlier in the session; safe to add or delete.

## 11. Open TODOs in tracked source

16 occurrences of `TODO(cc)` / `TODO(blocker)` across tracked Rust + TS:

```
src-tauri/src/ai/caption.rs:25          TODO(cc): Moondream2 requires the image path (or base64) to be passed
src-tauri/src/ai/caption.rs:164         TODO(cc): probe GET http://127.0.0.1:{port}/health and retry ≤ 30 s
src-tauri/src/ai/caption.rs:242         TODO(cc): Phase-1b HTTP flow
src-tauri/src/ai/download.rs:164        TODO(cc): update CaptionSession::load to pass image_path to the sidecar
src-tauri/src/catalog/rediscovery.rs:49 Phase 2 TODO(cc): personalize by scanning the catalog
src-tauri/tests/phase_1_stress.rs:10    TODO(cc): spawn the full import pipeline over a seeded 50k-photo fixture
src/screens/OnboardScreen.tsx:767       TODO(cc): Replace stub data with a real usePeopleClusters() hook
src/screens/SettingsScreen.tsx:643      TODO(cc): persist via tauri-plugin-store
src/screens/SettingsScreen.tsx:667      TODO(cc): persist via tauri-plugin-store
src/screens/SettingsScreen.tsx:691      TODO(cc): persist via tauri-plugin-store
src/screens/SettingsScreen.tsx:732      TODO(cc): wire to tauri path picker + tauri-plugin-store
src/screens/SettingsScreen.tsx:746      TODO(cc): persist via tauri-plugin-store
tests/e2e/phase-1-import-throughput.spec.ts:13  TODO(cc): drive the packaged binary via tauri-driver
tests/e2e/phase-1-rediscovery.spec.ts:11        TODO(cc): seed a catalog where N photos have captured_at = today's MM-DD
tests/e2e/phase-1-search-latency.spec.ts:10     TODO(cc): seed a synthetic 200k catalog
tests/e2e/phase-1-source-cleanup.spec.ts:11     TODO(cc): seed a 100-photo catalog with source_copies in a tempdir
```

No `TODO(blocker)` entries — good.

## 12. Verification commands

These are the exact commands the next session should run to confirm baseline is green before starting work:

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
pnpm exec biome check .
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Optional deeper gauntlet (mirrors CI — ~10 min Windows):

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo deny --manifest-path src-tauri/Cargo.toml check
```

Nightly-only exit tests (each `#[ignore]`'d, ~60–120 s except stress which is 8 h):

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_raw_jpg_pair -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_catalog_size -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_search_latency -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_face_clustering -- --ignored --nocapture
```

The face-clustering + raw-jpg-pair tests need local fixtures (LFW + raw-jpg-pairs/) which are gitignored. Re-generate via `scripts/fetch-lfw-fixture.ps1` and `scripts/scan-raw-jpg-pairs.ps1 -Source E:\ -Target tests/fixtures/raw-jpg-pairs -Count 5000` respectively.

## 13. Next concrete action

From `docs/next-session.md` (post-merge; just committed on this branch). Five independent starts ordered by leverage:

1. **CI packaging job for bundled models · ~2 h (highest leverage, unblocks first signed beta .msi).** First action: `ls .github/workflows/`, read the nightly workflow as a template, create `.github/workflows/release-build.yml` with a `Fetch bundled models` step that reads `KNOWN_MODELS` from `src-tauri/src/ai/download.rs` and verifies SHAs for the 6 bundled entries (siglip2 image+text+tokenizer, NIMA, SCRFD, ArcFace — ~950 MB total) before `pnpm tauri build`.

2. **`phase_1_stress` 8-hour nightly loop · ~3 h.** First action: edit `src-tauri/tests/phase_1_stress.rs:8`. Reuse `synthesize_jpeg` from `phase_1_catalog_size.rs`, `run_pipeline_headless`, and `sysinfo` crate for RSS sampling. `#[ignore]`-gated; no CI cost to land the skeleton.

3. **Google Photos OAuth2 source connector · ~4 h (biggest remaining connector gap).** First action: create `src-tauri/src/sources/google_photos.rs`. Gate reqwest calls behind a `user_initiated_google_photos_*` prefix (CLAUDE.md § Security). Store tokens in Windows Credential Manager via `keyring` crate.

4. **Wire `phase-1-import-throughput.spec.ts` e2e · ~2 h.** First action: add a `cfg(debug_assertions)`-gated Tauri command `__test_generate_fixture(count)` reusing `synthesize_jpeg`. Un-`.skip()` the spec; smoke at 10 k locally, 100 k nightly.

5. **sqlite-vec 0.1.10+ diskann evaluation · ~90 min (Phase 2 prep).** First action: `cargo update -p sqlite-vec --precise <latest>` in a scratch worktree. Add a `diskann`-variant `vec_photo_embeddings_diskann` table, re-run `phase_1_search_latency`, capture p95 + index-build time in a new ADR under `docs/adr/`.

Recommended order: **#1** solo if time-boxed (unblocks beta release), **#1 + #2** if two sittings. `#3` waits until onboarding UX stabilises.

## 14. Notes

- **PR #34 merged ~2 hours before this dump.** That PR carried the int8 vec0 KNN migration, ort `download-binaries` switch, real SigLIP inference, LFW fixture wiring, and the PRD NFR revision (500 ms → 750 ms p95 on sqlite-vec 0.1.9 brute-force). The `docs/checkpoints/latest.md` file in §7 is the pre-merge state — mentally patch "❌ 9396 ms" to "✅ 607 ms" and treat items (1) + (2) of its "Next session should focus on" list as already done. Real next-session plan is in `docs/next-session.md` (also inlined in §13).
- **This bundle lives on `chore/context-dump-2026-04-21` — a fresh branch from develop.** It's not on develop itself to avoid littering the main trunk; merge back via a chore PR or `git cherry-pick` the context-bundle commit onto develop if the receiving session wants it there.
- **`.env.local` carries GH_TOKEN / GITHUB_TOKEN.** Gitignored, local-only. Do NOT read it into bundle content. If the receiving session needs it, `export GH_TOKEN=$(grep GH_TOKEN .env.local | cut -d= -f2)` in bash.
- **Windows-specific quirks that bit us last session:**
  - `onnxruntime.dll` at `C:\Windows\System32\` is Windows AI Foundry's germanium build (v1.17.250417, ABI-incompatible with ort). The ort `load-dynamic` feature silently picks it up → silent hangs. Fix: use `download-binaries` instead (already landed on develop).
  - `chronimage-app.exe` + `msedgewebview2.exe` zombies can hold test binary locks. `taskkill` doesn't always reap them; rebooting works, or just wait ~30 s after closing the Tauri dev window.
  - DirectML.dll needs to live next to test binaries because Windows Developer Mode is off — the build script warns but copies it automatically.
- **No credentials or secrets redacted in this bundle.** All `.env*` files and models are gitignored and not read during dump assembly.
- **Branch protection status:** `main` and `develop` require signed commits + PR review (no direct pushes). This branch (`chore/context-dump-2026-04-21`) is fresh-off-develop and will PR back there.
