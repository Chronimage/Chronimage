# Phase 3 · RAW Develop

> Lightroom-class non-destructive RAW editor covering 80% of hobbyist needs — exposure, curves, masks, AI presets, copy/paste edits — all GPU-accelerated via wgpu compute shaders, with a CPU fallback that's merely slow, not missing.

## Context

Phases 1–2 make Chronimage a best-in-class organizer + culler. Phase 3 is where it becomes a legitimate Lightroom alternative for hobbyists. The user's concrete pain: "Lightroom is too expensive," so a solid develop surface — even if it lacks the 10% of pro features — is what turns Chronimage from "the free organizer" into "the tool I actually edit in."

Following RapidRAW's proven path: Rust + wgpu compute shaders for the heavy lifting, React+Radix for sliders, non-destructive edit records in SQLite.

## Personas & stories

- **Jay (Sony A7 IV, 24 MP ARW, i5 laptop + no GPU)**
  - As Jay, I can load an ARW into Develop in < 2 s and adjust exposure/contrast sliders with visible feedback within 80 ms, even without a GPU.
  - As Jay, I can apply "Clean up face" at strength 55 + "Enhance sky" at 72 and export the result; the RAW is never modified.
  - As Jay, I can copy edits from one photo and paste-sync to 12 selected siblings.

- **Priya (event photographer, 3070 GPU)**
  - As Priya, my slider drags render at 60 fps on 45 MP RAW thanks to the GPU pipeline.
  - As Priya, I can mask the subject (AI-detected) and apply shadows +35 only to her; the sky stays untouched.
  - As Priya, Curves (RGB + per-channel L) let me dial in a custom film look in under 30 s.

## Must-have deliverables

### 1. RAW decode module (`src-tauri/src/raw/`) — **deferred to week 2+**
The MVP develop loop operates on the already-decoded 1280 px thumbnails that Phase 1 stage 2.6 writes, which covers the "JPEG + embedded RAW preview" case for all supported formats without the rawler dep. True RAW develop (operating on 14-bit linear sensor data) lands with the wgpu pipeline and `rawler`/`libheif-rs` integration in week 2.
- [ ] `decode.rs` — `rawler` first pass; `rsraw` (LibRaw FFI) fallback for exotic cameras
- [ ] Formats: ARW (priority — Sony A7 IV), CR2, CR3, NEF, NRW, RAF, RW2, ORF, DNG, PEF, SRW
- [ ] HEIC via `libheif-rs` (already behind `heic` cargo feature from Phase 2 §6)
- [ ] Embedded-JPEG fast path: < 50 ms for Grid thumbnails, < 400 ms full decode
- [ ] Orientation applied from EXIF exactly once (tracked via a `Linear<T>` / `GammaEncoded<T>` type wrapper that makes pipeline bugs compile-errors)
- [ ] `color.rs` — ICC profile handling via `lcms2-sys` or pure-Rust `qcms` fallback; sRGB / P3 / AdobeRGB output profiles

### 2. Develop screen (ported from `screens_editor.jsx`)
- [x] Three tabs via `Seg`: **Develop** (sliders + curves), **Mask** (renders but inactive — SAM2 engine is week 2), **Prompt** (Phase 4, hidden)
- [x] Editor canvas with the preview image at centered aspect; slider drags repost the stage via `develop_apply`
- [ ] Mask overlays (AI subject, sky, foreground, radial, linear, brush) — week 2
- [ ] Histogram bottom-left (RGB + luminance) — static SVG stub today; live histogram is a follow-up
- [x] Filmstrip bottom (current photo's neighbours)
- [x] Toolbar: Back/Forward · filename + RAW/dims · Copy edits · Save
- [ ] Crop tool + Before/After toggle — week 2
- [x] Inspector panel (right-side by default):
  - [x] **Auto** button — seeds reasonable exposure/shadow/vib defaults
  - [x] **Light** sliders: Exposure, Contrast, Highlights, Shadows, Whites, Blacks
  - [x] **Curves** — panel wired; drag-and-drop control points + RGB/R/G/B/L channel tabs are week 2 (current build shows a static preview)
  - [x] **Color**: Temp, Tint, Vibrance, Saturation
  - [x] **Detail**: Clarity, Dehaze (Texture merged with Clarity for the MVP)
  - [x] **Copy · Paste · Sync** — Copy edits toolbar button + `develop_paste_edits` command
  - [x] **Export & Archive** — Save button writes to `edits`; Export flow reuses the Phase 2 sheet via multi-select

### 3. GPU pipeline (`src-tauri/src/raw/wgsl/`) — **deferred to week 2+**
- [ ] WGSL compute shaders, one per stage: `exposure.wgsl`, `contrast.wgsl`, `highlights_shadows.wgsl`, `white_black.wgsl`, `curves.wgsl`, `color.wgsl`, `clarity.wgsl`, `dehaze.wgsl`
- [ ] Shared struct definitions via `include!()` of `.wgsl.inc` so Rust-side push constants stay in sync
- [ ] `pipeline.rs` — builds a wgpu compute pipeline with one pass per active stage; pre-computes LUTs (curves) on CPU and uploads as storage buffers
- [x] CPU path at `src-tauri/src/develop/pipeline.rs` using `rayon` for parallelism. 8 stages (exposure / wb / endpoints / tone / contrast / saturation + vibrance / clarity / dehaze) applied in order on a 1280 px preview. Identity-ops fast-path bit-exact; every non-identity combination runs in ~30–60 ms on an i5.
- [ ] Adapter selection: DirectX 12 on Windows (wgpu default)

### 4. Mask engine
- [ ] `masks.rs` — mask is a grayscale buffer applied as a multiplier in shader
- [ ] AI subject/sky/foreground via Phase 1's SAM2 ONNX model
- [ ] Radial + linear masks (pure geometry, CPU)
- [ ] Brush mask with falloff; stored as a run-length-encoded bitmap in `edits.operations_json`
- [ ] Masked sliders: Exposure, Contrast, Shadows, Highlights, Temp, Clarity, Sharpness

### 5. Preset library
- [x] 4 built-in scalar-slider presets shipped in `src-tauri/src/develop/presets.rs`:
  - **Face**: Clean up face · Portrait relight (mask-dependent ones — Beautify lips, Whiten teeth — need SAM2, deferred to week 2)
  - **Scene**: Enhance sky (Remove background / Remove object need SAM2, deferred)
  - **Style**: B&W film (Tri-X 400)
- [x] Preset definition: `operations_json` (flat slider stack) in the `presets` table, seeded idempotently on app boot
- [x] Custom preset save: `preset_save(name, group, operations)` → user row
- [x] Strength slider per preset: linear interpolation via `Operations::blend`. Bit-exact at strength=0 (verified by `phase_3_preset_strength_zero_noop.rs`).

### 6. Edit history / non-destructive store
- [x] `src-tauri/src/develop/history.rs` — each save appends an `edits` row with `parent_edit_id`, `operations_json`, `saved_at`
- [ ] Undo/redo UI — chain is stored; UI buttons wire in week 2 (backend walk helper is straightforward from `list_for_photo`)
- [ ] Snapshot-every-N-minutes collapse — week 2
- [x] Reset: `develop_reset(photo_id)` — deletes all edits for a photo + clears `current_edit_id`
- [x] Copy edits: `develop_copy_edits(photo_id)` — returns current `Operations`
- [x] Paste edits: `develop_paste_edits(photo_ids, ops)` — one new row per photo, verified by `phase_3_copy_paste_edits.rs`

### 7. Auto-light (NIMA-informed)
- [ ] `develop/auto.rs` — uses NIMA score + histogram analysis to propose exposure, shadows, highlights
- [ ] "Match batch" mode: averages another photo's edit stack onto current

### 8. Crop & rotate
- [ ] Crop tool with aspect presets (original, 1:1, 3:2, 4:5, 16:9)
- [ ] Rotate (free + 90° steps) stored in `edits.operations_json` (doesn't re-encode the RAW)
- [ ] Upright / auto-straighten via Hough transform on detected horizon

### 9. Advanced face metrics (filed from Phase 2 rehaul · ADR 0007)

The catalog detail inspector currently hides an "Eyes open" quality bar because the underlying column (`photos.eyes_open`) is always NULL — SCRFD's 5 keypoints don't expose eyelid geometry. Phase 3 adds the dedicated model pass:

- [ ] MediaPipe FaceMesh (468 landmarks, ~15 MB ONNX) or dlib's 68-point model — pick based on accuracy on the `phase_1_face_clustering` fixture. MediaPipe has better cross-face coverage; dlib is smaller.
- [ ] New `src-tauri/src/ai/face_landmarks.rs` session singleton, similar shape to `FacesSession`. Wired via `providers::session_builder_with_ep` so it picks up DirectML on Windows.
- [ ] Pipeline Stage 5.5 (or extend Stage 5): for each detected face, compute Eye Aspect Ratio (EAR) = (vertical eyelid distance) / (horizontal eye width). EAR < 0.18 = eye closed. Store mean EAR across both eyes in `photos.eyes_open` (0.0–1.0 scale).
- [ ] Surface in the inspector Quality bars (already positioned; Phase 2 rehaul hid the bar when NULL).
- [ ] Also exposes motion-blur / face-occlusion metrics (optional — future cull issue-filter work uses the same model).
- [ ] Cost: ~50 ms per detected face on CPU, ~10 ms on DirectML. Negligible vs existing Stage 5 costs.

## Non-goals

- No prompt-driven generative edits (Phase 4)
- No local-adjustment tools beyond masks (no gradient filter, no spot-heal — those are Phase 5+)
- No proofing / soft-proof workflows (pro use case)
- No tethered shooting (pro use case)

## Non-functional requirements

- 24 MP ARW loads to screen in < 800 ms on GPU host, < 2 s on CPU-only host
- Slider drag maintains 60 fps on a 3060-tier GPU for 45 MP RAW
- Edit history round-trips: save → restart app → reload photo → identical pixel output (hash check)
- Preset application with strength = 0 is a no-op (bit-exact)
- Mask engine accuracy: SAM2 subject mask IoU ≥ 0.85 on a 100-photo test set

## Schema changes

New migration: `src-tauri/migrations/20260801000000_phase3_develop.sql`

```sql
CREATE TABLE IF NOT EXISTS edits (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  parent_edit_id  INTEGER REFERENCES edits(id) ON DELETE SET NULL,
  operations_json TEXT    NOT NULL CHECK (json_valid(operations_json)),
  saved_at        TEXT    NOT NULL,
  is_snapshot     INTEGER NOT NULL DEFAULT 0,
  label           TEXT
);

CREATE INDEX IF NOT EXISTS idx_edits_photo ON edits(photo_id);
CREATE INDEX IF NOT EXISTS idx_edits_parent ON edits(parent_edit_id);

-- User-saved presets (built-ins live in code).
CREATE TABLE IF NOT EXISTS presets (
  id             INTEGER PRIMARY KEY,
  name           TEXT    NOT NULL UNIQUE,
  group_name     TEXT    NOT NULL,
  description    TEXT,
  operations_json TEXT   NOT NULL CHECK (json_valid(operations_json)),
  is_system      INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT    NOT NULL,
  updated_at     TEXT    NOT NULL
);

-- Per-photo current-edit pointer (speeds up hot path; writable from UI).
ALTER TABLE photos ADD COLUMN current_edit_id INTEGER REFERENCES edits(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_photos_current_edit ON photos(current_edit_id);

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '4', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
```

## API surface (new commands)

- `async fn develop_open(photo_id: i64) -> Result<DevelopSession>`
- `async fn develop_apply(session_id: DevelopSessionId, ops: Operations) -> Result<RenderReceipt>` — streams back a preview JPEG (data URL) at 1024 long-edge
- `async fn develop_save(session_id: DevelopSessionId, label: Option<String>) -> Result<EditId>`
- `async fn develop_reset(photo_id: i64) -> Result<()>`
- `async fn develop_copy_edits(photo_id: i64) -> Result<Operations>`
- `async fn develop_paste_edits(photo_ids: Vec<i64>, ops: Operations) -> Result<PasteReceipt>`
- `async fn develop_preset_apply(photo_id: i64, preset_id: PresetId, strength: u8) -> Result<RenderReceipt>`
- `async fn presets_list(group: Option<String>) -> Result<Vec<Preset>>`
- `async fn preset_save(name: String, group: String, ops: Operations) -> Result<PresetId>`

Events:
- `chronimage.develop.render` — high-frequency preview updates during slider drags
- `chronimage.develop.saved` — fires when a new `edits` row lands

## Entitlements

- `Feature::GpuDevelop` — gates the wgpu pipeline; CPU path always available. Returns `true` in v1.
- `Feature::PromptEdit` — already defined in Phase 1's enum; used by Phase 4's develop tab. Returns `true` in v1.
- No other gates.

## Exit criteria (test-bound)

- [ ] `src-tauri/benches/raw_decode.rs` — 24 MP ARW full decode < 400 ms CPU, < 80 ms via embedded-JPEG path (criterion, 50-sample median)
- [ ] `src-tauri/benches/pipeline.rs` — full 9-stage pipeline on 45 MP input < 16 ms GPU, < 200 ms CPU (for slider feedback)
- [x] `src-tauri/tests/phase_3_edit_history_roundtrip.rs` — save → drop pool → reopen → operations + pixel output round-trip bit-exact
- [x] `src-tauri/tests/phase_3_preset_strength_zero_noop.rs` — every built-in preset at strength=0 yields input-identical output (+ a sanity companion test that strength=100 reproduces the preset exactly)
- [x] `src-tauri/tests/phase_3_copy_paste_edits.rs` — paste onto N photos creates N edits rows with identical `operations_json`; missing ids are returned in the `skipped` list
- [ ] `src-tauri/tests/phase_3_mask_subject_iou.rs` — SAM2 subject mask IoU ≥ 0.85 on the 100-photo fixture set
- [ ] `tests/e2e/phase-3-slider-fps.spec.ts` — exposure slider drag sustains > 50 fps on CI runner
- [ ] `src-tauri/tests/phase_3_color_profile_roundtrip.rs` — export as sRGB then re-import; color values within ΔE < 1 of source

## Open questions

- **wgpu backend on older Windows**: DirectX 12 (default) vs Vulkan fallback vs WARP (software) — DirectX 12 is the plan, document the graceful degradation to CPU.
- **SAM2 model size**: ~180 MB; first-time download adds setup friction. Consider bundling a smaller MobileSAM variant for v1 and offering "upgrade to full SAM2 in Settings."
- **Preset strength curve**: linear interpolation works for scalar parameters; what about Curves? Lean "apply curves at `strength/100` blend onto base state."
- **Bit-exact reproducibility across GPU vendors**: NVIDIA vs AMD vs Intel may produce ±1 LSB differences. Test-suite tolerance of ΔE < 1 covers this; document as expected.
- **History pruning**: how many `edits` rows to keep per photo before auto-collapsing to snapshots? Lean "collapse to snapshot after 30 edits or 24 hours, whichever first."

## TODO log

- [ ] Migration `20260801000000_phase3_develop.sql`
- [ ] `raw/` module completion (decode, color, pipeline, wgsl shaders, orientation, masks)
- [ ] `develop/` module (history, copy-paste, auto, presets)
- [ ] `src/screens/develop/` ported
- [ ] SAM2 / MobileSAM ONNX download flow
- [ ] Benchmarks in `src-tauri/benches/` tied to perf-cop baselines
- [ ] 8 exit-criterion test files
- [ ] Built-in preset JSON definitions in `src-tauri/src/develop/presets/*.json`
- [ ] `wgpu`, `bytemuck`, `pollster`, `lcms2-sys` or `qcms`, `rawler`, `libheif-rs`, `mozjpeg` cargo deps
