# Checkpoint · Phase 1 live-data + UX · 2026-04-19

## Branch
`feature/phase1-live-data`

## What landed since Phase 0 exit

### Phase 1 data layer (PR #12, merged)
- `sha2` streaming hash — real SHA256 in `import/hash.rs`
- `AppState { pool: SqlitePool }` managed via `app.manage()` in `main.rs`
- `catalog::seed::seed_default_smart_albums()` — 12 system albums seeded on first run
- Real import pipeline (`import/pipeline.rs`): scan → hash → insert photos + source_copies → emit `chronimage://import-progress` events
- Tauri commands: `list_albums`, `list_photos`, `list_sources`
- TanStack Query hooks: `useAlbums`, `usePhotos` (infinite), `useSources`
- Catalog screens migrated from static fixtures to live query hooks

### Phase 1 feature iteration (PR #13 + ongoing, `feature/phase1-live-data`)
- `create_source` Tauri command + `useCreateSource` mutation
- `start_import` + `list_imports` commands + hooks
- `OnboardScreen`: functional folder picker via `tauri-plugin-dialog`, creates source, triggers import, live progress bar with ETA, finished-import toasts, "Open Catalog" button
- `CatalogScreen`:
  - Virtual photo grid with `@tanstack/react-virtual` (row virtualizer, ResizeObserver column count)
  - **Detail overlay**: double-click any photo → full-screen detail view with filmstrip, EXIF summary, AI tag chips, arrow-key nav, Escape to close
  - Rediscovery rows: `ON THIS DAY` + `UNSEEN · WORTH ANOTHER LOOK` (horizontal scroll strips above grid, hidden when empty)
- `on_this_day` + `unseen_photos` Tauri commands + hooks
- `cleanup_dry_run` Tauri command: returns per-source reclaimable bytes (SHA256-verified copies that have a backup elsewhere); safety: never flags sole copy
- `OnboardScreen` cleanup section: shows reclaimable GB per source with per-source breakdown; "Clean up" button stubbed for Phase 1b execute step

## Test counts
- **Rust**: 54 tests (commands, catalog, import, entitlements, error, util)
- **TypeScript**: 33 tests (4 files: app shell, ui state, invoke wrappers, query hooks)

## Next highest-value items (priority order)

### Core MVP
1. **`cleanup_execute`** — two-step deletion with confirm token + SHA256 re-verify + free-space check. UI: confirmation dialog with item list + "I understand" checkbox.
2. **Smart album rule engine** — evaluate `smart_albums.rule_json` against photos; re-run on new imports. Start with rules: `tag`, `is_raw`, `aesthetic_score_gte`, `captured_year`, `source_kind`.
3. **`list_photos` album filter** — `list_photos(album_id?)` so the catalog grid filters by album selection in the side panel.

### AI layer (spawn `ai-wrangler` agent)
4. `src-tauri/src/ai/budget.rs` — VRAM detection (DXGI/WMI), picks model variants
5. `src-tauri/src/ai/siglip.rs` — image + text encoders via `ort` (DirectML)
6. `src-tauri/src/ai/aesthetic.rs` — NIMA score (feeds rediscovery)
7. `src-tauri/src/ai/faces.rs` — RetinaFace + ArcFace
8. `src-tauri/src/ai/cluster.rs` — HDBSCAN face clusters

### UX
9. **5-step onboarding port** — full `screens_onboard.jsx` design (Welcome · Sources · Import · Models · People-naming). Current onboard is a single functional page; design has a stepper.
10. **Album filter in catalog** — side panel album click actually filters the photo grid.
11. **Search** — SigLIP text encoder → sqlite-vec k-NN (stub today; real in Phase 1 AI week).

### Source connectors
12. **iCloud-for-Windows folder scanner** — special-case `%USERPROFILE%\Pictures\iCloud Photos\Downloads`
13. **Google Photos OAuth + Takeout** — Library API wrapper + zip reader

## Notes
- `cleanup_execute` must be gated behind `CleanupPlan` plan_id (server-side token) + 2× free-space check. Do not implement without the full safety chain.
- `smart_albums.rule_json` spec lives at `docs/adr/0001-smart-album-rules.md` (to be written before implementing the rule engine).
- `tauri.conf.json` has placeholder updater pubkey — generate before any release workflow.
- `src-tauri/Cargo.lock` not yet committed to git.
- `models/` directory gitignored; first-run downloader not yet implemented.
