# ADR 0007 — Catalog masonry grid · EXIF orientation · face clustering wire · inspector rehaul

**Status:** Accepted
**Date:** 2026-04-25
**Phase:** Phase 2 week 2 (catalog + People rehaul — pulled ahead of Cull work after real-photo testing)

## Context

First real-library ingest of 111 Sony A7 IV JPGs (Ugadi 2026) surfaced four orthogonal failures:

1. **Portrait photos rendered rotated 90°** on every grid and in the detail view. The `image` crate's `image::open()` does not auto-apply EXIF orientation; thumbnails cached at import and generated on demand both landed in raw sensor orientation. The EXIF reader never extracted `Tag::Orientation`, so `photos.orientation` stayed at default `1` for every photo. The catalog grid additionally cropped portraits via `.libgrid .cell { aspect-ratio: 4/3 }`.
2. **People screen showed "No face clusters yet"** despite 145 detected faces. The HDBSCAN implementation at `src-tauri/src/ai/cluster.rs` existed and was tested, but every `cluster_faces` call site was inside `#[cfg(test)]`. Production pipeline populated `faces` rows with `cluster_id = NULL`.
3. **Catalog was visually flat.** Fixed 4:3 equal-width cells, 170 px minimum column width. The library felt dense but uninformative — nothing surfaced the photo's composition.
4. **Detail inspector was half-wired.** Quality bars for Aesthetic / Sharpness / Face clarity / Eyes-open all rendered at 0% (only Face clarity had a real backend). AI Tags section said "No tags yet. Auto-tagging runs as part of import (SigLIP + face clustering)" — misleading, because SigLIP runs but produces no textual tags, and clustering was broken.

Cull + Export are Phase 2's on-paper scope, but shipping them on top of a visibly broken catalog would have compounded the felt quality gap. The rehaul pulls Phase-2 UX debt forward before the feature work.

## Decision

Four coupled fixes land together as a catalog + People rehaul:

### A. EXIF orientation end-to-end

- Extended `ExifData` at `src-tauri/src/import/exif.rs` with `pub orientation: Option<u32>` (TIFF spec values 1–8).
- New module `src-tauri/src/ai/image_util.rs` with `apply_exif_orientation(img, orientation) -> DynamicImage` — 8-case dispatch using `DynamicImage::rotate90/180/270/fliph/flipv`. Eleven unit tests cover all orientation cases.
- Applied at both thumbnail generation sites:
  - `import/pipeline.rs` Stage 2.6 (cache build)
  - `commands.rs::generate_thumbnail_bytes` (on-demand path)
- `photos.orientation` column populated at import (schema already had it).
- Masonry grid and detail inspector read orientation back from `PhotoRow` to swap width/height for aspect-ratio calculations when value ∈ {5, 6, 7, 8}.
- `rebuild_thumbnails` command re-generates the cache for existing photos so pre-rehaul imports inherit the fix without re-importing.

### B. Face clustering wired into the pipeline

- New module `src-tauri/src/ai/cluster_persist.rs` with `reeval_clusters(pool) -> AppResult<ReclusterReceipt>`:
  1. Loads all `(faces.id, faces.embedding)`.
  2. Calls `cluster::cluster_faces(inputs, &ClusterParams::default())`.
  3. Matches new clusters to existing by centroid cosine similarity ≥ 0.60 (`NAME_INHERIT_COSINE_THRESHOLD`) to preserve `clusters.name` + `is_named` across re-runs.
  4. Updates `faces.cluster_id` in batches.
  5. Prunes `clusters` rows with `face_count = 0` (unless `is_named = 1` — named clusters survive as empty placeholders so a user doesn't lose a name when photos are deleted).
  6. Picks `cover_face_id` per cluster = highest-quality face.
- Auto-triggered at the end of every import (best-effort: warn on failure, don't fail the import).
- User-triggered via `recluster_faces` command + `chronimage://recluster-progress` events. PeopleScreen header gets a "Re-cluster" button; empty state detects `faces > 0 && clusters == 0` and CTAs to re-cluster.

### C. Catalog masonry grid (full Pinterest-style)

- New component `src/screens/catalog/MasonryGrid.tsx`. Absolute-positioned, shortest-column-first packing. Honors per-photo aspect ratio using EXIF-corrected (width, height). ResizeObserver re-packs on container width change; scroll-driven visibility with a ±800 px buffer.
- Replaces `VirtualGrid` in `CatalogScreen` for both the main grid and the detail filmstrip-less mode.
- `MIN_COLUMN_WIDTH` tied to `tweaks.gridDensity`: `compact` = 180, `comfortable` = 220, `spacious` = 340. Grid/Stack toggle in the toolbar switches between comfortable and spacious.
- Dropped `.libgrid .cell { aspect-ratio: 4/3 }`. New `.cell-masonry` class is absolute-positioned with `overflow: hidden`.
- Sort dropdown with six options persisted to `tweaks.sortBy`: captured desc/asc, imported desc, filename asc, aesthetic desc, random. Wired through `ListPhotosParams.sort_by` to a new `sort_by_to_sql` helper in `commands.rs`.

### D. Detail inspector overhaul

- `.detail-hero .ph` dropped its `aspect-ratio: 3/2` lock; now uses `aspect-ratio: auto` + `object-fit: contain` so portraits display portrait.
- Sharpness score computed via `laplacian_variance` on the 320 px thumbnail during Stage 2.6 (~10 ms/photo, zero extra I/O). Stored in `photos.sharpness_score` (schema already had the column). Display maps to 0–800 range (800+ = tack sharp, <100 = blur).
- Quality bars now hide Face clarity when `best_face_quality == null` and Eyes-open when `min_eyes_open == null` (no dedicated eye-open detector yet — deferred to Phase 3 §9).
- Toolbar Rate/Flag/Develop/Export buttons soft-disabled (`disabled` + `.phase-gated` class + `title` pointing to the correct phase).

### E. Google-Photos-style selection indicator

- `.sel-indicator` overlay in the top-left of each masonry cell. Hidden by default, reveals on hover (`opacity: 1`). Clicking the indicator toggles selection without opening the detail view (`e.stopPropagation()`). Clicking the cell body opens detail view (preserved behaviour).
- Selected cells dim slightly (`transform: scale(0.95)`) so a large selection reads en masse.

### F. Typography + layout polish

- New `.page-title` class with `font-family: var(--display-font); font-size: clamp(32px, 4vw, 56px);` and italic-em period treatment. Applied uniformly on Catalog hero, People screen, Settings header.
- `.sidepanel .section-label` padding tightened from `14px 16px 6px` → `8px 16px 4px`.

## Deferrals (explicitly filed in phase PRDs)

This rehaul intentionally stops at the bug-fix + masonry + inspector layer. The following are explicitly homed in named phases so no item lingers as an implicit stub:

| Item | Target phase | PRD section updated |
|---|---|---|
| Rate button (detail inspector) | Phase 2 | `phase-2.md` §1 |
| Flag button (detail inspector) | Phase 2 | `phase-2.md` §2 |
| Manual user tagging | Phase 2 | `phase-2.md` §10 (new) |
| Eyes-open detector | Phase 3 | `phase-3.md` §9 (new) |
| Reverse-geocoder (GPS → place names) | Phase 4 | `phase-4.md` §5 |
| Mobile/tablet responsive pass | Post-v1 | `phase-5.md` TODO |

Optional-big work from the plan that is **not** in this ADR and will land in follow-up commits if picked up:

- **Phase C3 Highlights bento** above the masonry — new `Highlights.tsx` + `highlights(limit)` backend command.
- **Phase C4 six facets** (People / Cameras / Events / Places / Colors / Objects) — includes two new pipeline stages (dominant colors + zero-shot object tagging), new migration `20260426000000_photo_colors.sql`, `FacetBar.tsx`, eleven new commands. Scope is large enough to be its own ADR.
- **Phase D5 responsive breakpoints** via CSS `@container` queries — sidebar collapse at 1280 px, drawer below 900 px. The app is desktop-first and the current fixed layout does not actively break down to ~1024 px, so this is deferred without a user-visible regression today.

## Consequences

**Positive:**
- Portrait photos display correctly everywhere, first time.
- People screen populates after the first import — the feature now works end-to-end.
- Masonry lets the library's compositional variety read at a glance; aspect-ratio preservation is the single biggest visible change.
- Name preservation in `reeval_clusters` means users can rename a cluster ("Ari") and re-import or re-cluster without losing the label.
- Quality + selection UX patterns are now consistent with Google Photos / Apple Photos, reducing the learning curve for imports from those sources.

**Negative / trade-offs:**
- Masonry virtualiser is hand-rolled rather than `@tanstack/react-virtual` — we own its correctness. Mitigated by container-width + viewport-range tests on `MasonryGrid`.
- `reeval_clusters` is O(n²) on face centroid matching. At 10 k faces it takes ~10 s. Fine for v1 libraries; would need a nearest-neighbour index (sqlite-vec) past 50 k.
- Sharpness score's 0–800 display range is empirical. Some lens/subject combos land above that clip; the bar saturates rather than misreports.
- Deferring facets + Highlights means the facet chip row renders with no interactive filters today. All six chips are decorative until Phase C4 lands.

## Alternatives considered

- **Fix rotation in the frontend CSS (`transform: rotate(90deg)`)** — would double-render-cost and still crop under `aspect-ratio: 4/3`. Rejected in favour of a pre-rotated thumbnail cache.
- **Skip HDBSCAN, ship a name-a-face manual-only workflow for v1.** Rejected because the "this is AI" value prop is face clustering, not manual folder-ing. The HDBSCAN implementation already existed; the fix was a three-line call-site addition plus the name-preservation matcher.
- **Row-based virtual grid with variable row heights** (keep `@tanstack/react-virtual`) — simpler, but forces uniform column widths across a row, which defeats the Pinterest feel. Rejected.

## References

- Plan document: `C:\Users\jayas\.claude\plans\glistening-questing-hearth.md`
- Feature inventory + Phase C4 facet details + verification walkthrough live in the plan.
- Subsequent ADR 0008 covers the detail inspector redesign + unified typography + sort control in closer detail.
