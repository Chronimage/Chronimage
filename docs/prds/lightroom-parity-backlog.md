# Lightroom Parity Backlog

> Post-v1 PRD for the Lightroom-class features Chronimage still lacks after Phase 6. This is not a mandate to become a full Lightroom clone; it is a prioritized gap list for the features that make users stay in Lightroom after import, cull, basic develop, presets, prompt edits, and source cleanup already work.

## Source audit

Baseline checked on 2026-04-27 against current Adobe docs:

- Lightroom desktop masking includes AI and manual masks; Lightroom Classic exposes Subject, Sky, Background, Landscape, Objects, People, Brush, Linear Gradient, Radial Gradient, Color Range, Luminance Range, Depth Range, plus add/subtract/intersect/invert mask composition: <https://helpx.adobe.com/si/lightroom-classic/help/masking.html>
- Lightroom's recent AI surface includes Distraction Removal for reflections and extra people, Scene Enhance, Select Landscape, Adaptive Profiles, and AI Edit Status refresh/update workflows: <https://helpx.adobe.com/lightroom-cc/using/whats-new.html>
- Lens Blur provides AI focus selection, bokeh controls, focus range, depth visualization, and brush refinements: <https://helpx.adobe.com/lightroom-cc/using/lens-blur.html>
- Generative Upscale creates stacked upscaled output at 2x/4x depending on model/file limits: <https://helpx.adobe.com/lightroom-cc/using/generative-upscale.html>
- Lightroom Classic supports panorama/HDR panorama merge with Boundary Warp, Fill Edges, Auto Crop, and stack creation: <https://helpx.adobe.com/lightroom-classic/help/panorama.html>
- Lightroom Classic supports tethered capture for supported cameras, with current camera support maintained by Adobe: <https://helpx.adobe.com/lightroom-classic/kb/tethered-camera-support.html>

## Current Chronimage state

Shipped:

- Non-destructive global develop operations: exposure, contrast, highlights, shadows, whites, blacks, temp, tint, vibrance, saturation, clarity, dehaze, tone curves.
- Preset library with system and user presets, including scalar approximations for face, scene, quality, and style workflows.
- Prompt editing sidecar with optional SAM2/CLIP mask input.
- Dedicated Mask tab that creates prompt-driven masks and passes the active mask into prompt generation.
- XMP sidecar import/export basics, copy/paste edits, culling, catalog, source cleanup, map, rediscovery.

Missing:

- Persistent local-adjustment mask layers that affect develop operations.
- Manual mask tools, mask composition, range masks, per-person/body-part masks.
- Adaptive presets that create a mask and apply localized develop operations.
- Lens blur, AI denoise/super-resolution/upscale, distraction removal, reflection removal, AI edit invalidation/refresh.
- Crop/straighten/transform, lens correction/profile handling, chromatic aberration, spot heal/content-aware remove.
- HDR/panorama merge, tethered capture, video edit support, robust print/book/slideshow outputs.

## Goals

- Close the 80% Lightroom gap for hobbyists and semi-pros without sacrificing Chronimage's local-first, non-subscription positioning.
- Make masks first-class non-destructive edit layers, not one-off prompt-edit payloads.
- Add AI-powered features only when they can run locally or through an explicit user-configured sidecar.
- Preserve a clean fallback: unsupported features must be visible as pending capabilities, not silent data loss.

## Non-goals

- Cloud sync parity with Lightroom cloud.
- Adobe ecosystem parity: Photoshop round-trip, Creative Cloud libraries, Behance, Firefly account flows.
- Full prepress/print module parity in the first parity pass.
- Mobile-first editing parity.

## Personas & stories

- As Priya, I can select a person, brighten only their face, soften skin, sharpen eyes, and sync that adaptive portrait preset across a batch.
- As Jay, I can remove a window reflection or background person without leaving Chronimage.
- As a landscape photographer, I can select sky, water, mountains, or vegetation, tune each independently, and save those settings as a reusable adaptive preset.
- As a studio shooter, I can tether a supported Sony/Canon/Nikon/Fuji body and review imported frames immediately in cull/develop.
- As an archivist, I can merge HDR brackets or panoramas and keep the result stacked next to source frames.

## Milestones

### 1. Mask Engine Parity

- [ ] `develop_masks` table with ordered mask layers, names, visibility, mode, source, bitmap/path payload, and operations JSON.
- [ ] Render pipeline applies global operations first, then each visible local mask operation through a grayscale multiplier.
- [ ] Manual Brush mask with size, feather, flow, density, erase mode, and RLE or tiled bitmap persistence.
- [ ] Linear Gradient and Radial Gradient masks as geometry payloads.
- [ ] AI Subject, Sky, Background, Foreground, Object, Person, and Landscape masks using the existing sidecar contract.
- [ ] Color Range, Luminance Range, and Depth Range masks where source data exists.
- [ ] Add, Subtract, Intersect, Invert, Duplicate, Rename, Hide, and Solo actions in the Masks panel.
- [ ] Mask overlay colors and opacity control; show mask as color overlay or black/white matte.

### 2. Adaptive Presets

- [ ] Preset schema supports `scope = global | mask`.
- [ ] Adaptive preset definition includes mask source, mask prompt/options, local operations, fallback global operations, and confidence threshold.
- [ ] Built-ins: Portrait relight, Skin smooth, Eye pop, Whiten teeth, Enhance sky, Darken background, Scene enhance, Denoise subject.
- [ ] Applying an adaptive preset creates or refreshes a mask layer and stores the generated mask with the edit history.
- [ ] Copy/paste/sync preserves adaptive preset intent and can regenerate masks per target photo.

### 3. AI Edit Stack

- [ ] `ai_edits` table tracks feature, model id, source edit hash, output artifact, invalidation state, and refresh action.
- [ ] AI Edit Status panel lists Denoise, Upscale, Lens Blur, Remove, and generated masks that need refresh after upstream changes.
- [ ] Denoise: RAW-preferred local model path, preview proxy, full-res render on export.
- [ ] Generative Upscale: 2x/4x sidecar contract, creates stacked DNG/TIFF/JPEG output next to original.
- [ ] Distraction Removal: object brush/prompt mask plus remove/inpaint sidecar request.
- [ ] Reflection and extra-people removal: specialized prompts/models behind the same explicit sidecar gate.

### 4. Lens Blur

- [ ] Depth map source: embedded mobile depth map, local monocular depth model, or user-painted focus/blur mask.
- [ ] Blur controls: amount, bokeh shape, cat-eye, bokeh boost, focus range.
- [ ] Visualize Depth overlay.
- [ ] Refinement brush for focus/blur with feather and flow.
- [ ] Batch copy/paste lens blur settings with per-photo depth regeneration.

### 5. RAW Develop Completeness

- [ ] Crop, rotate, straighten, aspect presets, and transform guides.
- [ ] Lens correction database integration, vignetting, distortion, and chromatic aberration controls.
- [ ] Camera/profile browser with default import profile assignment.
- [ ] Spot heal/remove for small dust and blemishes, separate from generative remove.
- [ ] History panel with named snapshots and compare.
- [ ] Before/after split and reference view.

### 6. Merge And Capture

- [ ] HDR merge for bracketed source frames; output stacked DNG/TIFF.
- [ ] Panorama merge with preview, boundary warp, fill edges, auto crop, and stack source frames.
- [ ] HDR panorama merge where bracket detection succeeds.
- [ ] Tethered capture MVP for Windows-supported camera SDKs or vendor CLIs, starting with Sony and Canon.
- [ ] Watched-folder fallback for unsupported tethered cameras.

## Schema sketch

```sql
CREATE TABLE IF NOT EXISTS develop_masks (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  edit_id         INTEGER REFERENCES edits(id) ON DELETE CASCADE,
  name            TEXT NOT NULL,
  source          TEXT NOT NULL,
  mode            TEXT NOT NULL DEFAULT 'normal',
  visible         INTEGER NOT NULL DEFAULT 1,
  order_index     INTEGER NOT NULL,
  mask_payload    TEXT NOT NULL,
  operations_json TEXT NOT NULL CHECK (json_valid(operations_json)),
  confidence      REAL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_edits (
  id                INTEGER PRIMARY KEY,
  photo_id          INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  feature           TEXT NOT NULL,
  model_id          TEXT NOT NULL,
  source_edit_hash  TEXT NOT NULL,
  output_path       TEXT,
  output_b64        TEXT,
  state             TEXT NOT NULL DEFAULT 'current',
  params_json       TEXT NOT NULL CHECK (json_valid(params_json)),
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);
```

## API surface

- `develop_masks_list(photo_id) -> Vec<DevelopMask>`
- `develop_mask_create(photo_id, source, payload, operations) -> MaskId`
- `develop_mask_update(mask_id, payload?, operations?, visible?, order_index?)`
- `develop_mask_delete(mask_id)`
- `develop_mask_from_prompt(photo_id, prompt, options) -> MaskReceipt`
- `develop_mask_apply_preview(photo_id, global_operations, masks) -> RenderReceipt`
- `adaptive_preset_apply(photo_id, preset_id, strength) -> AdaptivePresetReceipt`
- `ai_edit_status(photo_id) -> Vec<AiEditStatus>`
- `ai_edit_refresh(photo_id, feature) -> AiEditReceipt`
- `hdr_merge(photo_ids, options) -> MergeReceipt`
- `panorama_merge(photo_ids, options) -> MergeReceipt`
- `tether_session_start(profile) -> TetherSessionId`

## Exit criteria

- Mask preview and saved export match within 1 mean absolute RGB value at 1280 px and full-res render.
- Subject/Sky/Background prompt masks hit IoU >= 0.85 on the 100-photo fixture set.
- Brush latency stays under 16 ms per stroke segment on a 24 MP preview proxy.
- Adaptive preset copy/paste regenerates masks per target photo and never applies a stale bitmap silently.
- AI Edit Status marks affected AI edits stale after upstream global or mask changes.
- HDR/panorama merge keeps source frames stacked and never deletes originals.
- Tethered capture imports a 25-frame session without blocking the UI.

## Open questions

- Local models vs sidecar: which features must run fully local for Community, and which can require an explicit sidecar?
- Mask bitmap storage: SQLite BLOB, sidecar PNG in app data, or tiled mask cache keyed by edit hash?
- RAW full-res export: finish GPU pipeline before local masks, or ship CPU full-res with queue/progress first?
- Adaptive presets: should system presets remain scalar-only until mask regeneration is deterministic?
- Tethering: vendor SDK integration or watched-folder first?
