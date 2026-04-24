# Checkpoint 2026-04-26 · Phase 2 — all screens navigable, design-handoff retired

Branch: `develop` · clean working tree (PR #58, #59, #60 merged)

## Summary

Three PRs landed today in sequence. Catalog + People rehaul (#58) had already shipped; on top of that we wired full interactive frontend stubs for the three remaining Phase-2/3 screens (Cull, Cull Bin, Develop) and retired the `design-handoff/` bundle now that every screen is authored in TSX directly.

End state: every rail destination renders a real, interactive surface. No `PlaceholderScreen` left in the routing switch. Every backend-bound action is explicitly `.phase-gated` with a per-feature tooltip ("Coming in Phase 2" / "Coming in Phase 3") so users can see the roadmap without hitting silent stubs.

## What shipped

### PR #58 — catalog + People rehaul · [ADR 0007](../adr/0007-catalog-masonry-and-clustering-wire.md)

See the previous checkpoint at [`2026-04-25-phase2-week2-rehaul.md`](2026-04-25-phase2-week2-rehaul.md) for the full details. One-line: EXIF orientation fix end-to-end, face-clustering wired into the production pipeline, Pinterest-style masonry grid, Google-Photos selection indicator, detail inspector overhaul, sort control, typography unification.

### PR #59 — cull / cullbin / develop stubs

All three screens now render a complete, interactive UI over live catalog photos. Keyboard navigation + local state work; real actions (verdict persistence, restore/empty, export, RAW develop) are phase-gated.

- **Cull** (`src/screens/cull/`) — Compare / Grid / Swipe modes, AI-pick + issue chips, keyboard verdicts (←/→/A/B/↵), kept/rejected tallies tick in real time. Shared state via new `src/state/cull.ts` Zustand store.
- **Cull Bin** (`src/screens/cullbin/`) — recoverable-rejects list with per-row Restore/Delete (phase-gated), bulk select, 8.2 GB reclaimable sidebar with 30-day retention copy.
- **Develop** (`src/screens/develop/`) — Develop / Mask / Prompt tabs; full inspector (Auto · Light · Curves with RGB/R/G/B/L + histogram + curve SVG · Color · Detail · Copy·Paste·Sync · Export); preset sidebar with 5 categories + active-preset sticky + strength slider. Sliders fully interactive locally; Auto-light applies sane defaults. Prompt tab has before/after split + textarea + strength slider.
- **Coverage:** +10 smoke tests (`src/screens/{cull,cullbin,develop}/*.test.tsx`) lifted overall coverage from 43.66% → 60.14% lines / 47.5% functions, passing the 50 / 40 thresholds.

### PR #60 — drop `design-handoff/` and its constraints

The `design-handoff/` directory held the original claude.ai/design JSX bundle. Every screen backed by it is now either ported to TSX or stubbed in PR #59, so the bundle is no longer load-bearing.

Removed: `design-handoff/` (all chats + project source + styles), `.claude/agents/ux-porter.md`, `.claude/commands/port-screen.md`.

Cleaned: `CLAUDE.md` (repo-map + Design handoff section + ux-porter subagent reference), `.claude/settings.json` (cp-r allow + edit/write denies), `scripts/forbidden-patterns.cjs` + `.claude/hooks/post-edit-format.cjs` (skip-path lists), `biome.json` + `tsconfig.json` + `vite.config.ts` + `vitest.config.ts` (exclude lists), `.claude/commands/context-dump.md` (bundle-exclusion list), `src/screens/SettingsScreen.tsx` + `src/primitives/Icon.tsx` + `src/styles/tokens.css` (header comments), `docs/prds/phase-0.md` (checklist lines referencing ported-from-design).

Left intentionally untouched: `docs/context-bundles/*.md` and historical `docs/checkpoints/*.md` — immutable session history.

## Test posture

- **Rust:** 278 lib tests pass (unchanged from PR #58 — no backend surface touched in #59/#60). Clippy `--all-targets -D warnings` clean.
- **Frontend:** 95 vitest pass (85 + 10 new stub smoke tests). `pnpm typecheck` + `pnpm exec biome check .` clean.
- **Coverage:** 60.14% lines / 47.5% functions (threshold 50 / 40).

## What's explicitly deferred (next session's candidates)

From the rehaul plan that didn't land in #58:

1. **Phase C3 — Highlights bento** above the masonry. One new component + `highlights(limit)` backend command (top-N by `aesthetic_score DESC`). ~2 h.
2. **Phase C4 — Six facets** (People / Cameras / Events / Places / Colors / Objects). Pipeline Stage 2.7 (dominant OKLCH colors) + Stage 4.5 (zero-shot object tagging via SigLIP text prompts), migration `20260426000000_photo_colors.sql`, `FacetBar.tsx` + `FacetSubList.tsx`, ~11 new commands, 2 backfill commands. ~1–2 full days.
3. **Phase D5 — Responsive breakpoints** via CSS `@container` queries. Sidebar collapse at 1280 px, drawer below 900 px. ~3 h.
4. **Face-crop covers for PeopleScreen.** `get_face_thumbnail(face_id, size)` command — crop to face bounding box with 20% padding, cache. ~2 h.

From the Phase-2 PRD that's now unblocked by the stubs:

5. **Cull verdict engine** (`phase-2.md §2`) — replace the stub's local Zustand state with a real `cull_verdicts` SQLite table + `apply_verdict` Rust command. Rate / Flag verdicts land from the detail inspector too.
6. **Cull Bin backend** (`phase-2.md §3–§4`) — `list_cull_bin`, `restore_from_bin`, `empty_bin`, 30-day auto-empty sweep.
7. **Export sheet** (`phase-2.md §5–§6`) — modal + engine. First time any of the Export buttons in Catalog / Cull / Develop stop being phase-gated.
8. **Manual tagging** (`phase-2.md §10`, new in ADR 0007's PRD updates) — multi-select → Tag dropdown → add/remove user tags (`tags.kind='user'`), inline-editable in detail inspector.

## Next action

Two natural starting moves:

- **Ship Cull end-to-end** — replace the Cull screen's local Zustand state with real backend persistence. See `phase-2.md §1–§4`. Highest user value; the stub has already validated the UX.
- **Ship Phase C4 facets** — unblocks the decorative facet chips in Catalog. Big but bounded; two new pipeline stages + eleven commands are the core.

Pick by impact. Both start from `develop` at commit `3d1a9fe`.
