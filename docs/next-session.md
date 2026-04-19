# Next session · Phase 1 week 2 → week 3

Landed since last plan (PR #14 + #15 merged to `develop`): faces/cluster/caption scaffolds, rule-engine composition (All/Any/Not/CapturedAt/Camera/FaceCluster/Starred/on_mmdd), lift & shift Rust + UI, source cleanup end-to-end, rediscovery (3 seeded albums), background re-evaluator (10-min tokio), `chronimage-cli migrate` wired, migrations `is_starred` + `smart_albums.kind` + `photos.last_viewed_at`. 171 Rust + 60 frontend tests green on develop (`05388bf`).

## Starts (in order)

### 1. Fix FTS5 contentless-trigger defect · 20 min
First file: new migration `src-tauri/migrations/20260424000000_fts5_triggers_fix.sql`.

The existing `tags_fts_insert` / `tags_fts_delete` triggers do `UPDATE photos_fts SET tags = ...` — SQLite's contentless FTS5 (`content=''`) rejects UPDATE on column values. Fix: drop both triggers, recreate as DELETE + INSERT of the rowid with refreshed columns. Unblocks end-to-end imports the moment AI-tagging writes any row to `tags`.

Why highest leverage: silent data-path bug with a ~15-line migration. Noted in PR #15 as deferred. Catch-early before Phase 2 culling wires up tag filters.

### 2. PeopleScreen shell + `face_clusters_list` / `face_cluster_name` commands · 90 min
First file: `src-tauri/src/commands.rs` (add 3 Tauri commands per PRD §10 API surface). Then port `design-handoff/chronimage/screens_people.jsx` → `src/screens/PeopleScreen.tsx` via `/port-screen`.

Data can be stub-shaped (cluster rows synthesized from the catalog's `clusters` table — empty until real HDBSCAN runs). The point is to land the routing, the side-panel entry, and the typed `Cluster` model so the UI can iterate independently of inference.

Why: unblocks the second-largest missing PRD surface (§10). The UI layer is independent of the blocked `todo!()` inference paths.

### 3. Settings screen (PRD §14) · 75 min
First file: `design-handoff/chronimage/screens_settings.jsx` → `src/screens/SettingsScreen.tsx` via `/port-screen`.

Backing commands mostly exist or are trivial — `ai_model_list`, `budget_report`, `catalog_stats`. The "change model with re-index warning", "updater channel picker", and "nightly re-index toggle" need small Rust-side wiring but no new schema.

Why: leaves the onboarding + catalog + people + settings triad complete — every frame in the design that isn't the Detail overlay (Phase 2) or the Develop screen (Phase 3).

### 4. Wire `photo_views` recording · 30 min
First file: `src-tauri/src/commands.rs` (add `record_photo_view(photo_id)` command). Call from `src/screens/catalog/CatalogScreen.tsx` in the Detail overlay open handler.

Inserts into `photo_views` with `ON CONFLICT(photo_id) DO UPDATE SET last_viewed_at = excluded.last_viewed_at, view_count = view_count + 1`. The trigger on `photo_views` (migration `20260423000001`) already syncs `photos.last_viewed_at`, so the re-evaluator picks it up automatically.

Why: unlocks the fourth rediscovery album "Unseen in 2 years" (seed it in `catalog/rediscovery.rs` once the insertion path exists). One of the two user-visible payoffs of the `last_viewed_at` migration that shipped this session but has no writer yet.

### 5. Exit-criteria test scaffolds · 45 min
First files: `src-tauri/tests/phase_1_face_clustering.rs`, `src-tauri/tests/phase_1_catalog_size.rs`, `tests/e2e/phase-1-source-cleanup.spec.ts`.

PRD lists 8 exit-criteria test files (see § Exit criteria). Most don't exist yet. Create the empty shells with `#[ignore]` or `.skip()` markers and a `// TODO(cc): drive the 100-photo fixture through cleanup dry-run → execute → SHA256 post-check` comment. Phase 1 can't exit until these are green, so having the skeleton visible forces the remaining work to be concrete.

Why: makes the "how do we know Phase 1 is done?" question answerable with `cargo test --ignored` + `playwright test --grep phase-1`.

## Deferred (known-blocked)

- Real RetinaFace / ArcFace / HDBSCAN / llama.cpp inference — blocked on end-to-end model-download smoke (next-next session)
- Phase 1 performance tests (100k import throughput, 200k search latency, 5k RAW+JPG pair F1, 8h stress) — need real fixtures
- `photo_album_membership` schema — Phase 2 per ADR 0001
