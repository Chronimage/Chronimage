# Next session · Phase 1 — exit the phase

Branch: `develop` (post PR #35, merged as `ed5b011`). The full Phase 1 frontend is now real-data backed — catalog, people, onboarding, settings all off fixtures; five-section inspector, duplicates panel, people drill-down, real thumbnails, two new rediscovery rows, updater-channel picker. Coverage gates re-passing (84 tests · src/state 90.5%). What remains to close Phase 1: one Rust stress stub, three e2e specs, and a first beta tag to exercise the packaging workflow.

## Exit-criteria scoreboard

| Test | Status |
|---|---|
| `phase_1_raw_jpg_pair` | ✅ 1.0000 precision (5 000 pairs) |
| `phase_1_catalog_size` | ✅ 0.00202 ratio |
| `phase_1_face_clustering` | ✅ F1 1.0000 (LFW) |
| `phase_1_search_latency` | ✅ 607 ms p95 (int8 vec0 KNN) |
| `phase_1_stress` | ❌ `unimplemented!()` — last Rust stub |
| `phase-1-import-throughput.spec.ts` | 🟡 wired; skips unless `CHRONIMAGE_E2E_TAURI=1` |
| `phase-1-source-cleanup.spec.ts` | ❌ top-level `test.skip()` |
| `phase-1-rediscovery.spec.ts` | ❌ top-level `test.skip()` |
| `phase-1-search-latency.spec.ts` | ❌ top-level `test.skip()` (Rust test carries the NFR) |

## Five starts — pick one

### 1. `phase_1_stress` 8-hour nightly loop · ~3 h
Last unclosed Rust integration test. `#[ignore]`-gated so zero CI cost; completes the Rust side of the exit matrix.
**First action:** edit [phase_1_stress.rs:13](src-tauri/tests/phase_1_stress.rs#L13). Reuse `synthesize_jpeg` + `run_pipeline_headless` helpers from the existing integration tests. Loop: import 50k synthetic photos, run search every 5 min, rebuild face clusters every 30 min, sample RSS via `sysinfo`. Assert: zero panics hooked, peak RSS < 2 GB, final pool free-list count monotone.

### 2. Seed + un-skip 3 e2e specs · ~4 h bundled
Three `test.skip()` specs remain. The pattern from import-throughput works for all three: debug-only Tauri commands seed the catalog, spec drives `page.evaluate(() => __TAURI__.core.invoke(...))`.
**First action:** add `__test_seed_catalog(rows)` + `__test_seed_source_copies(tmpdir, sha256)` next to `__test_generate_fixture` in [commands.rs](src-tauri/src/commands.rs) (cfg(debug_assertions) only). Then un-skip [phase-1-source-cleanup.spec.ts](tests/e2e/phase-1-source-cleanup.spec.ts), [phase-1-rediscovery.spec.ts](tests/e2e/phase-1-rediscovery.spec.ts), [phase-1-search-latency.spec.ts](tests/e2e/phase-1-search-latency.spec.ts) under the same `CHRONIMAGE_E2E_TAURI=1` gate.

### 3. Install Windows 11 SDK locally · ~10 min (unblocks 4+5) · user-driven
Session-to-session blocker: `pnpm tauri dev` can't link without `kernel32.lib` — VS 2026 has MSVC but no Windows SDK. CI has its own SDK so PR #35 merged fine, but the next session should be able to run the app end-to-end locally.
**First action:** open Visual Studio Installer → Modify VS 2026 Professional → Individual components → tick **Windows 11 SDK (10.0.22621.0)** → install. Alternatively: `winget install --id Microsoft.WindowsSDK.10.0.22621 --silent`. Verify with `cargo check --manifest-path src-tauri/Cargo.toml` → no linker error.

### 4. Live Playwright screenshot pass across Phase 1 surfaces · ~2 h (needs #3)
Now that every screen is wired to real data, capture visual goldens while nothing is mid-flight. Covers catalog grid, detail inspector, people drill-down, duplicates panel, onboarding wizard, settings. Becomes the regression baseline for Phase 2.
**First action:** with the app running via `pnpm tauri dev`, write [tests/e2e/phase-1-visual.spec.ts](tests/e2e/phase-1-visual.spec.ts) that drives through every routed screen and `await expect(page).toHaveScreenshot()`. Store goldens under `tests/fixtures/visual/phase-1/`.

### 5. First beta tag + packaging smoke · ~45 min
Cheapest way to confirm the `Fetch bundled models` release step works before it matters at beta time. Signed MSI from `develop`.
**First action:** `git tag -s v0.1.0-beta.1 -m 'first beta smoke' && git push origin v0.1.0-beta.1` → `gh run watch <release-beta run id>` → download the MSI artifact → unzip, verify `models/bundled/` has the 6 pinned files with matching SHA256s.

## Recommended order

**#1** is safe, high-leverage, closes the last Rust exit. **#3** first if you'll be working locally tomorrow — it's 10 min that unblocks #4 permanently. **#2** buys the final four PRD boxes. **#4** captures the visual-regression baseline while the UI is freshly assembled. **#5** is a 45-min fire-and-forget verification. Defer nothing to Phase 2 prep — close Phase 1 first.

## Deferred follow-ups (log, not blockers)

- Face-crop thumbnails for `PeopleScreen` + sidebar covers (new `get_face_crop_thumbnail` command; ~2 h).
- `refresh_smart_albums` manual-trigger UI (backend exists, no frontend consumer).
- Click-to-open-detail on `DuplicatesPanel` thumbnails (currently read-only review).
- Rotate the PAT used in the 2026-04-21 session transcript — it's leaked.
