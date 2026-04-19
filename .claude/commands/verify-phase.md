---
description: Run the current phase's exit-criteria test suite and report pass/fail per criterion.
---

Verify the current phase's exit criteria are met.

Steps:

1. Read `CLAUDE.md` to find the current phase number.

2. Open `docs/prds/phase-N.md` and locate the "Exit criteria" section.

3. For each exit criterion that has an associated test (e.g., `tests/e2e/phase-1-import-throughput.spec.ts`), run it and report pass/fail.

4. Run the standard gauntlet regardless of phase:
   ```bash
   pnpm typecheck
   pnpm exec biome check .
   pnpm exec vitest run
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path src-tauri/Cargo.toml
   ```

5. Print a markdown table:
   | Criterion | Test | Status | Notes |
   |---|---|---|---|

6. If ALL criteria pass, suggest flipping the phase marker in `CLAUDE.md` and creating the next phase's PRD with `/prd phase-N+1`. Do not flip it automatically.

7. If any fail, update `docs/checkpoints/latest.md` with the failures as open threads.
