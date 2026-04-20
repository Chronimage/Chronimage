# Next session · Phase 1 week 3 → week 4

Landed on `feature/phase1-community-models` (this session): swapped `KNOWN_MODELS` to 5 community-licensed sources (SigLIP-2, NIMA, SCRFD-10g, ArcFace W600K R50, Moondream2), added zip-extract support for InsightFace's `buffalo_l.zip`, updated ADR 0002. First-run download drops ~6.6 GB → ~2.3 GB; no gated licenses. 193 Rust + 64 frontend tests green.

## Starts (in order)

### 1. Lock real SHA256s by downloading each model once · 30 min
First file: `src-tauri/src/ai/download.rs` — replace each `sha256: "tbd"` with the real hex hash.

Commands:
```bash
# each URL from KNOWN_MODELS, compute sha256
curl -L <url> -o /tmp/model.bin && sha256sum /tmp/model.bin
```

For the two buffalo_l.zip entries, hash the zip once and record the same value for both SCRFD + ArcFace specs (they resolve to the same URL). The download path hash-checks the post-extract ONNX, so also record the per-ONNX hash after extraction (easier: first-run extraction populates them, then lock).

Why highest leverage: `"tbd"` disables hash verification entirely — a MITM could swap a model file silently. Locking the hashes is required before any v0.1 release. One-hour action, permanent win.

### 2. Manual smoke of the swapped models + boot path · 45 min
First command: `pnpm tauri dev`. Then:
- Trigger model download from Settings or Onboarding (whichever surface invokes `download_models`).
- Verify the buffalo_l zip downloads once, gets extracted, ONNX files land in `%LOCALAPPDATA%/app.chronimage.desktop/models/`, temp zip is cleaned up.
- Check `ai_models_status()` reports 5 models, `installed: true` for whatever landed.
- Walk `/people` + `/settings` — AI Models list shows the new 5 names.

Why: zip-extract is new code on the critical path. Unit tests cover the extract function in isolation but not the end-to-end download → zip → extract → file-on-disk pipeline.

### 3. First real inference path: SCRFD-10g preprocessing + decode · 90 min (needs #1 + #2)
First file: `src-tauri/src/ai/faces.rs`, the `detect_faces` function at the `TODO(cc): retinaface inference` marker (rename to `TODO(cc): scrfd inference` while there).

Steps: resize+letterbox image to 640×640 RGB f32, normalise `(x - 127.5) / 128.0`, `session.run([pixel_values])`, decode 3-scale anchor outputs (stride 8/16/32), apply NMS at IoU 0.45 + conf 0.5, map bboxes back to original image coords. Include 5-point landmarks for ArcFace alignment later.

Why: first real `todo!(…)`→live transition of Phase 1. Unlocks the `faces` table actually populating, which in turn unlocks the face_clusters_list command returning non-empty data in PeopleScreen.

### 4. Fill `phase_1_catalog_size` exit test with a real fixture · 45 min
First file: `src-tauri/tests/phase_1_catalog_size.rs` — remove `#[ignore]`, replace `unimplemented!()` with real code. Pre-generate 10k tiny 100×100 JPEGs (~30 MB) via `image::save_buffer`, import via the pipeline, assert catalog.db / fixture_bytes ≤ 0.02.

Why: cheapest of the 8 exit-criteria scaffolds; proves the scaffold→passing path works. One green exit criterion leaves 7.

### 5. Wire `record_photo_view` from Detail overlay · 20 min
First file: `src/screens/catalog/CatalogScreen.tsx` — find the Detail overlay open handler. Call `useRecordPhotoView().mutate(photoId)` on open.

Why: command landed in PR #16 but nothing calls it yet. Until it fires, `last_viewed_at` stays NULL across the library and "Unseen in 2 years" can't distinguish never-viewed from 3y-ago-viewed.

## Deferred (known-blocked or large-scope)

- ArcFace W600K R50 real inference (blocked: needs #3 first + landmark-based 112×112 alignment)
- Moondream2 vision-language sidecar protocol (blocked: sidecar needs image-input path, not just text — caption.rs TODO explicitly notes this)
- HDBSCAN real clustering (blocked: needs ArcFace embeddings flowing)
- Remaining 7 exit-criteria tests (blocked: fixtures don't exist yet)
- SigLIP-2 `naflex` variable-aspect-ratio support (current path uses fixed 224 preprocess)
