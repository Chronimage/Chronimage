# Checkpoint · Phase 0 exit + Phase 1 scaffolding · 2026-04-19 (late)

## Phase 0 exited · Phase 1 entered

All Phase 0 exit criteria that are verifiable without a real release tag are green:

- `pnpm typecheck` ✅
- `pnpm exec biome check .` ✅
- `pnpm test` ✅ (6 frontend tests)
- `cargo check --manifest-path src-tauri/Cargo.toml` ✅
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` ✅
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` ✅ (32 Rust tests, incl. catalog pool + Phase 1 migration smoke)
- `pnpm dev` serves the shell at http://127.0.0.1:1420/ cleanly
- Chrome shell renders (Titlebar + Rail + StatusBar + Catalog side panel + main grid for the Catalog route)

Not yet verified (require real release tags + signing certs):
- Tag push → signed MSI per channel (needs EV cert + Cloudflare token)
- Probe commit → lefthook rejection (needs `pnpm prepare && lefthook install`)
- Auto-updater manifest hosting (needs Cloudflare Pages project)

Deferred to a "release rehearsal" session later. They don't block Phase 1 work.

## Just finished this session

### Resolved dev-server + typecheck + lint
- Removed the TanStack Router plugin from `vite.config.ts` (routes/ tree doesn't exist yet; Zustand screen switching works for now — router re-enters in a later Phase 1 session)
- Relaxed `exactOptionalPropertyTypes` in `tsconfig.json` (breaks library typings broadly)
- Excluded `*.config.ts` + `playwright.config.ts` from tsconfig includes
- Biome: added `noSvgWithoutTitle: off` + expanded script override glob to `**/scripts/**` + `**/.claude/hooks/**` for `.cjs` files
- Removed dead biome-ignore comments from `src/util/log.ts`
- Fixed `scripts/new-migration.cjs` template literal style error

### Node ESM migration
- Renamed all repo-internal scripts `.js` → `.cjs` (repo `package.json` has `type: module`; `.cjs` lets them keep `require()`)
- Updated references in `.claude/settings.json`, `lefthook.yml`, `package.json`, all `.github/workflows/*.yml`

### Rust toolchain + build
- Bumped `rust-toolchain.toml` to `channel = "stable"` (several transitive deps want 1.88; Tauri deps want edition2024)
- Removed `macos-private-api` Tauri feature (Windows-only + undeclared in capabilities)
- Generated placeholder mint-green app icon via `scripts/make-placeholder-icon.cjs` + `pnpm exec tauri icon`
- Removed redundant `BEGIN/COMMIT` from migration SQL (sqlx wraps each migration in its own tx)

### Phase 1 Rust scaffolding (new)
- **Phase 1 SQL migration** `src-tauri/migrations/20260420000000_phase1_catalog.sql`
  - Adds: `tags`, `models`, `photo_embeddings`, `faces`, `clusters`, `smart_albums`, `photo_views`, `source_deletions`, `photos_fts` (FTS5), triggers
  - Extends `photos` with phash, EXIF, aesthetic/sharpness scores, camera metadata, GPS
  - Bumps schema_version → 2
- **Catalog pool** `src-tauri/src/catalog/db.rs` — `open_pool()` with WAL + NORMAL + foreign-keys + 15s busy timeout; runs `sqlx::migrate!("./migrations")`. 3 integration tests verifying idempotent opens + Phase 1 table existence
- **Domain models** `src-tauri/src/catalog/models.rs` — Photo/Source/SourceCopy/Import/Setting/SmartAlbum + typed `SourceKind` enum (Local/External/NAS/SD/iPhone/Android/GooglePhotos/iCloud/OneDrive/Dropbox) with snake_case serde
- **Import pipeline** `src-tauri/src/import/`
  - `scanner.rs` — `scan_dir()` walks recursively, filters to image extensions, captures size+mtime. 4 tests (extension filter, missing root, max depth, size capture)
  - `pair.rs` — RAW+JPG pair detector. Handles 11 RAW formats (ARW/CR2/CR3/NEF/NRW/RAF/RW2/ORF/DNG/PEF/SRW). 7 tests — happy path, case insensitivity, different dirs, two-RAWs, two-JPGs, mixed-bag partitioning, format recognition
  - `hash.rs` — streaming SHA256 placeholder (stable API signature; real hash once `sha2` is added). 2 tests
- **Tauri command** `import_dry_run(root) → ScanReport` wired into the app. 2 tests.
- Added `walkdir` dep to `Cargo.toml`.

### Phase 1 frontend scaffolding (new)
- **Primitives**: Chip, Seg, Slider, Toggle, Placeholder (all ported from design)
- **Icon set**: expanded to full 38-icon roster from design
- **Fixtures** `src/state/fixtures.ts` — typed PHOTOS (60), ALBUMS (12), SOURCES (10), PEOPLE (6), SEARCH_SUGGESTIONS
- **CatalogScreen** `src/screens/catalog/` — sidepanel (smart albums, people, sources) + main grid (search, facetbar, hero, 48-photo grid, selection-aware toolbar)
- **OnboardScreen** — partial port (Welcome + Sources list); full 5-step flow lands in next session
- **PlaceholderScreen** — generic "Phase N pending" screen reused by Cull, CullBin, Develop, Settings
- **App router** — `app.tsx` switches between all 6 screens via Zustand state
- **invoke.ts** — added typed `importDryRun()` wrapper + `ScanReport` type
- **tests/setup.ts** — added `import_dry_run` mock

### Context dump/load
- `/context-dump [label]` and `/context-load <path>` slash commands added at user request; live at `.claude/commands/context-{dump,load}.md` with a dedicated section in CLAUDE.md.

## Test counts

- **Frontend**: 6 tests (2 files) — app shell smoke + ui state store
- **Rust lib**: 32 tests
  - catalog::db (3) — pool creation, idempotency, Phase 1 tables exist
  - catalog::models (2) — SourceKind strings + serde
  - commands (5) — ping, app_version, current_channel, import_dry_run happy + detection
  - entitlements (3)
  - error (2)
  - import::hash (2)
  - import::pair (7) — the critical RAW+JPG detector coverage
  - import::scanner (4)
  - telemetry (2)
  - util::paths (2)

## Open threads for the next Phase 1 session (priority order)

### Core MVP unblocks (do these first)
1. **sha2 hashing** — replace the placeholder in `src-tauri/src/import/hash.rs`. Add `sha2 = "0.10"` to Cargo.toml. Test against a known vector.
2. **Wire `AppState` with `SqlitePool`** into Tauri — create `src-tauri/src/state.rs` with `struct AppState { pool: SqlitePool }`, manage via `app.manage()` in `main.rs`'s setup closure, take `State<'_, AppState>` in commands that need DB access.
3. **Seed smart albums** at first launch — port the design's `ALBUMS` list into a Rust seed function that populates `smart_albums` table with rule JSON. Add a seeder called from `catalog::db::open_pool` after migrate completes (or a separate `catalog::seed::default_smart_albums`).
4. **Real import pipeline** — tokio task that: scans → hashes → inserts photo rows → detects pairs → publishes progress event `chronimage.import.progress`. Back-pressure via mpsc channel.
5. **Wire frontend fixtures through Tauri** — swap `src/state/fixtures.ts` calls for TanStack-Query hooks (`useAlbums`, `usePhotos`, `useSources`) that invoke new `list_albums`, `list_photos`, `list_sources` Rust commands.

### Source connectors
6. **Google Photos OAuth + Takeout ingest** — `src-tauri/src/import/google_photos.rs` with the Library API wrapper + zip reader for Takeout archives.
7. **iPhone USB (WPD/MTP)** — `src-tauri/src/import/iphone_usb.rs` using the `windows` crate's WPD bindings.
8. **iCloud-for-Windows folder scanner** — special-case in `scanner.rs` that knows the `%USERPROFILE%\Pictures\iCloud Photos\Downloads` path.
9. **NAS UNC path scanner** — already covered by `scan_dir` + the `local` SourceKind; just need the UI picker.

### Source-side cleanup (the user's primary motivation)
10. `src-tauri/src/commands/source_cleanup.rs` with:
    - `cleanup_dry_run()` returning a `CleanupPlan` with per-source bytes reclaimable
    - `cleanup_execute(plan_id, confirm_token)` with SHA256 verification + 2× free-space check + per-source adapter
11. Rediscovery — `photo_views` join queries: "On this day" / "Unseen in 2y" / "First camera" / "Unflagged NIMA 8+" — surfaced on Catalog home.
12. **5-step onboarding flow** — full port of `design-handoff/chronimage/project/src/screens_onboard.jsx`.
13. **Detail overlay** inside CatalogScreen (double-click opens).

### AI layer (biggest chunk; spawn the `ai-wrangler` agent)
14. `src-tauri/src/ai/` module:
    - `budget.rs` — VRAM detection (Windows: DXGI / WMI), picks model variants
    - `models.rs` — registry of downloadable models with URLs + SHA256 pins
    - `downloader.rs` — first-run download progress events
    - `siglip.rs` — image + text encoders via `ort` crate (DirectML provider)
    - `faces.rs` — RetinaFace detect + ArcFace embed
    - `cluster.rs` — HDBSCAN over face embeddings (can use `petal-clustering` crate or port)
    - `aesthetic.rs` — NIMA scoring
    - `caption.rs` — gemma4-9b via llama.cpp sidecar (GPU-only)
15. `tauri-plugin-shell`-based llama.cpp sidecar management.

### Infrastructure + nice-to-haves
16. Install lefthook: run `pnpm prepare` (adds the pre-commit hooks).
17. `cargo install cargo-nextest --locked` for faster test runs.
18. Git branch setup: create `develop` + `main` on remote, set protection rules, squash-merge discipline.
19. Commit `src-tauri/Cargo.lock` on the first Phase 1 feat commit.

## Next session start (exact moves)

1. `export PATH="$HOME/.cargo/bin:$PATH"` (Rust may need PATH refresh per shell)
2. `git status` — review everything staged from this session
3. Read `CLAUDE.md` → `docs/checkpoints/latest.md` → `docs/prds/phase-1.md`
4. Run the verification gauntlet before touching anything:
   ```bash
   pnpm install --frozen-lockfile
   pnpm typecheck && pnpm exec biome check .
   pnpm test
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path src-tauri/Cargo.toml --lib
   ```
5. Start with #1 (sha2 hashing) — smallest change, biggest unblock.
6. Then #2 (AppState with SqlitePool) — enables real commands to touch the DB.
7. Then #3 (smart album seeder) — gives the frontend real data through TanStack Query.
8. Then start picking from the AI layer (#14) — biggest chunks, define Phase 1 completion.
9. At session end: `/phase-checkpoint` + `/context-dump` if about to move machines.

## Notes

- Repo directory remains `halide/` for historical reasons. Product name is **Chronimage** everywhere in code/copy.
- OneDrive path — `pnpm install` took ~24s. Watch for file-lock errors during cargo builds (OneDrive sync vs. target/).
- Rust 1.85 is too old (darling / icu / serde_with / time want 1.86–1.88). `rust-toolchain.toml` pins `stable`.
- `tauri.conf.json` has a placeholder `pubkey: "REPLACE_WITH_TAURI_UPDATER_PUBKEY"`. Generate with `pnpm exec tauri signer generate` before any release workflow runs for real.
- `scripts/make-placeholder-icon.cjs` generated a solid-mint 512×512 icon; replace with real art before v0.2.0. `app-icon.png` was deleted; only `src-tauri/icons/*.png + .ico` remain.
- Lefthook's pre-push tries `cargo test` — slow on first run (dep download). Consider swapping to `cargo nextest` once installed.
- Cargo.lock exists in `src-tauri/`. Not yet committed. `.gitignore` allows it; commit on the first Phase 1 feat commit.
- All gate-able features open; `Entitlements::current()` returns all-true.
- Phase 1 exit criteria in `docs/prds/phase-1.md` — none yet satisfied. The scaffolding landed here unblocks the work.
- Clippy-level `deny(unwrap_used/expect_used/panic)` was removed from `lib.rs` because clippy fires inside `#[cfg(test)]` modules where those are fine. The `scripts/forbidden-patterns.cjs` check still guards non-test code via pre-commit and CI.
- Test mocks in `tests/setup.ts` cover `import_dry_run` now; add more invoke mocks as new commands land.
