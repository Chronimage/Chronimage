# Next session · Phase 1 week 5 → exit

Queued on local `feature/phase1-week5-polish` (5 commits, unpushed):

1. `931923a` memoised FacesSession + persisted Tweaks via `@tauri-apps/plugin-store`
2. `f92b6d3` scaffolded `phase_1_raw_jpg_pair.rs` + `phase_1_search_latency.rs` + `scripts/scan-raw-jpg-pairs.ps1`
3. `9c46fce` (LFS) `tests/fixtures/face-detect/group.jpg`
4. `121538b` UTF-8 BOM handling in manifest parser + script

**Already passing:** `phase_1_raw_jpg_pair` = **100 % precision** (5000/5000 across ARW + CR2 + CR3 + NEF + DNG) in 0.10 s.

**Fixture on disk** (gitignored, 200 GB): `tests/fixtures/raw-jpg-pairs/` with 10 001 files + `manifest.json`.

## Starts (in order)

### 1. Push + PR the week-5 branch · 15 min
```bash
git push -u origin feature/phase1-week5-polish
gh pr create --base develop --head feature/phase1-week5-polish
```
5 unpushed commits. The raw-jpg-pair test uses the repo-relative path fallback so CI can't exercise it (fixture is local-only), but CI will validate all 217 lib tests + the catalog-size + search-latency skeletons still compile.

### 2. Run the other 3 ready exit tests + land the numbers · 30 min
After the Tauri dev app is closed (or PID reaped; see previous session's `msedgewebview2` lingering-children note):

```bash
# Self-synthesising — no fixture needed
cargo test --manifest-path src-tauri/Cargo.toml \
  --test phase_1_catalog_size -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml \
  --test phase_1_search_latency -- --ignored --nocapture

# Uses tests/fixtures/face-detect/group.jpg (LFS) + bundled SCRFD
cargo test --manifest-path src-tauri/Cargo.toml \
  --lib scrfd_detects_faces_in_real_group_photo -- --ignored --nocapture
```

Record pass/fail + measured numbers in `docs/checkpoints/latest.md` so we can see where Phase 1 NFRs stand against PRD § Non-functional requirements. If search-latency p95 blows past 500 ms on 200k synthetic, that's the "real SigLIP + vec0 KNN" unblock signal.

### 3. Build the face-cluster fixture (option 2 from last session) · 30 min
Pick ~60 real photos across 3–5 recurring subjects from existing shoots under `E:\`. Write `tests/fixtures/face-clusters/labels.json` by hand:

```json
[
  {"file":"engagement_12.jpg","cluster":0},
  {"file":"haritha_07.jpg",    "cluster":0},
  {"file":"andaman_002.jpg",   "cluster":1},
  {"file":"stranger_03.jpg",   "cluster":-1}
]
```

Copy selected photos into `tests/fixtures/face-clusters/photos/` (gitignored). Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  --test phase_1_face_clustering -- --ignored --nocapture
```

Assert the test hits the PRD F1 ≥ 0.95 threshold. If it fails with HDBSCAN clumping too aggressively or spitting out noise, tune `ClusterParams.min_cluster_size` based on the fixture size.

### 4. Real SigLIP image + text inference · 120 min
First file: `src-tauri/src/ai/siglip.rs`. `load_or_stub` returns a stub regardless of path presence (Phase 1 placeholder). Replace with real ort session init — same pattern as `FacesSession::load` in `faces.rs`.

Required:
- Image encoder: `siglip2-b16-image.onnx` (bundled · 375 MB) — 224×224 RGB f32 normalised to `[-1, 1]`. Session output `[1, 768]`.
- Text encoder: **not currently bundled.** Either (a) swap `KNOWN_MODELS` embedding entry to also include `text_model.onnx` from the same `onnx-community/siglip2-base-patch16-224-ONNX` repo + bundle it, or (b) leave NL search zero-vector-stubbed and mark explicitly.
- Once the encoders run, also populate `vec_photo_embeddings` during stage-4 pipeline enrichment (currently only `photo_embeddings` BLOB is written).

This is the single biggest missing piece for Phase 1 exit — it's what makes the "semantic search" proposition real.

### 5. Decide: face-cluster labeling tool? · 15 min
After #3, if labeling 60 photos by hand was annoying: build the semi-automatic tool (scan → detect+embed → static HTML thumbnail grid → labels.json writer). Documented in last session's response. Skip if #3 was fine.

## Remaining Phase 1 critical path (after this session)

- 3 e2e fixtures (import-throughput 100k, source-cleanup 100, rediscovery dated) — lowest urgency; Rust integration tests are giving us the signal that matters.
- CI packaging job (fetch bundled models before `tauri build`, attach LICENSE files under `resources/licenses/`).
- Google Photos OAuth2 live-sync source (biggest unshipped connector).
- `phase_1_stress.rs` nightly 8 h loop.
- Moondream2 sidecar + mmproj (genuinely Phase 2).
- ModelPickerModal custom HF URL flow (Phase 1b).

## Push discipline

- #1 ships standalone (5 commits already coherent).
- #2 (running tests, recording numbers) happens before #3 lands — the numbers inform whether HDBSCAN tuning is needed.
- #3 + #4 bundle as "week-5 follow-up" PR.
- #5 optional — only if needed.
