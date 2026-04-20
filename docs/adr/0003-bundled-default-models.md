# ADR 0003 — Bundle default models in the installer; user-swappable from Settings

**Status:** Accepted · 2026-04-20
**Supersedes:** ADR 0002 (first-run download of all 5 models) in part — the download infrastructure stays, it's no longer the first-run critical path.

## Context

Phase 1 shipped a 5-model download that users trigger during onboarding (PR #22, #23, #25). Smoke testing hit multiple failure modes: gated HF repos, wrong repo owners, zip-extract filename mismatches, per-model download buttons confusingly doing only one download. The UX asks a novice photographer to make five technical decisions before they can see a single photo catalogued.

Total community-model footprint is ~2.3 GB, but 1.7 GB of that is Moondream2 (captions) — a Phase 2+ feature. The four models actually required for Phase 1 exit (embeddings, face detect, face embed, aesthetic) total ~580 MB.

## Decision

**Ship default models bundled in the installer. Move model selection from onboarding to a Settings panel.**

### Bundled set (~580 MB, mandatory for Phase 1 catalog)

| Model | Size | Purpose | License |
|---|---|---|---|
| SigLIP-2 B/16 (224 ONNX) | 375 MB | Embeddings / semantic search | Apache-2.0 |
| SCRFD-10g (`det_10g.onnx` from `buffalo_l`) | 17 MB | Face detection | MIT |
| ArcFace W600K R50 (`w600k_r50.onnx` from `buffalo_l`) | 166 MB | Face embedding | MIT |
| NIMA (MobileNet) | 13 MB | Aesthetic score | permissive (community ONNX) |

Every license here permits redistribution in a commercial installer.

### On-demand (opt-in, from Settings → AI Models)

- Moondream2 GGUF (1.7 GB) — captions; only triggered when user enables captioning or a caption-dependent feature (Phase 2+).
- Any user-chosen alternates: Florence-2, Qwen2.5-VL, SigLIP-2-Large, etc. — paste an HF repo URL + filename, the app downloads, SHA-verifies, and registers.

### Onboarding change

Remove the "Models" step from onboarding entirely. Steps go from 5 → 4: Welcome · Sources · Import · People-naming.

### Settings change

Settings → AI Models panel replaces the old static render. One row per feature (Embeddings, Face detection, Face embedding, Aesthetic, Captions) showing current model + [Bundled] / [Installed] / [Not installed] badge + Swap button. Swap opens a picker with curated presets + custom HF URL field. Post-swap, a "Re-index affected photos" CTA appears (explicit confirmation; reuses the existing re-evaluator machinery).

Model choice persists via `tauri-plugin-store`; `AppState::new` reads it at boot before constructing sessions.

## Consequences

### Positive

- **Zero-click first run.** Jay (our hobbyist persona, flaky hotel Wi-Fi) can import photos on day one without hitting HuggingFace at all.
- **No 2.3 GB download-failure UX.** The two most painful footguns (gated repos, zip-extract filename drift) disappear for the bundled set.
- **Auto-updater deltas stay small.** Bundled model files are pinned + rarely change; `tauri-plugin-updater` patches only app code on most updates.
- **Settings becomes the honest source of model state.** Users who care can swap; users who don't never think about it.

### Negative

- **Installer size ~700 MB** (580 MB models + 120 MB app + webview runtime). Roomy for Windows but larger than typical desktop apps. If user research pushes back, consider a "slim" variant that falls back to first-run download.
- **Redistribution obligations.** Each bundled model's `LICENSE` file must ship in the installer under `%ProgramFiles%/Chronimage/licenses/` with attribution. Tracked under release-captain work.
- **CI artifact size.** GitHub Actions artifact storage for model files is non-trivial; keep the bundled `.onnx`s in a separate LFS-backed release asset, download + sign during the packaging job rather than committing them.
- **Re-indexing cost after a swap.** Changing embeddings invalidates the `photo_embeddings` table; user must sit through a re-embed of their whole library. The confirmation dialog must surface expected duration.

## Implementation pointers

- `src-tauri/build.rs` or Tauri's `bundle.resources` config pulls model files from a dev-time cache (`models/bundled/…`) into the MSI. CI fetches them from a pinned S3/HF release ahead of `tauri build`.
- `src-tauri/src/ai/download.rs` `KNOWN_MODELS` grows a `pub bundled: bool` field; bundled entries skip the download URL path and resolve from the resource directory.
- `ai_models_status()` returns `installed: true` for bundled entries regardless of user data dir presence, with a new `source: "bundled" | "downloaded" | "missing"` variant.
- `AppState::new` looks up bundled-resource path first (via `tauri::path::resolve_resource`), falls back to `%LOCALAPPDATA%/.../models/` for on-demand files.
- Onboarding `MODEL_TIERS` and the "Models" step are removed; the stepper recomputes to 4 steps.

## Rollout

1. Land the PRD + ADR edits (this commit).
2. Branch-land the installer bundling path (build.rs + tauri.conf.json `resources`).
3. Update `AppState::new` + `ai_models_status` + `KNOWN_MODELS.bundled`.
4. Settings AI Models panel gains the picker modal.
5. Remove Models step from OnboardScreen; update `onb-step` stepper to 4.
6. Doc + release notes.
