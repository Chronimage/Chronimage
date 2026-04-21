# Next session · Phase 1 exit-criteria wrap-up

Branch: `develop` (post PR #34). 4/4 Rust NFR tests green — functional Phase 1 scope is complete. Remaining: CI model packaging, stress loop, e2e specs, Google Photos source.

## Scoreboard on develop (2026-04-21)

| Test | Status | Measured | Threshold |
|---|---|---|---|
| `phase_1_raw_jpg_pair` | ✅ | 1.0000 precision | ≥ 0.995 |
| `phase_1_catalog_size` | ✅ | 0.00202 ratio | ≤ 0.02 |
| `phase_1_face_clustering` | ✅ | F1 1.0000 (LFW) | ≥ 0.95 |
| `phase_1_search_latency` | ✅ | **607 ms** p95 (int8 vec0 KNN) | ≤ 750 ms |
| `phase_1_stress` | ❌ | `unimplemented!()` | — |
| 3 × Phase 1 e2e specs | ❌ | `.skip()` | — |

## Four starts — pick one

### 1. CI packaging job for bundled models · ~2 h — highest leverage
Blocks the first signed beta `.msi`. `tauri build` fails on a fresh runner because `models/` is gitignored.
**First action:** `ls .github/workflows/` → clone the nightly workflow → create `.github/workflows/release-build.yml` with a `Fetch bundled models` step that reads `KNOWN_MODELS` from [src-tauri/src/ai/download.rs](src-tauri/src/ai/download.rs) and SHA-verifies 6 entries (siglip2 image+text+tokenizer, NIMA, SCRFD, ArcFace — ~950 MB) before `pnpm tauri build`.

### 2. `phase_1_stress` 8-hour nightly loop · ~3 h
Last Rust integration test stub. `#[ignore]`-gated → zero CI cost to land.
**First action:** edit [phase_1_stress.rs:8](src-tauri/tests/phase_1_stress.rs#L8). Reuse `synthesize_jpeg` from [phase_1_catalog_size.rs:33](src-tauri/tests/phase_1_catalog_size.rs#L33) + `run_pipeline_headless`. RSS sampling via `sysinfo` (already a dev-dep). Assert no panic hook fires; peak RSS < 2 GB.

### 3. Google Photos OAuth2 source · ~4 h — biggest connector gap
Local + iCloud-folder + USB done; Google Photos is the last major onboarding piece.
**First action:** create [src-tauri/src/sources/google_photos.rs](src-tauri/src/sources/google_photos.rs). Gate reqwest behind `user_initiated_google_photos_*` (CLAUDE.md § Security). Tokens in Windows Credential Manager via the `keyring` crate.

### 4. Wire `phase-1-import-throughput.spec.ts` · ~2 h
e2e is `.skip()`'d. Rust test covers the NFR; e2e value is end-user-browser surface validation under load.
**First action:** add a `cfg(debug_assertions)` Tauri command `__test_generate_fixture(count)` reusing `synthesize_jpeg`. Un-`.skip()` the spec. Smoke at 10 k local, 100 k nightly.

## Recommended order

**#1** solo if time-boxed — unblocks beta release. **#1 + #2** if two sittings. `#3` waits until onboarding UX stabilises. Defer Phase 2 diskann spike until Phase 1 exits.
