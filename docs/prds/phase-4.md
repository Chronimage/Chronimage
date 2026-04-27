# Phase 4 · Prompt editing + polish

> Natural-language edits via Flux-dev / SDXL-inpaint on-device, map view with GPS clustering, and the keyboard shortcut overlay. The phase that takes Chronimage from "great editor" to "this feels designed."

## Context

Phase 3 ships a solid develop surface covering 80% of hobbyist edits. Phase 4 adds the remaining 20% that require generative models — inpainting, background removal/replacement, prompt-driven tone adjustments — plus the UX polish layer (tweaks, map, shortcuts) that the design already specced but we deferred.

The generative path is intentionally gated behind an entitlement + first-run model download because Flux/SDXL are 6–12 GB each and require explicit user consent to fetch. The map / shortcuts / XMP / ignore-file surfaces don't need models and ship unconditionally.

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
- [x] Before/After split layout (two canvases side by side)
- [x] Prompt textarea, multi-line, auto-grow
- [x] Constraint chips — add/remove with inline input, pinnable
- [x] Strength slider (0–100, default 65)
- [x] Mask button — live when sidecar reachable; invokes `mask_from_prompt` and feeds the result into `prompt_edit.mask_b64`
- [ ] Ctrl+Enter = Generate keybinding — **deferred** (week 4+)
- [ ] History: each generation stored as a new `edits` row with `kind='prompt'` and full prompt/seed/model metadata in `operations_json` — **deferred** (week 4+)
- [ ] Fork (accept result = save edit; reject = drop) — **deferred** (week 4+)

### 2. Generative backend
- [x] OpenAI-compatible HTTP client (`src-tauri/src/prompt/mod.rs`) with `/v1/models` + `/v1/edit` endpoints; reads `ai.prompt_sidecar_url` + `ai.prompt_sidecar_model` KV settings; commands `prompt_sidecar_get/set/model_get/model_set/ping/edit` (2026-04-24 week 3)
- [x] Request/response schema: `{ image_b64, mask_b64?, prompt, strength, constraints }` → `{ image_b64, latency_ms, model_id, seed }`
- [x] Settings → Prompt sidecar section — URL + preferred model fields, Test button, live-coloured status chip
- [ ] Sidecar management via `tauri-plugin-shell` + `externalBin` in tauri.conf.json — **deferred** (week 4+): v3 ships as a client; the user runs the sidecar themselves for now
- [ ] PID-supervised; graceful shutdown on app exit; auto-restart on unexpected exit — **deferred** (week 4+)
- [ ] First-run download UI for Flux-dev (~12 GB) / SDXL-Inpaint (~6 GB) — **deferred** (week 4+)

### 3. Mask-from-prompt
- [x] `mask_from_prompt(photo_id, prompt)` command posts to `{sidecar}/v1/mask` and returns a PNG mask + confidence (2026-04-24 week 3)
- [x] Prompt tab's Mask button captures a text prompt and pipes the resulting `mask_b64` into the next `prompt_edit`
- [ ] Integrated with Phase 3's mask engine (appears as a new mask source alongside AI-subject / sky / foreground) — **deferred** (week 4+): v3 pipes masks directly into generative edits only, not the non-generative mask engine

### 4. ~~Tweaks panel~~ — **removed from Phase 4 scope**

The Settings → Library screen already exposes theme / accent / display font / grid density / facet placement / cull mode / editor layout via the existing `useUi().tweaks` plumbing from Phase 0. A separate drawer UI was deemed redundant; all the same controls are reachable from Settings without the overlay. Closed out 2026-04-24 per user direction.

### 5. Map view
- [x] `src/screens/map/` new screen (added to Rail between Cull Bin and Develop) — Leaflet map + trip list + photo grid; ships 2026-04-24
- [x] Trip clustering (backend `src-tauri/src/map/trips.rs`) — single-pass temporal (48 h gap) + spatial (30 km centroid cutoff) Haversine clustering; auto-runs post-import
- [x] Commands `map_recompute_trips`, `map_list_trips`, `map_photos_in_trip`
- [x] OpenStreetMap tiles via `react-leaflet` 5 — attribution shown; circle markers per trip centroid, radius scaled by photo count; marker click selects trip (2026-04-24 week 2)
- [ ] OpenStreetMap tiles cached locally to `{data_dir}/tiles/{z}/{x}/{y}.png` — **deferred** (week 4+): needs a Tauri custom scheme handler (`chronimage-tile://…`) wired in `tauri.conf.json` to intercept the Leaflet TileLayer URL. App currently streams tiles from `tile.openstreetmap.org` with attribution.
- [x] Zoom-aware pin clustering — screen-space 80 px cells over the current Leaflet projection; shared cells render a DivIcon count badge that flies in on click; single-cell pins keep the CircleMarker look (2026-04-24 week 3)
- [x] Filter by time range — segmented `7d / 30d / 1y / all` control filters map markers + trip list + fit-to-bounds in one pass (2026-04-24 week 3)
- [x] **Offline reverse-geocoder** — `src-tauri/src/map/geocode.rs` bundled const table of ~120 global cities; Haversine nearest-neighbour; trip centroids auto-named "Bengaluru, IN" / "Tokyo, JP" when within 250 km. Larger GeoNames `cities15000` import (~5 MB SQLite) + `photos.place_label` column + Places facet integration still on the week-4+ plate (2026-04-24 week 3)

### 6. Keyboard shortcut overlay
- [x] Press `?` anywhere → modal showing all shortcuts grouped by context (`src/primitives/ShortcutOverlay.tsx` + `useShortcutOverlay` hook)
- [x] `shortcuts` table + `shortcuts_list` / `shortcuts_set` commands for per-command overrides
- [x] `useShortcut(commandId, defaultBinding, handler)` hook — registers a keydown listener that respects any override stored in `shortcuts` (2026-04-24 week 2)
- [x] Click-to-rebind inside the overlay — captures the next keystroke combo, persists via `shortcuts_set`, with inline **reset** back to the built-in default (2026-04-24 week 2)
- [x] Settings → Shortcuts full table with conflict detection — `src/screens/settings/ShortcutsSection.tsx` lists every shortcut grouped by context; click any binding to rebind; inline reset to default; rows with colliding bindings in the same context render a red `conflict` badge (2026-04-24 week 3)
- [ ] All shortcut data stored in tauri-plugin-store; exported/imported with tweaks — superseded: live in SQLite `shortcuts` table

### 7. Import + write-out via `.xmp` sidecars
- [x] On import, if `photo.xmp` exists alongside `photo.jpg`, parse it (`src-tauri/src/xmp/mod.rs`)
- [x] Map `dc:subject` → `tags` (kind='user'), `xmp:Rating` → `photos.rating INTEGER`, `xmp:Label` → `photos.color_label`
- [x] `xmp_rescan` command for post-hoc library re-scan
- [x] Write-out — opt-in `xmp.write_on_change` setting; `write_sidecar()` emits a minimal Adobe-compatible packet that round-trips through `parse_str`; `add_user_tag` / `remove_user_tag` trigger a best-effort write; `xmp_export_all` backfills the entire library on demand (2026-04-24 week 2)
- [x] Embedded XMP inside `.arw` / `.jpg` — `xmp::read_embedded(path)` scans the first 4 MB for `<x:xmpmeta>`…`</x:xmpmeta>` (works for JPEG APP1 + TIFF/ARW/DNG tag 700 since both wrap the same packet). Pipeline falls through to embedded when no external sidecar exists so in-camera star ratings land in the catalog (2026-04-24 week 3)

### 8. `.chronimage-ignore` file support
- [x] `.gitignore`-style include/exclude at any folder level (via `ignore::WalkBuilder` with `add_custom_ignore_filename`)
- [x] Respected by `scan_dir`
- [x] Default rules: exclude `Thumbs.db`, `.DS_Store`, `@eaDir`, `.thumbnails`, `cache/`

## Non-goals

- No video editing
- No tethered shooting
- No advanced retouching (frequency separation, dodge/burn beyond what masks provide)
- No smart-object / layer system
- No proofing workflows

## Non-functional requirements

- Generative inference latency: < 30 s for 1024×1024 SDXL-Inpaint on 3060-tier; < 45 s for Flux-dev
- Map tile cache hit rate: > 95% for a 100-trip library after first view
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
```

## API surface (new commands)

Prompt edit:
- `async fn prompt_edit_start(photo_id: i64, prompt: String, strength: u8, mask_id: Option<MaskId>, constraints: Vec<String>) -> Result<PromptJobId>`
- `async fn prompt_edit_status(job_id: PromptJobId) -> Result<PromptJobStatus>`
- `async fn prompt_edit_cancel(job_id: PromptJobId) -> Result<()>`
- Event `chronimage.prompt.progress { job_id, percent, preview_b64? }`
- Event `chronimage.prompt.done { job_id, edit_id }`

Shortcuts:
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
- Map / shortcuts / XMP / ignore are all free.

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

- [x] Migration `20261001000000_phase4_polish.sql`
- [ ] `src-tauri/src/prompt/` module (sidecar, jobs, constraints) — **week 2+**
- [x] `src-tauri/src/map/` module (trip clustering — tile cache deferred)
- [x] `src-tauri/src/xmp/` module (parser + opt-in writer)
- [x] `src/screens/map/` new screen (Leaflet renderer + trip list + photo grid)
- [x] `src/primitives/ShortcutOverlay.tsx` + `useShortcut` hook + inline rebinder
- [x] `leaflet` + `react-leaflet` frontend deps (2026-04-24 week 2)
- [ ] `src/components/ExportSheet` — add the Phase 3-gated toggles (auto-light, copy-paste-edits) — **week 3+**
- [ ] SAM2 + CLIP text encoder integration — **week 3+**
- [ ] Flux / SDXL-Inpaint sidecar download + start scripts — **week 3+**
- [ ] `supercluster` + offline reverse-geocoder SQLite — **week 3+**
- [x] 2 of 7 exit-criterion test files (xmp roundtrip, .chronimage-ignore); remaining 5 tied to deferred deliverables

## Week 1 status (2026-04-24)

Shipped end-to-end:

- §5 Map view — trip-clustering backend (Haversine, 48 h / 30 km thresholds) + list-based Map screen reachable from Rail
- §6 Keyboard shortcut overlay — `?` opens a read-only overlay of the documented shortcuts; override table exists for a future rebinder UI
- §7 XMP sidecar import — rating / color label / subjects → catalog rows + tags; idempotent re-import
- §8 `.chronimage-ignore` — gitignore-style ignore files + sane defaults, honoured by `scan_dir`
- Migration `20261001000000_phase4_polish.sql`

Removed from scope:

- §4 Tweaks panel — already reachable from Settings (see strikethrough above)

Deferred to week 2+ (all gated on generative infra or rendering libs we don't yet pull in):

- §1 Prompt tab + §2 Flux/SDXL generative backend — needs the Python sidecar, model downloader, first-run consent flow
- §3 SAM2 mask-from-prompt — needs SAM2 ONNX + CLIP text-encoder plumbing
- §5 Map view renderer — OpenStreetMap tile cache, leaflet/maplibre, pin clustering, time-range filter, offline reverse-geocoder
- §6 `useShortcut` central hook + Settings rebinder UI
- §7 XMP write-out + embedded-XMP-in-ARW

## Week 2 status (2026-04-24)

Shipped end-to-end:

- §5 Map view — real Leaflet map with OSM tiles + circle markers sized by trip photo count; fits to bounds; popups on marker click
- §6 `useShortcut(commandId, defaultBinding, handler)` hook — respects per-user overrides from the `shortcuts` table
- §6 Overlay inline rebinder — click any keys column to capture a new combo + persist; `reset` link reverts to built-in default
- §7 XMP write-out — opt-in `xmp.write_on_change` setting + `write_sidecar` (round-trip safe) + `add_user_tag`/`remove_user_tag` hooks + `xmp_export_all` backfill command
- §1 Prompt tab polish — tooltips corrected (Phase 4 week 3+ instead of stale "Phase 3"); constraints are now add/remove-stateful with an inline input

Still deferred (week 3+):

- §1/§2 Flux + SDXL sidecar (Python runtime, first-run model download, PID supervision)
- §3 SAM2 + CLIP text encoder for mask-from-prompt
- §5 tile cache, supercluster, time-range filter, offline reverse-geocoder
- §6 Settings → Shortcuts table with conflict detection
- §7 Embedded XMP inside `.arw`

## Week 3 status (2026-04-24)

Shipped end-to-end:

- §1 Prompt tab — live sidecar wiring: Generate button enables when `prompt_sidecar_ping` reports reachable; AFTER panel renders the returned base64 image or error banner; Mask button captures a text prompt and calls SAM2 via the sidecar
- §2 Generative backend — OpenAI-compatible HTTP client (`src-tauri/src/prompt/mod.rs`) with `/v1/models` + `/v1/edit` endpoints and `ai.prompt_sidecar_url` + `ai.prompt_sidecar_model` KV settings. Seven new tauri commands (`prompt_sidecar_get/set/model_get/model_set/ping/edit`, plus `mask_from_prompt`). Settings → Prompt sidecar section with Test button and live-coloured status chip.
- §3 SAM2 mask-from-prompt — `mask_from_prompt` command posts to `{sidecar}/v1/mask` and returns `{ mask_b64, confidence }`. Wired to the Prompt tab's Mask button; resulting mask is passed back in the next generate call.
- §5 Map view renderer — zoom-aware pin clustering (screen-space 80 px cells; count-badge DivIcons; click to fly-in), segmented time-range filter (7d/30d/1y/all), offline reverse-geocoder (`src-tauri/src/map/geocode.rs`, 120-city bundled table → auto trip names like "Bengaluru, IN").
- §6 Settings → Shortcuts full table — click-to-rebind, inline reset, same-context conflict detection flagging collisions.
- §7 Embedded XMP — `xmp::read_embedded` scans the first 4 MB of any file for `<x:xmpmeta>`; pipeline falls through from sidecar → embedded so Sony A7 IV in-camera ratings land in the catalog.

Still deferred (week 4+ — each needs infra work beyond pure client code):

- §1 Ctrl+Enter Generate keybinding + `edits` history row for prompt generations + fork/accept/reject flow
- §2 Sidecar process management (`tauri-plugin-shell` + `externalBin`), PID supervision, first-run Flux/SDXL download consent flow
- §3 Mask engine integration — wiring prompt masks into the Phase-3 mask-layer stack alongside AI-subject / sky / foreground
- §5 OSM tile cache — Tauri custom scheme handler (`chronimage-tile://`) to intercept the Leaflet TileLayer URL; respects OSMF fair-use policy
- §5 Full GeoNames `cities15000` (~5 MB SQLite) + `photos.place_label` column + Catalog Places-facet label integration

## Week 4 status (2026-04-24)

Shipped end-to-end:

- §1 Ctrl+Enter Generate keybinding on the Prompt tab textarea + Accept/Reject buttons on the AFTER panel + `prompt_edits` table (migration 20261002, schema v6) persisting every generation with state ∈ {pending, accepted, rejected}. Accept auto-rejects sibling pending rows on the same photo. Three new commands (`prompt_edit_list`, `_accept`, `_reject`).
- §5 `photos.place_label` — migration 20261003 adds the column (schema v7) + partial index. Import pipeline labels new GPS-tagged photos automatically via `map::geocode::label_photo`; `backfill_place_labels` command fills the existing catalog in one transaction.
- §5 OSM tile cache — `src-tauri/src/map/tile_cache.rs` serves tiles from `{data_dir}/tiles/{z}/{x}/{y}.png`, falling through to `tile.openstreetmap.org` with a `Chronimage/0.1` user-agent and caching bytes on disk. MapScreen's `CachedTileLayer` overrides Leaflet's `createTile` to route through `map_tile` + blob URLs. Zoom bounds validated (0..=19).
- §2 Prompt tab health polling — badge re-pings the sidecar every 30 s so the Generate button state stays honest between turns.
- §3 Mask engine integration — n/a: Phase 3 shipped Develop sliders only, not a user-facing mask-layer stack. The prompt's `mask_b64` threads directly through `{sidecar}/v1/edit` which is the full extent of masking v1 ships with.

Still deferred (genuinely week 5+ packaging work):

- §2 Sidecar process management via `tauri-plugin-shell` + `externalBin` + PID supervision + first-run model-download consent UI. Design decision for v1: Chronimage is a sidecar **client**, user runs the generative server themselves (ComfyUI / diffusers / candle-vllm). The health poll covers liveness; the download UX is a packaging initiative on its own.
- §5 Full GeoNames `cities15000` (~5 MB TSV import + SQLite table + Places-facet label integration). The bundled 120-city table plus `photos.place_label` already handles the common cases; GeoNames expansion is additive.
