# Next session · Phase 1 week 4

On develop after PRs #22 (KNOWN_MODELS swap to community), #23 (FE model names), #24 (SCRFD filename fix). Pending commit on local develop: **URL fixes** for siglip-2 (`naflex` repo 401 → `224-ONNX` repo), nima (`Chronimage/models` 401 → `cromsc/nima-mobilenet-aesthetic`), moondream2 (`vikhyatk/moondream2` 404 → `moondream/moondream2-gguf`). Ship that as PR #25 first thing. 193 Rust lib tests + 64 frontend green.

## Starts (in order)

### 1. Ship the URL-fix commit + full-download smoke · 30 min
First command: `git status` — push the uncommitted `ai/download.rs` + `docs/adr/0002-model-download.md` changes as `fix(ai): correct siglip-2, nima, moondream2 repo urls`. Then `rm` the old failed files in `%LOCALAPPDATA%/app.chronimage.desktop/models/` and click "Download all" in Onboarding → Models.

All 5 models should now install end-to-end. If any fails, the extract error now lists archive contents — use that to debug.

Why: this closes the smoke-test loop the user hit tonight. Every other next-session item assumes models are actually present on disk.

### 2. Lock real SHA256s · 30 min (after #1 confirms downloads work)
First file: `src-tauri/src/ai/download.rs` — replace each `sha256: "tbd"` with the real hex hash.

After a successful "Download all" run:
```bash
cd "$LOCALAPPDATA/app.chronimage.desktop/models"
for f in *.onnx *.gguf; do echo "$f $(sha256sum $f)"; done
```
Copy into the specs. For `det_10g.onnx` and `w600k_r50.onnx` the hashes are the post-extract ONNX files, not the zip. Add an integration test `known_model_hashes_are_hex_64` asserting no entry still says `"tbd"`.

Why: `"tbd"` disables the MITM check entirely. Required before any v0.1 release.

### 3. First real inference path: SCRFD-10g preprocess + decode · 90 min (needs #1)
First file: `src-tauri/src/ai/faces.rs` — the `detect_faces` function at the `TODO(cc): scrfd inference` marker.

Steps: resize + letterbox to 640×640 RGB f32, normalise `(x − 127.5)/128.0`, `session.run([pixel_values])`, decode 3-scale anchor outputs (stride 8/16/32), NMS at IoU 0.45 + conf 0.5, map boxes back to original coords. Keep the 5-point landmarks in `FaceBox` for downstream ArcFace alignment.

Add a small fixture (1 public-domain group photo) under `tests/fixtures/face-detect/` and an `#[ignore]` integration test that drives the real SCRFD session and asserts N faces detected.

Why: first `todo!(…)` → live transition of Phase 1. Starts populating the `faces` table so `face_clusters_list` has non-empty data in PeopleScreen.

### 4. Fill `phase_1_catalog_size` exit test · 45 min
First file: `src-tauri/tests/phase_1_catalog_size.rs` — drop `#[ignore]`, replace `unimplemented!()` with real code.

Generate 10k tiny 100×100 JPEGs (`image::save_buffer` in a setup helper, ~30 MB total), import via the pipeline, assert catalog.db size / fixture bytes ≤ 0.02.

Why: cheapest of the 8 exit-criteria scaffolds — proves one `.skip()` → passing. One solved criterion leaves 7.

### 5. Wire `record_photo_view` from Detail overlay · 20 min
First file: `src/screens/catalog/CatalogScreen.tsx` — find the Detail overlay open handler. Add `useRecordPhotoView().mutate(photoId)` on open.

Why: command landed in PR #16 but is never called. Until it fires, `last_viewed_at` stays NULL across the catalog and "Unseen in 2 years" can't distinguish never-viewed from 3y-ago-viewed.

## Deferred (known-blocked)

- ArcFace real inference (chained after #3 — needs landmark-based 112×112 alignment)
- HDBSCAN real clustering (needs ArcFace embeddings flowing)
- Moondream2 llama.cpp sidecar + mmproj companion download (vision-language protocol design; see ADR 0002 open issues)
- Remaining 7 exit-criteria tests (fixtures don't exist yet)
- Settings "change model with re-index warning" + tauri-plugin-store persistence
