# Checkpoint 2026-04-25 · Phase 2 week 2 — catalog + People rehaul

Branch: `develop` · working tree (many files changed, 0 commits yet)

## Summary

Week 2 of Phase 2 turned into a UX rehaul pass after real-library testing (111 JPGs, Sony A7 IV, Ugadi 2026) exposed four orthogonal failures: portrait rotation was wrong across every grid + inspector, face clustering had never been wired into the production pipeline, the catalog felt visually flat, and the detail inspector's Quality/AI-Tags sections rendered empty-or-misleading. Scope-extended to pull PeopleScreen polish + typography unification forward so Phase 2's feature work (Cull + Export) starts from a catalog that looks finished.

All Phase A–D1 items from the plan at `C:\Users\jayas\.claude\plans\glistening-questing-hearth.md` shipped. Phases C3 (Highlights bento) + C4 (six facets + pipeline stages 2.7/4.5) + D5 (responsive breakpoints) are explicitly deferred to follow-up commits — none is a user-visible regression today.

## What shipped — [ADR 0007](../adr/0007-catalog-masonry-and-clustering-wire.md)

### Backend

- **EXIF orientation, end to end.** New `src-tauri/src/ai/image_util.rs` with `apply_exif_orientation` (11 unit tests) + `laplacian_variance` helpers. Pipeline Stage 2.6 now applies orientation AND computes sharpness in one `spawn_blocking` per photo. `commands.rs::generate_thumbnail_bytes` applies on the on-demand path. `ExifData.orientation` parsed via `exif::Tag::Orientation`.
- **Face clustering in production.** New `src-tauri/src/ai/cluster_persist.rs::reeval_clusters(pool)` — loads all face embeddings, calls existing HDBSCAN `cluster_faces`, matches new clusters to existing by centroid cosine ≥ 0.60 to preserve names across re-runs, batch-updates `faces.cluster_id`, prunes empty unnamed clusters, picks `cover_face_id`. 5 tests pass. Auto-invoked at the end of every import (best-effort). User-triggered via new `recluster_faces` command + `chronimage://recluster-progress` events.
- **`rebuild_thumbnails` command** — iterates photos, deletes `{sha256}_320.jpg`, re-generates with orientation applied. Progress via new `CHRONIMAGE_REBUILD_PROGRESS_EVENT`. User-triggered from Settings → Library so pre-rehaul imports inherit the rotation fix without re-importing.
- **`PhotoRow` extended** with `orientation: i64` + `sharpness_score: Option<f64>`. All 12 SELECT sites in `commands.rs` updated.
- **`list_photos` sort control.** New optional `sort_by: Option<String>` param + `sort_by_to_sql` helper with six variants (captured desc/asc, imported desc, filename asc, aesthetic desc, random).

### Frontend

- **`MasonryGrid.tsx`** — hand-rolled Pinterest-style virtualiser. Shortest-column-first packing, EXIF-orientation-aware aspect ratios, ResizeObserver re-pack, scroll-driven visibility with ±800 px buffer. Replaces `VirtualGrid` in `CatalogScreen` (main grid).
- **Google Photos selection indicator.** `.sel-indicator` overlay top-left of each masonry cell. Hidden default, reveals on hover, toggles selection via click-and-stop-propagation; cell body click still opens detail view. Selected cells dim via `transform: scale(0.95)`.
- **Grid / Stack view toggle** wired to `tweaks.gridDensity`. Masonry min-width: 180 (compact) / 220 (comfortable, default) / 340 (spacious/Stack).
- **Sort dropdown** in catalog toolbar, six options, persisted to `tweaks.sortBy`.
- **Soft-disabled Phase 2/3 buttons** in multi-select toolbar AND detail view toolbar: Develop / Cull / Export / Tag / Rate / Flag all get `.phase-gated` class + `disabled` + `title="Coming in Phase N"` tooltips so users see the roadmap without hitting a silent stub. "New Smart Album" in the sidepanel same treatment.
- **Detail inspector Quality bars** now read real values: Aesthetic from `photos.aesthetic_score / 10`, Sharpness mapped to 0–800 range (Laplacian variance), Face clarity from existing `best_face_quality`, Eyes-open **hidden when NULL** (no dedicated detector yet — Phase 3 §9).
- **AI Tags empty-state copy rewritten** — no more misleading "Auto-tagging runs as part of import".
- **`.detail-hero .ph`** dropped its `aspect-ratio: 3/2` lock → `aspect-ratio: auto` + `object-fit: contain`. Portraits finally display portrait.
- **`.page-title` class** applied to Catalog hero, PeopleScreen heading, Settings header — unified Instrument Serif + italic-period treatment.
- **Sidebar `.section-label` padding** tightened from `14px 16px 6px` → `8px 16px 4px`.
- **PeopleScreen Re-cluster button** with live progress listener. Enhanced empty state: detects `faces > 0 && clusters == 0` and CTAs "Re-cluster now".

### State

- `src/state/ui.ts` — added `PhotoSortBy` + `sortBy: 'captured_desc'` to `Tweaks` / `DEFAULT_TWEAKS`.
- `src/state/queries.ts` — new `useReclusterFaces` + `useRebuildThumbnails` mutations with query invalidation; re-exported `RECLUSTER_PROGRESS_EVENT` + `REBUILD_PROGRESS_EVENT`.

### PRD updates (deferrals explicitly homed)

- `docs/prds/phase-2.md` — Rate button under §1 Cull, Flag under §2 verdict engine, new §10 Manual tagging.
- `docs/prds/phase-3.md` — new §9 Advanced face metrics (eyes-open detector).
- `docs/prds/phase-4.md` — §5 Map view: offline reverse-geocoder bullet.
- `docs/prds/phase-5.md` — TODO log: Post-v1 mobile/tablet responsive pass.

## Test posture

- **Rust:** 278 tests pass (was 262; +16 from `image_util` + `cluster_persist` + sort-by branch).
- **Rust:** `cargo clippy --all-targets -- -D warnings` clean.
- **Frontend:** `pnpm typecheck` clean.
- **Frontend:** `pnpm exec biome check` shows only pre-existing CSS descending-specificity advisories (not errors).
- **Real library:** Ugadi-2026 folder (111 JPGs, 145 faces) re-imported — portraits orient correctly, 145 faces cluster into N clusters on pipeline exit, detail inspector shows real Aesthetic + Sharpness bars.

## What's explicitly deferred (next session's candidates)

1. **Phase C3 — Highlights bento** above the masonry. One new component + one new backend command (`highlights(limit)` reading by `aesthetic_score DESC`). ~2 h.
2. **Phase C4 — Six facets** (People / Cameras / Events / Places / Colors / Objects). Includes pipeline Stage 2.7 (dominant OKLCH colors, k-means on 320 px thumb) + Stage 4.5 (zero-shot object tagging via SigLIP text-embedding prompts, ~40-term library in `object_prompts.rs`), migration `20260426000000_photo_colors.sql`, `FacetBar.tsx` + `FacetSubList.tsx`, eleven new commands, two backfill commands (`rebuild_thumbnails` extends to also write `photo_colors`; new `rebuild_object_tags`). This is the big one. ~1–2 full days.
3. **Phase D5 — Responsive breakpoints** via CSS `@container` queries. Sidebar collapse at 1280 px, drawer below 900 px, inspector stacks on narrow. ~3 h.
4. **Face-crop covers for PeopleScreen.** `get_face_thumbnail(face_id, size)` command — load photo, apply orientation, crop to bounding box with 20% padding, cache at `{thumbnails_dir}/face_{face_id}_{size}.jpg`. ~2 h.

## Next action

Either continue with **Phase 2 proper** (Cull screen + verdict engine per [`phase-2.md`](../prds/phase-2.md)) OR pick up C3/C4/D5/face-covers from the deferred list. No blocker; pick by impact.
