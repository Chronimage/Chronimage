# Next session · Phase 1 week 4

Queued on local branch `fix/model-urls` (2 commits, unpushed):
- `3e6d6af` URL fixes — siglip-2 / nima / moondream2 repo paths corrected
- `4b27ba6` chore — drop rust pre-push hook, rewrite vite `manualChunks` (332 KB → 4 cache-friendly chunks)

On develop: 193 Rust lib tests + 64 frontend, SCRFD filename fix (PR #24), FE community names (#23), backend community catalogue (#22).

## Starts (in order)

### 1. Ship the queued commits + smoke-test Download-all · 30 min
```bash
git push -u origin fix/model-urls
gh pr create --base develop --head fix/model-urls \
  --title "fix(ai): correct model urls; trim pre-push hook; rewrite vite chunks"
```
Then clean the stale models dir and smoke-test:
```bash
rm -rf "$LOCALAPPDATA/app.chronimage.desktop/models"
pnpm tauri dev
# Onboarding → Models → Download all
```
All 5 models must land end-to-end. If any fails, `extract_from_zip` now lists archive contents in the error.

Why: closes the smoke-test loop from tonight. Every downstream item assumes models exist on disk.

### 2. Lock real SHA256s · 30 min (after #1 confirms downloads)
First file: `src-tauri/src/ai/download.rs`. Replace each `sha256: "tbd"` with the real hex hash from:
```bash
cd "$LOCALAPPDATA/app.chronimage.desktop/models"
for f in *.onnx *.gguf; do sha256sum "$f"; done
```
For `det_10g.onnx` / `w600k_r50.onnx` the hash is the post-extract ONNX bytes, not the zip. Add test `known_model_hashes_are_hex_64` asserting no entry still says `"tbd"`.

Why: `"tbd"` disables the MITM check entirely. Must be locked before v0.1.

### 3. First real inference: SCRFD-10g preprocess + decode · 90 min
First file: `src-tauri/src/ai/faces.rs`, at the `TODO(cc): scrfd inference` marker in `detect_faces`.

Steps: resize+letterbox → 640×640 RGB f32, normalise `(x − 127.5) / 128.0`, `session.run([pixel_values])`, decode 3-scale anchor outputs (stride 8/16/32), NMS IoU 0.45 + conf 0.5, map boxes back to original coords. Keep 5-point landmarks in `FaceBox` for later ArcFace alignment.

Add tiny public-domain group photo under `tests/fixtures/face-detect/` and an `#[ignore]` integration test that drives the real session and asserts `N > 0` faces.

Why: first `todo!()` → live transition of Phase 1. Populates `faces` table so PeopleScreen's `face_clusters_list` returns real data.

### 4. Fill `phase_1_catalog_size` exit test · 45 min
First file: `src-tauri/tests/phase_1_catalog_size.rs`. Drop `#[ignore]`, replace `unimplemented!()` with real code: generate 10k 100×100 JPEGs via `image::save_buffer` (~30 MB), import via pipeline, assert `catalog.db / fixture_bytes ≤ 0.02`.

Why: cheapest of the 8 exit scaffolds. One `.skip()` → passing proves the pattern; seven left.

### 5. Wire `record_photo_view` from Detail overlay · 20 min
First file: `src/screens/catalog/CatalogScreen.tsx`. Find the Detail overlay open handler; add `useRecordPhotoView().mutate(photoId)` on open.

Why: command landed in PR #16 but nothing calls it. Until it fires, `last_viewed_at` stays NULL and "Unseen in 2 years" can't distinguish never-viewed from 3y-ago.

## Bundle + push discipline

CLAUDE.md now says: **accumulate meaningful work before pushing.** If #1 ships clean, stack #2 + #4 + #5 in one branch and push as a single "phase-1 week-4 plumbing" PR rather than four tiny ones. #3 deserves its own PR (review surface is larger).

## Deferred (known-blocked)

- ArcFace real inference (needs #3 first + landmark-based 112×112 alignment)
- HDBSCAN real clustering (chained: needs ArcFace flowing)
- Moondream2 llama.cpp sidecar + mmproj companion download
- Remaining 7 exit tests (fixtures missing)
- Settings "change model" + tauri-plugin-store persistence
