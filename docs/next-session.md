# Next session · Phase 1 exit-criteria wrap-up

Branch: `develop` (up to date with origin after PR #34 merged `feature/phase1-week5-followup`). Four Rust integration tests that carry measurable Phase 1 NFRs are green. What's left is scaffolding the three Playwright e2e specs, wiring a real stress loop, and closing the last functional gaps (CI model packaging, Google Photos). Starts are independent — pick one per sitting.

## Scoreboard on develop (2026-04-21, post-merge)

| Test | Status | Measured | Threshold |
|---|---|---|---|
| `phase_1_raw_jpg_pair` | ✅ | precision 1.0000 (5 000 pairs) | ≥ 0.995 |
| `phase_1_catalog_size` | ✅ | ratio 0.00202 | ≤ 0.02 |
| `phase_1_face_clustering` | ✅ | F1 1.0000 (200 LFW, prealigned) | ≥ 0.95 |
| `phase_1_search_latency` | ✅ | **607 ms** p95 (int8 vec0 KNN, 200 k) | ≤ 750 ms (PRD revised) |
| `phase_1_stress` | ❌ | `unimplemented!()` | — |
| 3 × Phase 1 e2e specs | ❌ | all `.skip()` | — |

PRD `docs/prds/phase-1.md` § Exit criteria reflects this. Functional Phase 1 scope is complete; remaining work is fixtures + CI plumbing + one connector.

## Five concrete starts — pick one

### 1. CI packaging job for bundled models · ~2 h (highest leverage)
`tauri build` currently can't produce a signed .msi on a fresh runner because `models/` is gitignored. Add a workflow step that fetches the pinned SHA256s from [src-tauri/src/ai/download.rs](src-tauri/src/ai/download.rs) `KNOWN_MODELS` before invoking `pnpm tauri build`. Unblocks the first beta release.

**First action:** `ls .github/workflows/` — read the nightly workflow as a template, then create `.github/workflows/release-build.yml` with a `Fetch bundled models` step that loops over the 6 bundled entries (siglip2 image+text+tokenizer, NIMA, SCRFD, ArcFace — ~950 MB total) verifying SHAs into `src-tauri/models/`.

### 2. `phase_1_stress` 8-hour nightly loop · ~3 h
[phase_1_stress.rs](src-tauri/tests/phase_1_stress.rs) is a 10-line `unimplemented!()`. PRD asks for an 8 h loop over a 50 k synthetic fixture running import + search + face rebuilds, asserting no panic hook fires and peak RSS < 2 GB.

**First action:** edit [phase_1_stress.rs:8](src-tauri/tests/phase_1_stress.rs#L8). Reuse `synthesize_jpeg` from [phase_1_catalog_size.rs:33](src-tauri/tests/phase_1_catalog_size.rs#L33), `run_pipeline_headless`, and the `sysinfo` crate for RSS sampling. `#[ignore]`-gated so zero CI cost to land the skeleton; nightly workflow picks it up via `--ignored`.

### 3. Google Photos OAuth2 source connector · ~4 h (biggest remaining connector gap)
Local + iCloud-folder + USB sources are in place; Google Photos Library API is the last major onboarding piece. Requires real OAuth2 + token storage.

**First action:** create [src-tauri/src/sources/google_photos.rs](src-tauri/src/sources/google_photos.rs). Gate all reqwest calls behind a `user_initiated_google_photos_*` prefix (CLAUDE.md § Security). Store tokens in Windows Credential Manager via the `keyring` crate.

### 4. Wire `phase-1-import-throughput.spec.ts` e2e · ~2 h
[tests/e2e/phase-1-import-throughput.spec.ts](tests/e2e/phase-1-import-throughput.spec.ts) is `.skip()`'d. Rust integration tests already prove throughput; the e2e's value is validating end-user-browser progress surfaces under real load.

**First action:** add a `cfg(debug_assertions)`-gated Tauri command `__test_generate_fixture(count)` that drops `count` synthetic JPEGs into a tempdir (reuses `synthesize_jpeg`). Un-`.skip()` the spec; smoke at 10 k locally, 100 k nightly.

### 5. Phase 2 spike — sqlite-vec 0.1.10+ diskann evaluation · ~90 min
`docs/prds/phase-2.md` § 9 carries this as a follow-up. Worth 90 min to confirm 0.1.10+ ships diskann on Windows before committing it to Phase 2 scope.

**First action:** `cargo update -p sqlite-vec --precise <latest>` in a scratch worktree. Add a `diskann`-variant `vec_photo_embeddings_diskann` table, re-run `phase_1_search_latency`, capture p95 + index-build time in a new ADR under [docs/adr/](docs/adr/).

## Recommended order

If you can only do one: **#1** — it blocks the first signed beta .msi. If two: **#1 + #2**. `#5` is a useful spike; `#3` can wait until onboarding UX stabilises.
