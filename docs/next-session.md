# Next session · Phase 1 week 5 — bundle default models

Queued on local `feature/phase1-week4-plumbing` (2 commits, unpushed):
- `63cd55a` SHA locks + catalog-size exit test + record_photo_view + stage-4 filename fix + `CHRONIMAGE_MODELS_DIR` override
- `9528c8a` real SCRFD + ArcFace inference + pipeline stage-5 + `phase_1_face_clustering` exit test

**New direction (this session):** PRD §1 / §5 / §14 amended + ADR 0003 added. Default models will ship bundled in the installer; Settings owns swaps. See `docs/adr/0003-bundled-default-models.md`.

## Starts (in order)

### 1. Ship the week-4 bundle as-is · 15 min
```bash
git push -u origin feature/phase1-week4-plumbing
gh pr create --base develop
```
This is independent of the bundling work and unblocks everything else.

### 2. Installer-resource plumbing for bundled models · 90 min
First file: `src-tauri/tauri.conf.json` — add `"bundle.resources": ["models/bundled/*"]` (or equivalent Tauri v2 syntax).

Then:
- `src-tauri/src/ai/download.rs` — add `pub bundled: bool` to `ModelSpec`; mark the 4 always-needed entries `bundled = true`, Moondream2 `false`.
- `src-tauri/src/util/paths.rs` — new `bundled_models_dir()` via `tauri::path::resolve_resource`.
- `src-tauri/src/ai/download.rs` or `commands.rs::ai_models_status` — check bundled path first; treat bundled files as permanently `installed: true` with `source: "bundled"`.
- `src-tauri/src/state.rs` — `AppState::new` resolves model paths from bundled dir first, falls back to user data dir for Moondream2.

Add a `models/bundled/` directory to the repo **gitignored** plus a `scripts/fetch-bundled-models.ps1` / `.sh` that downloads the 4 defaults with the locked SHA256s. CI runs this in the packaging job before `tauri build`. Dev-time: documented in CLAUDE.md + README.

Why highest leverage: every other item in this plan assumes the bundling path exists. Do it first so downstream UI work can be built against the final shape.

### 3. Remove Models step from onboarding · 30 min
First file: `src/screens/OnboardScreen.tsx` — delete `OnbModels`, the `MODEL_TIERS` array, the step entry for `models`, and the associated download-listener wiring. Renumber the stepper from 5 → 4 (Welcome · Sources · Import · People-naming).

Update `src/screens/OnboardScreen.test.tsx` expectations. Visual QA: the `onb-step` stepper must still show 4 dots + connector lines with correct `on`/`done` states.

Why: direct user-facing change that closes half the "first-run is confusing" feedback. Cannot land before #2 because the stepper assumes the bundled set exists.

### 4. Settings → AI Models picker modal · 90 min
First file: `src/screens/SettingsScreen.tsx` — the AI Models section already renders rows; replace the "(phase-1b)" muted badge with a real **Swap…** button per row.

New picker modal (`src/screens/settings/ModelPickerModal.tsx` or similar):
- Curated presets per feature (3-5 options: current bundled + 2-4 alternates we've vetted — SigLIP-2-Large, Florence-2 for captions, etc.).
- "Add custom HF URL…" field + filename input → `ai::download::user_initiated_download_model` with SHA verification (post-download hash display for user to compare).
- "Re-index affected photos" CTA appears on model change (reuses the re-evaluator + adds an `ai_reindex(kind)` command that truncates + recomputes the affected column/table).

Persist active choice via `tauri-plugin-store`; `AppState::new` reads it before session construction.

Why: primary user-facing interaction now that models live in Settings. Also the mechanism that lets power users add any HF model post-install.

### 5. Smoke + merge · 30 min
`pnpm tauri dev`:
- First-run with fresh `%LOCALAPPDATA%/app.chronimage.desktop/` (delete it manually) — onboarding must complete without any download prompt, first photo imports cleanly, ai_models_status shows 4 bundled `installed: true` + Moondream2 `installed: false, source: "missing"`.
- Settings → AI Models → Swap on Embeddings to SigLIP-2-Large → download progresses → "Re-index" CTA appears → click → photo_embeddings rebuilds.

Then PR + merge.

## Push discipline

- #1 as its own PR (already queued; just push).
- #2 + #3 + #4 as one "bundle default models" PR (shares the `bundled` flag + onboarding delete + settings UI; reviewer can trace the end-to-end flow).
- #5 manual smoke before merging #2-#4.

## Deferred

- HDBSCAN real clustering (unblocked now that ArcFace embeddings flow in stage-5; needs a real Rust HDBSCAN crate vetted or a hand-roll).
- Moondream2 mmproj companion download + llama.cpp sidecar binary bundling + vision-language HTTP protocol (Phase 2 prereq, not Phase 1 exit).
- Remaining 6 PRD exit-criteria tests (fixtures missing).
- CI packaging job: fetch bundled models from a pinned S3/HF release + attach LICENSE files under `resources/licenses/`.
- User research: is a ~700 MB MSI acceptable? If not, ship a "slim" variant that first-run-downloads the bundled set.
