# Next session — starting moves (post-rehaul)
_Refreshed 2026-04-25_

Phase 2 week 2 catalog + People rehaul is shipped (see [`docs/checkpoints/latest.md`](checkpoints/latest.md) + [ADR 0007](adr/0007-catalog-masonry-and-clustering-wire.md)). Working tree is heavy but all gates green: 278 Rust tests, clippy clean, `pnpm typecheck` + 85 vitest tests clean.

Pick one of these (ordered by leverage on the v1 roadmap).

## 1. Commit the rehaul — then start Cull screen (Phase 2 §1). [~1 d]

- **First command:** `git status` → stage in three logical commits (`feat(catalog): masonry grid + EXIF orientation`, `feat(ai): face clustering wired + re-cluster UX`, `feat(ui): detail inspector + sort + selection polish`).
- **Why highest leverage:** Phase 2's real scope is Cull + Export. The rehaul was a prerequisite; Cull now starts from a catalog that looks finished. Cull screen is the largest single on-PRD v1 feature still un-started.
- **First file to edit after commits:** new `src/screens/cull/CullScreen.tsx` (currently `PlaceholderScreen`). Backend: new `src-tauri/src/cull/` module for pair selection + verdict recording. See [`docs/prds/phase-2.md`](prds/phase-2.md) §1–§2.

## 2. Phase C4 — Six facets (deferred from rehaul). [~1–2 d]

- **First command:** `pnpm migrate:new photo_colors` then edit `src-tauri/src/import/pipeline.rs` to add Stage 2.7.
- **Why:** The facet chips (People / Cameras / Events / Places / Colors / Objects) render but don't filter. Wiring them unlocks the "find photos like this" UX that the masonry + clustering already enable. Two new pipeline stages (dominant OKLCH colors + zero-shot object tags via SigLIP text encoder) are the heavy lifts; the other four facets are pure SQL.
- **Scope blocker:** user-facing value is high, but the catalog still ships without it. If Cull is critical-path, defer.

## 3. Face-crop covers for PeopleScreen. [~2 h]

- **First command:** open `src-tauri/src/commands.rs`, add a `get_face_thumbnail` command next to `generate_thumbnail_bytes`.
- **Why:** PeopleScreen cluster cards currently show full-photo thumbs when `cover_face_id` is set. A face-bounding-box crop (20% padding, EXIF-oriented, cached at `{thumbnails_dir}/face_{face_id}_{size}.jpg`) is the single remaining UX gap on the People rehaul.
- Small, bounded, visible. Good first move if cognitive load from the rehaul is still high.

## 4. Phase C3 — Highlights bento above the masonry. [~2 h]

- **First command:** create `src/screens/catalog/Highlights.tsx` + new `highlights(limit)` Rust command (top-N by `aesthetic_score DESC`).
- **Why:** The catalog hero is dense text; the Highlights strip gives the library a visual anchor above the fold. Low risk, small blast radius, purely additive.

## 5. Phase D5 — Responsive breakpoints. [~3 h]

- **First command:** edit `src/styles/global.css` — add `@container` queries on `.body` at 1280 px + 900 px.
- **Why:** App is desktop-first and doesn't actively break to 1024 px today, but the rehaul reshuffled layout enough that narrower windows start to clip. Not a regression blocker; schedule if polish-pass.

---

## Suggested ordering

- **High-focus day:** 1 (commit rehaul) → 3 (face-crop covers) — ships both in one push.
- **Feature-push day:** 1 → 2 (at least People + Cameras + Events facets).
- **Cull-critical-path:** 1 → Cull scaffolding per PRD §1–§2; pull deferred items later.
