# ADR 0003 — Bundle default models in the installer; user-swappable from Settings

**Status:** Accepted · 2026-04-20
**Updated:** 2026-04-27 · v1 baseline uses SAM2.1 for local masks; SAM3 is optional
**Supersedes:** ADR 0002 (first-run download of all 5 models) in part — the download infrastructure stays, it's no longer the first-run critical path.

## Context

Phase 1 shipped a 5-model download that users trigger during onboarding (PR #22, #23, #25). Smoke testing hit multiple failure modes: gated HF repos, wrong repo owners, zip-extract filename mismatches, per-model download buttons confusingly doing only one download. The UX asks a novice photographer to make five technical decisions before they can see a single photo catalogued.

The product rule for v1 is stricter than the Phase 1 catalog exit: **advertised default features must work from bundled resources.** Sidecars and large downloads may enhance quality, captioning, or generative editing, but they must not be required for the default catalog, cull, people, search, or develop masking experience.

Total community-model footprint is multiple GB when captioning and generative models are included. The six core catalog models currently marked `bundled: true` in `src-tauri/src/ai/download.rs` total roughly 861 MB uncompressed. Adding SAM2.1 Hiera-Large for local Lightroom-style masks brings the baseline model payload to roughly 1.7-1.9 GB before installer compression.

## Decision

**Ship default models bundled in the installer. Move model selection from onboarding to a Settings panel.**

### Bundled set (~1.7-1.9 GB with SAM2.1 Large, mandatory for v1 defaults)

| Model | File | Size | Purpose | License |
|---|---|---|---|---|
| SigLIP-2 B/16 image encoder | `siglip2-b16-image.onnx` | ~372 MB | Image embeddings / semantic search | Apache-2.0 |
| SigLIP-2 B/16 text encoder | `siglip2-b16-text.onnx` | ~283 MB | NL query encoding for `search_photos` | Apache-2.0 |
| SigLIP-2 tokenizer | `siglip2-b16-tokenizer.json` | ~2.5 MB | HF tokenizer.json; loaded via `tokenizers` crate | Apache-2.0 |
| NIMA (MobileNet) | `nima.onnx` | 13 MB | Aesthetic score | permissive (community ONNX) |
| SCRFD-10g | `det_10g.onnx` (from `buffalo_l`) | ~16 MB | Face detection | MIT |
| ArcFace W600K R50 | `w600k_r50.onnx` (from `buffalo_l`) | ~174 MB | Face embedding | MIT |
| SAM2.1 Hiera-Large encoder | `sam2.1_hiera_large.encoder.onnx` | ~900 MB bundle estimate | Local AI masks: image features | Apache-2.0 |
| SAM2.1 Hiera-Large decoder | `sam2.1_hiera_large.decoder.onnx` | included above | Local AI masks: Subject, Sky, Object, Person, Foreground, Background | Apache-2.0 |

Current six-model core payload is roughly 861 MB. SAM2.1 Hiera-Large adds roughly 900 MB, producing a v1 baseline around 1.7-1.9 GB.

The text encoder and tokenizer are required for real NL search (`search_photos`); without them the feature returns empty results (stub path). The mask model is required for default AI masks; sidecar prompt masks may still exist, but the Develop tab must not depend on a user-configured generative sidecar for subject/sky/object/person masking.

Every shipped binary must have redistribution checked before release and include its license/attribution in the installer. The six current core models are cleared for redistribution; the final mask checkpoint must be pinned and verified before the first signed build that includes it.

### On-demand (opt-in, from Settings → AI Models)

- Moondream2 GGUF (2.84 GB) — captions; only triggered when user enables captioning or a caption-dependent feature.
- Moondream2 mmproj companion (~700 MB) — required for real vision captioning when using the GGUF sidecar path.
- SAM3 ViT-H (~3.5 GB) — optional Settings switch for open-vocabulary text masks after the SAM2.1 default path is stable.
- Larger SAM2 / HQ segmentation (380 MB+) — higher-quality point/box masks after the default local mask path works.
- FaceMesh / eyes-open model (~15 MB) — small enough to bundle later, but should remain out of the baseline until a real face-landmark / eyes-open code path exists.
- Flux / SDXL / ComfyUI sidecar models (2-12 GB) — generative prompt editing and inpainting; never required for default masking or basic develop.
- Any user-chosen alternates: Florence-2, Qwen2.5-VL, SigLIP-2-Large, etc. — paste an HF repo URL + filename, the app downloads, SHA-verifies, and registers.

Optional models are enhancement paths. A missing optional model may hide or disable its enhancement, but it must not make an advertised default feature inert.

### Onboarding change

Remove the "Models" step from onboarding entirely. Steps go from 5 → 4: Welcome · Sources · Import · People-naming.

### Settings change

Settings → AI Models panel replaces the old static render. One row per feature (Embeddings, Face detection, Face embedding, Aesthetic, Masks, Captions) showing current model + [Bundled] / [Installed] / [Not installed] badge + Swap button. Swap opens a picker with curated presets + custom HF URL field. Post-swap, a "Re-index affected photos" or "Regenerate masks" CTA appears (explicit confirmation; reuses the existing re-evaluator machinery where applicable).

Model choice persists via `tauri-plugin-store`; `AppState::new` reads it at boot before constructing sessions.

## Consequences

### Positive

- **Zero-click first run.** Jay (our hobbyist persona, flaky hotel Wi-Fi) can import photos on day one without hitting HuggingFace at all.
- **Default features are honest.** Search, people, cull ranking, and AI masks work immediately after install. Sidecars improve results; they do not gate the core product.
- **No multi-GB download-failure UX.** The two most painful footguns (gated repos, zip-extract filename drift) disappear for the bundled set.
- **Auto-updater deltas stay small.** Bundled model files are pinned + rarely change; `tauri-plugin-updater` patches only app code on most updates.
- **Settings becomes the honest source of model state.** Users who care can swap; users who don't never think about it.

### Negative

- **Installer size crosses 1 GB.** The current core AI payload is ~861 MB; adding SAM2.1 Large brings the model payload to roughly 1.7-1.9 GB before app code, resources, and installer overhead. Still below Lightroom's published 8-10 GB install-space requirement, but larger than typical desktop apps.
- **Redistribution obligations.** Each bundled model's `LICENSE` file must ship in the installer under `%ProgramFiles%/Chronimage/licenses/` with attribution. Tracked under release-captain work.
- **CI artifact size.** GitHub Actions artifact storage for model files is non-trivial; keep the bundled `.onnx`s in a separate LFS-backed release asset, download + sign during the packaging job rather than committing them.
- **Re-indexing cost after a swap.** Changing embeddings invalidates the `photo_embeddings` table; user must sit through a re-embed of their whole library. The confirmation dialog must surface expected duration.
- **Mask model quality trade-off.** SAM2.1 is the default for Lightroom-style masks. SAM3 is available through Settings when text-prompt segmentation is worth the larger download.

## Implementation pointers

- `src-tauri/build.rs` or Tauri's `bundle.resources` config pulls model files from a dev-time cache (`models/bundled/…`) into the MSI. CI fetches them from a pinned S3/HF release ahead of `tauri build`.
- `src-tauri/src/ai/download.rs` `KNOWN_MODELS` keeps `bundled: true` for the default offline baseline and `bundled: false` for opt-in enhancement models.
- `KNOWN_MODELS` includes the SAM2.1 encoder/decoder as bundled mask entries and SAM3 components as optional Settings downloads.
- `ai_models_status()` returns `installed: true` for bundled entries regardless of user data dir presence, with a new `source: "bundled" | "downloaded" | "missing"` variant.
- `AppState::new` looks up bundled-resource path first (via `tauri::path::resolve_resource`), falls back to `%LOCALAPPDATA%/.../models/` for on-demand files.
- Onboarding `MODEL_TIERS` and the "Models" step are removed; the stepper recomputes to 4 steps.
- Develop masking gets a local inference path for subject / sky / foreground / object / person before those actions are advertised as default features.

## Rollout

1. Land the PRD + ADR edits (this commit).
2. Branch-land the installer bundling path (`tauri.conf.json` `resources` already includes `models/bundled/*`; verify release CI fetches the full baseline before `tauri build`).
3. Update `AppState::new` + `ai_models_status` so bundled resources are resolved before downloaded model paths.
4. Add the default mask model manifest entry and local mask inference path.
5. Settings AI Models panel gains the picker modal.
6. Remove Models step from OnboardScreen; update `onb-step` stepper to 4.
7. Doc + release notes.
