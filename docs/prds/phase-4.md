# Phase 4 · Prompt editing + polish

> Natural-language edits via Flux-dev / SDXL-inpaint on-device, the Tweaks panel for theme/accent/layout, map view with GPS clustering, and the keyboard shortcut overlay. The phase that takes Chronimage from "great editor" to "this feels designed."

## Context

Phase 3 ships a solid develop surface covering 80% of hobbyist edits. Phase 4 adds the remaining 20% that require generative models — inpainting, background removal/replacement, prompt-driven tone adjustments — plus the UX polish layer (tweaks, map, shortcuts) that the design already specced but we deferred.

The generative path is intentionally gated behind an entitlement + first-run model download because Flux/SDXL are 6–12 GB each and require explicit user consent to fetch. The Tweaks panel / map / shortcuts don't need models and ship unconditionally.

## Personas & stories

- **Jay (hobbyist, 3060 GPU added since Phase 3)**
  - As Jay, I can type "remove the power lines in the top-right and replace the dull sky with a dramatic late-afternoon one, keep skin tones natural" and see a generated result in under 30 s.
  - As Jay, I can pin constraints ("keep faces sharp", "natural tones") so every generation respects them.
  - As Jay, I can browse my photos on a map and see my trips cluster into pins.
  - As Jay, I can switch accent color, font, grid density, and cull mode without restarting.

- **Priya (event photographer, 3070 GPU)**
  - As Priya, I can remove a stray photobomber from a group portrait with a brushed mask + "remove person" prompt.
  - As Priya, keyboard shortcuts are discoverable via `?` overlay; I can customize any of them.

## Must-have deliverables

### 1. Prompt tab in Develop (ported from `screens_editor.jsx` prompt tab)
- [ ] Before/After split layout (two canvases side by side, sync pan/zoom)
- [ ] Prompt textarea: multi-line, auto-grow, Ctrl+Enter = Generate
- [ ] Constraint chips: pinnable + dismissable ("keep faces sharp", "natural tones", "preserve colors")
- [ ] Strength slider (0–100, default 65)
- [ ] Mask button — switches to Phase 3 mask engine with "generate-only-inside-mask" flag
- [ ] History: each generation stored as a new `edits` row with `kind='prompt'` and full prompt/seed/model metadata in `operations_json`
- [ ] Fork (accept result = save edit; reject = drop)

### 2. Generative backend
- [ ] **Flux-dev** via `diffusers` Python sidecar OR `candle`/`mistral.rs` native Rust when model support lands (track upstream)
- [ ] **SDXL-Inpaint** fallback for users with <12 GB VRAM
- [ ] Sidecar management via `tauri-plugin-shell` + `externalBin` in tauri.conf.json
- [ ] OpenAI-compatible HTTP on localhost:17183 (configurable) so `candle-vllm` / `llama.cpp` / Python can all hide behind one interface
- [ ] Request/response schema: `{ image_b64, mask_b64?, prompt, strength, seed?, constraints: [] } → { image_b64, latency_ms, model_id, seed }`
- [ ] PID-supervised; graceful shutdown on app exit; auto-restart on unexpected exit
- [ ] First-run download UI: "Chronimage needs to download Flux-dev (~12 GB). Continue? Alternate: SDXL-Inpaint (~6 GB) if your GPU has <12 GB VRAM."

### 3. Mask-from-prompt
- [ ] SAM2 + CLIP text-encoder: "select the sky" → SAM2 generates a mask filtered by CLIP similarity to the prompt
- [ ] Integrated with Phase 3's mask engine (appears as a new mask source alongside AI-subject / sky / foreground)

### 4. Tweaks panel (ported from `tweaks.jsx`)
- [ ] Toggleable drawer bound to a tauri-plugin-store setting (`__TWEAKS__`)
- [ ] Settings: Theme · Accent (5 swatches) · Display font · Grid density · Facet placement · Cull mode · Editor layout
- [ ] Live-applies via `data-*` attributes on `<html>` (already wired in Phase 0)
- [ ] Reset to defaults button

### 5. Map view
- [ ] `src/screens/map/` new screen (added to Rail between Cull Bin and Develop)
- [ ] OpenStreetMap tiles cached locally to `{data_dir}/tiles/{z}/{x}/{y}.png`
- [ ] Tile cache respects Nominatim usage policy (attribution shown, max 2 zooms per second)
- [ ] Pin clusters via supercluster-rs algorithm (or Leaflet's cluster plugin)
- [ ] Click cluster → zoom; click pin → photo detail overlay
- [ ] Filter by time range (slider: "last week / last month / last year / all time")

### 6. Keyboard shortcut overlay
- [ ] Press `?` anywhere → modal showing all shortcuts grouped by context
- [ ] Shortcuts registered via a central `useShortcut(key, handler, scope)` hook
- [ ] Customizable via Settings → Shortcuts table (per-command rebinding, conflict detection)
- [ ] All shortcut data stored in tauri-plugin-store; exported/imported with tweaks

### 7. Import keyword + star from `.xmp` sidecars
- [ ] On import, if `photo.xmp` exists alongside `photo.jpg` or inside `photo.arw` (embedded XMP), parse it
- [ ] Map `dc:subject` → `tags` (kind='user'), `xmp:Rating` → new `photos.rating INTEGER`
- [ ] Write-out: when user edits tags / rating in Chronimage, write back to sidecar `.xmp` (opt-in, default off — avoids surprising other tools)

### 8. `.chronimage-ignore` file support
- [ ] `.gitignore`-style include/exclude at any folder level
- [ ] Respected by `scan_dir`
- [ ] Default rules: exclude `Thumbs.db`, `.DS_Store`, `@eaDir`, `.thumbnails`, `cache/`

## Non-goals

- No video editing
- No tethered shooting
- No advanced retouching (frequency separation, dodge/burn beyond what masks provide)
- No smart-object / layer system
- No proofing workflows

## Non-functional requirements

- Generative inference latency: < 30 s for 1024×1024 SDXL-Inpaint on 3060-tier; < 45 s for Flux-dev
- Map tile cache hit rate: > 95% for a 100-trip library after first view
- Tweaks panel toggle → UI reflects change in < 100 ms
- Sidecar crash: app detects + offers restart within 2 s
- `.xmp` import adds < 5% to import time for a 10k-photo library

## Schema changes

New migration: `src-tauri/migrations/20261001000000_phase4_polish.sql`

```sql
ALTER TABLE photos ADD COLUMN rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5);
ALTER TABLE photos ADD COLUMN color_label TEXT;                -- red / yellow / green / blue / purple / NULL
CREATE INDEX IF NOT EXISTS idx_photos_rating ON photos(rating);

-- Persisted user settings beyond the simple KV in `settings`.
CREATE TABLE IF NOT EXISTS shortcuts (
  command_id   TEXT PRIMARY KEY,
  key_binding  TEXT NOT NULL,
  context      TEXT NOT NULL DEFAULT 'global',
  updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS trips (                               -- computed by map-view clustering
  id                 INTEGER PRIMARY KEY,
  name               TEXT,
  start_at           TEXT NOT NULL,
  end_at             TEXT NOT NULL,
  center_lat         REAL NOT NULL,
  center_lng         REAL NOT NULL,
  radius_km          REAL NOT NULL,
  photo_count        INTEGER NOT NULL,
  auto_generated     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS trip_photos (
  trip_id   INTEGER NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
  photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  PRIMARY KEY (trip_id, photo_id)
);

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '5', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
```

## API surface (new commands)

Prompt edit:
- `async fn prompt_edit_start(photo_id: i64, prompt: String, strength: u8, mask_id: Option<MaskId>, constraints: Vec<String>) -> Result<PromptJobId>`
- `async fn prompt_edit_status(job_id: PromptJobId) -> Result<PromptJobStatus>`
- `async fn prompt_edit_cancel(job_id: PromptJobId) -> Result<()>`
- Event `chronimage.prompt.progress { job_id, percent, preview_b64? }`
- Event `chronimage.prompt.done { job_id, edit_id }`

Tweaks / shortcuts:
- `async fn tweaks_get() -> Result<Tweaks>`
- `async fn tweaks_set(tweaks: Tweaks) -> Result<()>`
- `async fn shortcuts_list() -> Result<Vec<Shortcut>>`
- `async fn shortcuts_set(command_id: String, key_binding: String) -> Result<()>`

Map:
- `async fn map_trips() -> Result<Vec<Trip>>`
- `async fn map_photos_in_bounds(bounds: BoundingBox) -> Result<Vec<PhotoPin>>`
- `async fn map_tile(z: u32, x: u32, y: u32) -> Result<Vec<u8>>`  — reads from cache, fetches if missing

XMP:
- `async fn xmp_enable_write_out(enabled: bool) -> Result<()>`

## Entitlements

- `Feature::PromptEdit` — gates Prompt tab + generation commands. Already defined in Phase 1; flip to require Pro in a future release.
- `Feature::ModelDownload` — gates the first-run download flow (bandwidth + disk consent).
- Tweaks / map / shortcuts / XMP are all free.

## Exit criteria (test-bound)

- [ ] `src-tauri/tests/phase_4_prompt_roundtrip.rs` — mocked sidecar: prompt + mask → job id → event stream → saved `edits` row with `kind='prompt'` and prompt stored in `operations_json`
- [ ] `src-tauri/tests/phase_4_sidecar_restart.rs` — kill sidecar mid-job → auto-restart within 2 s → job either resumes or fails cleanly with `error_msg`
- [ ] `tests/e2e/phase-4-tweaks-live.spec.ts` — change accent swatch → `<html data-accent>` attribute updates within 100 ms
- [ ] `src-tauri/tests/phase_4_map_trip_clustering.rs` — 5000 GPS-tagged photos → deterministic trip set (seeded clustering)
- [ ] `src-tauri/tests/phase_4_xmp_roundtrip.rs` — import photo with `.xmp` sidecar containing rating=4 + subject=[portrait, golden-hour] → DB has matching rating + tags; re-export sidecar matches byte-identical (respecting XMP packet ordering)
- [ ] `tests/e2e/phase-4-shortcut-overlay.spec.ts` — `?` opens overlay listing all registered shortcuts; rebinding updates immediately and persists across restarts
- [ ] `src-tauri/tests/phase_4_tile_cache.rs` — first tile request misses + fetches + caches; second request reads from disk in < 5 ms

## Open questions

- **Flux-dev vs SDXL-Inpaint default**: Flux looks better but the 12 GB cost is steep. Default to SDXL-Inpaint and offer Flux as "Pro tier" later?
- **Python sidecar vs candle-rs native**: Python is the easy path but adds a 200 MB `runtime/` dir. candle-rs progress on Flux ops is fast-moving — reassess quarterly.
- **XMP write-out default**: Opt-in vs opt-out? Lean opt-in — users who migrate from Lightroom will want it; users who don't care won't know it's off.
- **Map tiles legal**: OpenStreetMap's Nominatim has rate limits. For high-volume users we'd need Mapbox / MapTiler (paid). Ship with OSM, clearly labeled; expose a Settings → "Map tile provider" to swap in the user's own Mapbox token.
- **Shortcut conflict resolution**: hard-block or warn? Lean warn with a visual indicator in the Settings table.

## TODO log

- [ ] Migration `20261001000000_phase4_polish.sql`
- [ ] `src-tauri/src/prompt/` module (sidecar, jobs, constraints)
- [ ] `src-tauri/src/map/` module (trip clustering, tile cache)
- [ ] `src-tauri/src/xmp/` module (parser, writer)
- [ ] `src/screens/map/` new screen
- [ ] `src/components/TweaksPanel.tsx` + `src/components/ShortcutOverlay.tsx`
- [ ] `src/components/ExportSheet` — add the Phase 3-gated toggles (auto-light, copy-paste-edits)
- [ ] SAM2 + CLIP text encoder integration
- [ ] Flux / SDXL-Inpaint sidecar download + start scripts
- [ ] `reqwest` + `leaflet` / `maplibre-gl-js` + `supercluster` dependencies
- [ ] 7 exit-criterion test files
