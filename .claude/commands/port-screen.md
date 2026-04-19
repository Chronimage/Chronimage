---
description: Port a design screen from design-handoff/ JSX to src/screens/ TSX with typing, Tailwind, and Tauri invoke stubs.
argument-hint: <screen-name e.g. catalog, cull, develop>
---

Port the design's `$ARGUMENTS` screen into React + TypeScript, preserving visual parity against `design-handoff/chronimage/project/Chronimage.html` + `styles.css`.

Steps:

1. Read the source:
   - `design-handoff/chronimage/project/src/screens_$ARGUMENTS.jsx` (or the closest match — some screens are combined, e.g. onboard, cullbin, catalog)
   - `design-handoff/chronimage/project/styles.css` (for class names used in the JSX)
   - `design-handoff/chronimage/project/src/primitives.jsx` and `placeholders.jsx` (for Icon/Chip/Seg/Slider/Toggle/Placeholder usage — these are already ported under `src/primitives/`)
   - `src/styles/tokens.css` (oklch + font variables are already defined — use these, don't inline colors)

2. Create `src/screens/$ARGUMENTS.tsx` (and `src/screens/$ARGUMENTS/` subfolder if the screen has many children like onboarding).

3. Translation rules:
   - JSX → TSX with explicit prop types for every component.
   - `window.FOO` global access → proper imports from `src/state/` or `src/tauri/`.
   - `React.useState` globals → import `useState`, `useEffect`, etc. from React.
   - `Object.assign(window, {...})` at file end → proper `export` statements.
   - Inline `style={{...}}` is OK for one-offs, but repeated patterns → Tailwind utility class or `@apply`-style in the screen's CSS module.
   - Class names like `sidepanel`, `toolbar`, `canvas`, `display`, `mono`, `chip`, etc. — keep them; the tokens.css/global.css carries the styles.
   - Replace `Placeholder` usage with the Tauri-side `<Thumbnail photoId={…} />` component once Phase 1 thumbnails exist; for Phase 0, keep the design's striped SVG Placeholder.
   - Any data that came from `PHOTOS` / `ALBUMS` / `SOURCES` stubs → abstract behind a `useCatalog()` / `useSources()` hook from `src/state/`. In Phase 0 those hooks return the same stub data.
   - Keyboard handlers the design didn't wire → leave as TODO(cc) comments with the intended shortcut.

4. Do NOT change visual layout. If you're tempted to "simplify", stop and flag it to the user.

5. After writing, run `pnpm typecheck` and report any errors you can't resolve.

6. Do not run `biome check` — it runs automatically via PostToolUse hook.

7. Print a diff summary of: new files, modified state stores, anything you stubbed with TODO(cc).
