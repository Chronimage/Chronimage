# Next session · Phase 1 week 5 — ship + harden

Queued on local `feature/phase1-week4-plumbing` (**9 commits, unpushed**):

1. `63cd55a` sha locks + catalog-size exit test + record_photo_view
2. `9528c8a` real scrfd + arcface + pipeline stage-5 + face-clustering exit test
3. `a6c84ca` bundled defaults + hdbscan + settings picker modal
4. `24d4ab0` sync manage(AppState) race fix
5. `75a52bd` sqlite-vec auto-extension (vec0 + vec_face_embeddings)
6. `d41b67e` tokio reactor context for spawn_reevaluator
7. `398bef4` slim AppState (drop unused faces/caption fields → fast boot)
8. `1e0e90c` CI: prebuilt cargo-audit + cargo-mutants (saves ~2 min/run)
9. `25a5b7b` dependabot bundle: vite 8, jsdom 29, lefthook 2, @types/node 25, tanstack 5.99

`pnpm tauri dev` boots in ≈1 s and `Add local folder` works again.

## Starts (in order)

### 1. Push the bundle + open PR · 15 min
```bash
git push -u origin feature/phase1-week4-plumbing
gh pr create --base develop --head feature/phase1-week4-plumbing
```
CI will exercise every gate (the 9 commits compile, clippy and test clean locally). When merged, dependabot #17–#21 auto-close. **Nothing downstream is safe to start until this lands** — we'd be stacking on uncommitted-to-develop work.

### 2. Fix cache-save cancel class of failure · 10 min
First file: `.github/workflows/ci.yml`. Add to both `Swatinem/rust-cache@v2` uses (lines 84 and 112):

```yaml
  save-if: ${{ github.ref == 'refs/heads/develop' }}
```

PRs will read the cache but not attempt to save it, so `concurrency: cancel-in-progress` never kills a mid-tar upload. Develop pushes don't rapid-fire, so their saves complete cleanly. Tonight's failure (`Post Run Swatinem/rust-cache@v2 · The operation was canceled`) disappears.

### 3. Memoize FacesSession across pipeline runs · 45 min
First file: `src-tauri/src/ai/faces.rs` — add `pub fn global_faces_session() -> AppResult<&'static FacesSession>` guarded by `OnceLock<FacesSession>`. Then update `src-tauri/src/import/pipeline.rs` stage-5 to call `global_faces_session()` once instead of re-invoking `FacesSession::load(scrfd, arcface)` per import (currently ~2 s of ort init on every import batch). Bundled paths + user data dir checked once; failed load falls through to stub.

Why: stage-5 is now on every import's critical path. Re-loading 190 MB of ONNX for each import is the #1 avoidable latency in the pipeline.

### 4. Land one green face-detect exit-criterion test · 30 min
First file: `tests/fixtures/face-detect/group.jpg` — commit a tiny public-domain group photo (≤ 100 KB; Unsplash's CC0 collection or `picsum.photos` seed).

Then drop `#[ignore]` from `scrfd_detects_faces_in_real_group_photo` in `src-tauri/src/ai/faces.rs` (still gated by model-presence check — skips cleanly when bundled dir is absent). Assert `N ≥ 3` faces.

Why: we have real SCRFD + ArcFace + bundled models; the test was stubbed because the fixture didn't exist. Moves the first "real inference" exit test from `#[ignore]` → green → one more of the 8 PRD exit criteria closed.

### 5. Persist Settings tweaks via tauri-plugin-store · 60 min
First file: `src/state/ui.ts` — move `tweaks` (appName, culling thresholds, nightly re-index) from Zustand-in-memory into `tauri-plugin-store` reads/writes. Then extend to persist the per-feature model choice from `ModelPickerModal` so swaps survive restart.

Why: the `useState`/Zustand setup is in-memory only — user restarts lose everything. Plugin is already in `tauri.conf.json`; only the frontend wiring is missing.

## Deferred (known-blocked or low-ROI)

- Moondream2 mmproj companion download + llama.cpp sidecar binary + vision-language HTTP protocol (Phase 2 prereq).
- Remaining 5 PRD exit tests (need fixtures: 10k throughput, 200k search latency, 5k RAW+JPG pair F1, 100-photo cleanup, rediscovery dated fixture).
- CI packaging job: fetch bundled models from pinned S3/HF release + attach LICENSE files.
- User research: is a ~700 MB MSI acceptable? If not, ship a "slim" variant that first-run-downloads the bundled set.
- `advisory-db` caching for cargo-audit (~30 s gain; skip unless cargo-audit becomes a felt bottleneck post-prebuilt).

## Push discipline

- #1 ships standalone (already 9 commits; don't amend, just push).
- #2 is a 1-line YAML change — bundle with #3 + #4 + #5 as one "week-5 polish" PR so the reviewer sees the cache fix alongside the stage-5 memoization + exit-test landing.
