# ADR 0005 — Remove the onboarding wizard; make the catalog screen the app's home

**Status:** Accepted
**Date:** 2026-04-23
**Phase:** Phase 2 week 1 (UX hotfix pulled ahead of Cull+Export work)

## Supersedes / amends

- Supersedes [ADR 0003](./0003-bundled-default-models.md) §Onboarding change — that ADR shrank onboarding from 5 steps to 4 by moving model selection into Settings. This ADR deletes the wizard entirely.
- Amends the Phase 1 PRD (`docs/prds/phase-1.md` §13 "Onboarding / first-run UX") — the 4-step stepper described there is no longer the app's first-run surface.

## Context

The Phase-1 4-step onboarding (Welcome → Sources → Import → People-naming) was built to the design handoff and shipped ~1 500 lines in `src/screens/OnboardScreen.tsx`. Real user-testing on this branch surfaced three structural problems:

1. **No first-run flag was persisted.** The default `ScreenId` was `'onboard'` ([ui.ts:72](../../src/state/ui.ts#L72)) and nothing wrote an `onboarding_complete` marker anywhere. Every cold-restart returned to the wizard until the user navigated away mid-session.
2. **"Skip setup" was a non-interactive `<div>`** ([OnboardScreen.tsx:1469](https://github.com/Chronimage/Chronimage/blob/…)) — visually implied as a link, functionally inert. No working escape hatch.
3. **`catalogMode` (`consolidate` vs `index_in_place`) was TypeScript-only.** The Rust backend had zero knowledge of it. Its only effect was toggling visibility of a lift-and-shift panel. The choice was made up-front (Step 1) but didn't bind to anything the pipeline acted on.

Stepping back: the wizard was asking users to make five decisions (mode, add source, add another source, watch progress, name people) before they could see a single catalogued photo. For a "one library, your photos" app the friction-to-value ratio was wrong.

## Decision

Kill the wizard. Make the catalog screen the first-run surface. Move the single real decision ("index vs consolidate") inline where it matters — the moment the user is about to add a source.

### First-run surface

- [`ui.ts`](../../src/state/ui.ts): `'onboard'` removed from `ScreenId` and `SCREENS`; default screen is `'catalog'`.
- [`Rail.tsx`](../../src/chrome/Rail.tsx): "Sources" nav item removed — sources are managed inline in the catalog side panel.
- [`app.tsx`](../../src/app.tsx): `'onboard'` case deleted from the screen switch. `useImportProgressListener()` now mounts once at the app root so every screen sees the same import state (was previously scoped to `OnboardScreen`).
- [`OnboardScreen.tsx`](../../src/screens/OnboardScreen.tsx) + test file: deleted entirely (~1 500 LoC removed).

### Catalog empty-state

When `sources.length === 0 && photos.length === 0` the catalog grid is replaced by [`CatalogEmptyState`](../../src/screens/catalog/CatalogEmptyState.tsx) which delegates to [`AddSourcePopover`](../../src/screens/catalog/AddSourcePopover.tsx).

`AddSourcePopover` presents:
1. A **required radio choice**: `Index in place` | `Consolidate`. **Neither is pre-selected** — this is a deliberate UX decision; a default would bury a consequential "what will this do to my files?" decision. With neither selected the three action buttons stay disabled.
2. Three action buttons, unlocked once a mode is picked:
   - **Add a folder** (local / SD / external) — inline handler.
   - **Connect iCloud** (auto-detects the sync folder) — inline handler.
   - **Google Photos** — navigates to `Settings → Cloud sources` where the full OAuth + picker flow lives (`GooglePhotosPanel`, reused, not duplicated).

Mode is persisted via `tauri-plugin-store` as `default_import_mode` (`'index_in_place' | 'consolidate' | null`). Catalog home is persisted as `catalog_home_path`. Both live in a new [`src/state/settings.ts`](../../src/state/settings.ts) module — separate from `useUi` (cosmetic tweaks), because import mode has on-disk consequences.

### Steady-state catalog chrome

- [`CatalogSidePanel`](../../src/screens/catalog/CatalogSidePanel.tsx) gains three things:
  - [`ImportProgressCard`](../../src/screens/catalog/ImportProgressCard.tsx) — replaces the hardcoded `99.6% Cataloging` fixture with a live progress card reading from the global `useImportStore`. Collapses to nothing when no imports are active; each running import gets a row with an `INDEX`/`CONSOLIDATE` pill.
  - [`SourcesPanel`](../../src/screens/catalog/SourcesPanel.tsx) — replaces the inert sources list. Gains a real `+` button that inline-expands `AddSourcePopover` (layout="inline"), plus click-to-disconnect on each source row (routes to the modal described in ADR 0006).
  - Restructured markup: fixed head + scrollable `.sidepanel-body` + sticky `.sidepanel-footer`. The prior flat column couldn't scroll when `AddSourcePopover` expanded, clipping the iCloud / Google Photos buttons. Grid rule in [`global.css:181`](../../src/styles/global.css#L181).

- [`SettingsScreen`](../../src/screens/SettingsScreen.tsx) gains a Library section with:
  - Radio for `default_import_mode` (so users can flip the global default after first run).
  - Read-only display of `catalog_home_path` + a "Change…" picker. Explicitly labelled: existing photos are **not** migrated on change — new consolidations only.

### Global import state

New [`src/state/import.ts`](../../src/state/import.ts) Zustand store holds `active: Map<importId, ActiveImport>` and exposes:
- `register(...)` — called by every source-adding handler so progress events have a row to update.
- `applyProgress(e)` — subscriber for `IMPORT_PROGRESS_EVENT`.
- `useActiveImports()` — derives a sorted array via `useMemo` over `s.active` (returning a fresh-sorted array from the selector itself tripped Zustand's identity check and infinite-looped during testing — see commit that added the `useMemo` layer).

Each `ActiveImport` carries `mode: 'index_in_place' | 'consolidate'` captured at `register` time, so the UI can show the per-import mode pill even after the user flips the global default mid-run.

### Settings screen width fix

The `.body` grid is `grid-template-columns: 56px auto 1fr auto`. Screens without a `sidePanel` (Settings, People) were rendering their main panel in column 2 (`auto`) because React skipped the null slot, sizing the panel to content width (~1200 px) instead of the full viewport.

Fix in [`app.tsx`](../../src/app.tsx): always emit an empty `<div />` for the side-panel slot when absent (`{sidePanel ?? <div />}`). Keeps the grid columns aligned so the main panel always lands in the `1fr` column.

## What was deliberately *not* done

1. **No iPhone-USB entry point in AddSourcePopover (yet).** The existing flow in `OnboardScreen` was ~120 lines of inline device enumeration + pairing. Carrying that forward into this UI felt speculative — iPhone-via-USB is niche compared to iCloud + Google Photos + local folders. Users can still pair an iPhone after the app opens by going through the existing `listIphoneDevices` surface we'll wire into Settings in a follow-up. Filed as a known gap in the plan file.

2. **No auto-consolidate after import.** The plan flirted with threading `catalog_mode` through `start_import` so the Rust pipeline would auto-fire `lift_shift_execute` when the mode is `consolidate`. Decision: keep backend unchanged, make the frontend's import listener drive lift-and-shift after import completes (respects the existing `plan_lift` → `execute_lift` two-step confirm-token pattern). Not yet implemented — tracked as a Phase-2 follow-up.

3. **No per-source mode persistence.** Considered adding `import_mode` to the `sources` table. Decided the global setting is sufficient for v1 — per-source overrides add migration churn for a niche case. Can be added later without breaking this design.

## Alternatives considered

1. **Keep the wizard but gate it behind `onboarding_complete`.** Rejected — even with a working skip, the wizard still asks five questions before the user sees any value. The actual blocker is the decision load, not the flag.
2. **Pre-select `index_in_place` as the default mode (less friction).** Rejected — "when I imported, where did my files go?" is the kind of question a user must form an opinion on before anything touches their disk. The tiny friction of reading two radio labels is worth it for informed consent.
3. **Keep `OnbLiftPanel` as a Settings-screen action.** Rejected — it's redundant with the inline mode chooser + the future auto-consolidate flow. Cleanup.

## Verification

- `pnpm typecheck` clean.
- `pnpm exec biome check .` clean (0 errors; only the pre-existing 20 CSS descending-specificity warnings).
- `pnpm exec vitest run` — 85/85 passing, including:
  - `ConfirmDialog.test.tsx` (6)
  - `import.test.ts` — global store (4)
  - `ui.test.ts` — default screen updated to `'catalog'`
  - `CatalogScreen.test.tsx` — empty-state renders mode chooser instead of facet bar
  - `app.test.tsx` — Rail shows 5 primary buttons + Settings (was 6 + Settings)
- Manual cold-restart: lands on catalog empty state, not wizard. ✓

## References

- [`src/app.tsx`](../../src/app.tsx) — app-root switch + listener mount
- [`src/state/ui.ts`](../../src/state/ui.ts) — default screen
- [`src/state/import.ts`](../../src/state/import.ts) — global import store
- [`src/state/settings.ts`](../../src/state/settings.ts) — plugin-store wrappers
- [`src/primitives/ConfirmDialog.tsx`](../../src/primitives/ConfirmDialog.tsx) — reusable modal
- [`src/screens/catalog/AddSourcePopover.tsx`](../../src/screens/catalog/AddSourcePopover.tsx)
- [`src/screens/catalog/CatalogEmptyState.tsx`](../../src/screens/catalog/CatalogEmptyState.tsx)
- [`src/screens/catalog/SourcesPanel.tsx`](../../src/screens/catalog/SourcesPanel.tsx)
- [`src/screens/catalog/ImportProgressCard.tsx`](../../src/screens/catalog/ImportProgressCard.tsx)
