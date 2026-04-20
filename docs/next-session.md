# Next session · Phase 1 week 4 → week 5

Queued on local `feature/phase1-week4-plumbing` (1 commit, unpushed): `63cd55a` — SHA256 locks, `phase_1_catalog_size` exit test, `record_photo_view` wired from Detail overlay, pipeline stage-4 SigLIP filename fix, `CHRONIMAGE_MODELS_DIR` test override. **194** lib tests green in 1.30 s (was hanging >60 s).

On develop: PR #25 landed URL fixes + lefthook/vite cleanup.

## Starts (in order)

### 1. Ship the queued week-4 bundle · 15 min
```bash
git push -u origin feature/phase1-week4-plumbing
gh pr create --base develop --head feature/phase1-week4-plumbing \
  --title "feat(ai,catalog,ui): lock model sha256s, catalog-size exit test, view tracking"
```
CI should pass first push — full gauntlet already green locally. Merge when green.

Why: one clean commit already bundled and tested. Pushing is cheap; sitting on it risks drift.

### 2. SCRFD-10g real inference · 90 min
First file: `src-tauri/src/ai/faces.rs`, the `detect_faces` stub at the `TODO(cc)` marker.

Preprocess → 640×640 RGB f32, normalise `(x − 127.5) / 128.0`. `session.run([pixel_values])`. Decode 3-scale anchor outputs (stride 8/16/32), NMS IoU 0.45 + conf 0.5, map bboxes back to original image coords. Keep 5-point landmarks on `FaceBox`.

Add a tiny public-domain group photo under `tests/fixtures/face-detect/` and one `#[ignore]`d integration test that drives the real session and asserts `N > 0` faces.

Why: first `todo!()` → live transition of Phase 1. Starts populating `faces` table so PeopleScreen's `face_clusters_list` returns real data. All of §10 UI is currently showing empty-state forever.

### 3. ArcFace real inference · 60 min (needs #2)
First file: `src-tauri/src/ai/faces.rs`, `embed_face` stub.

112×112 alignment via 5-point landmarks (standard ArcFace affine matrix), RGB f32 normalised to [−1, 1], run ArcFace session, L2-normalise the 512-dim output. `FaceBox.landmarks` already populated by #2.

Why: without this, `faces` table has bboxes but no embedding vectors — HDBSCAN can't cluster. This is the straight-line continuation of #2.

### 4. Wire stage-5 face detection + embedding in import pipeline · 60 min (needs #2 + #3)
First file: `src-tauri/src/import/pipeline.rs` — after stage-4 AI enrichment, add a stage that calls `faces::detect_faces` + `faces::embed_face` per photo, writes to `faces` table. Gate behind `CHRONIMAGE_MODELS_DIR` absence check (tests skip automatically via the week-4 override).

Why: until pipeline actually writes `faces` rows, the real inference paths from #2/#3 only run from manual tests. This is the last wire to close the face-detection user value loop.

### 5. `phase_1_face_clustering` exit test · 45 min (after #2–#4)
First file: `src-tauri/tests/phase_1_face_clustering.rs` — replace `unimplemented!()` with a real fixture-driven test: labelled group photo, import, HDBSCAN, assert F1 ≥ 0.95 for the primary cluster.

Why: second of 8 exit criteria → passing. Proves the end-to-end face path works.

## Push discipline (per CLAUDE.md)

- #1 ships standalone (already tested, done).
- #2 + #3 + #4 = one "SCRFD + ArcFace real inference + pipeline wire" PR (cohesive review unit, ~3 hours of work).
- #5 gets its own PR since it depends on real models + fixtures existing.

## Deferred (known-blocked)

- HDBSCAN real clustering (chain: needs #3 embeddings flowing)
- Moondream2 llama.cpp sidecar + mmproj companion download (separate effort; vision-language protocol design)
- Remaining 6 exit tests (fixtures missing; do after #5 lands the pattern)
- Settings "change model" + `tauri-plugin-store` persistence
