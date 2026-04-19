# Checkpoint · Phase 0 scaffolding session · 2026-04-19 (end-of-session)

## Just finished

Full Phase 0 scaffolding landed. The repo went from empty → complete foundation with:

### Claude Code / agent infrastructure
- `CLAUDE.md` — resume protocol, phase marker, Sonnet-subagent preference noted
- `.claude/settings.json` — permissions/hooks/statusline
- `.claude/hooks/` — `post-edit-format.js`, `pre-bash-guard.js`, `remind-checkpoint.js`, `session-start.js`, `statusline.js`
- `.claude/commands/` — `phase-checkpoint`, `port-screen`, `verify-phase`, `prd`, `plan-next-session`, `import-dry-run`, `ai-model-audit`
- `.claude/agents/` — `catalog-architect`, `ux-porter`, `ai-wrangler`, `raw-pipeline-expert`, `perf-cop`, `release-captain` (all `model: sonnet`)
- User preferences saved to `~/.claude/projects/.../memory/` (user_profile, feedback_subagent_model, project_chronimage, MEMORY.md)

### Repo metadata + docs
- `README.md`, `LICENSE` (TBD placeholder)
- `.gitignore`, `.gitattributes` (Git LFS for RAW fixtures)
- `docs/prds/phase-0.md`, `docs/prds/phase-1.md` (both full PRDs with exit criteria bound to test files)
- `docs/checkpoints/latest.md` — this file

### Build / test / release tooling
- `package.json` — React 19, Tauri v2 client libs, TanStack Router/Query/Virtual, Radix, Zustand, Biome, Vitest, Playwright, lefthook, commitlint, git-cliff
- `tsconfig.json` — strict, noUncheckedIndexedAccess, exactOptionalPropertyTypes, path aliases
- `vite.config.ts`, `vitest.config.ts`, `playwright.config.ts`, `tailwind.config.ts`, `biome.json`
- `lefthook.yml` (pre-commit biome + rustfmt + clippy + typecheck + forbidden-patterns + secrets; pre-push vitest + cargo test)
- `commitlint.config.js` (Conventional Commits, strict scopes)
- `cliff.toml` (changelog generation)
- `cargo-deny.toml` (license audit)
- `scripts/forbidden-patterns.js`, `detect-secrets.js`, `bundle-size-check.js`, `new-migration.js`

### GitHub Actions (all 7 workflows + dependabot + CODEOWNERS)
- `ci.yml` — PR: lint/typecheck/test/clippy/rust-test/bundle-smoke/commitlint/forbidden-patterns
- `nightly.yml` — 03:00 UTC on develop: full gauntlet + criterion benches + mutation tests + signed nightly MSI
- `release-beta.yml` — tag `v*-beta.*` on develop ancestor: signed MSI + updater manifest
- `release-stable.yml` — tag `v*.*.*` on main ancestor: signed MSI + changelog + updater manifest + bundle-size baseline record
- `insider.yml` — workflow_dispatch: license-gated signed insider MSI
- `security.yml` — cargo audit/deny, pnpm audit, CodeQL, gitleaks
- `release-please.yml` — perpetual release PR on develop
- `dependabot.yml` — npm + cargo + github-actions weekly
- `.github/CODEOWNERS`

### Tauri v2 backend (`src-tauri/`)
- `Cargo.toml` — two bins (`chronimage-app`, `chronimage-cli`), lib crate, full plugin set, criterion/proptest/mockall dev deps
- `build.rs`
- `tauri.conf.json` — frameless Mica window, single-instance, updater wired to stable channel URL
- `capabilities/default.json` — Tauri v2 ACL
- `src/main.rs` — bootstrap with tracing, Mica apply on Windows, plugin wiring
- `src/lib.rs` — denies `unwrap/expect/panic/todo` at crate level
- `src/commands.rs` — `ping`, `app_version`, `current_channel` with tests
- `src/entitlements.rs` — `Feature` enum, `Entitlements` struct (all-true in v1), `ensure()` guard, tests
- `src/telemetry.rs` — no-op `event()` / `timing()` call sites for future opt-in analytics
- `src/error.rs` — `AppError` with typed serialization to `{code, message}`
- `src/util/paths.rs` — `app_data_dir`, `catalog_db_path`, `models_dir`
- `src/catalog/{mod.rs, models.rs}` — Photo, Source, SourceCopy, Import, Setting structs
- `src/bin/chronimage-cli.rs` — clap subcommands `doctor`, `migrate`, `import`, `ai audit` (stubs)
- `migrations/20260419000000_initial.sql` — `photos`, `sources`, `source_copies`, `imports`, `settings`

### React frontend (`src/`)
- `index.html` — fonts preload, `#root`, data-theme/accent attrs
- `src/main.tsx` — StrictMode boot, styles imports
- `src/app.tsx` — assembles Titlebar + Rail + Main placeholder + StatusBar, drives data-theme attrs on <html>
- `src/styles/tokens.css` — oklch theme tokens (dark/light/5 accents) + spacing/radius
- `src/styles/global.css` — full design styles.css copied as-is (424 lines; ux-porter can refine later)
- `src/chrome/{Titlebar, Rail, StatusBar}.tsx` — ported from design, TypeScript-typed
- `src/primitives/Icon.tsx` — partial icon set (expand in Phase 1)
- `src/state/ui.ts` — Zustand store for screen + tweaks (+ test)
- `src/tauri/invoke.ts` — typed wrappers for `ping`, `app_version`, `current_channel`
- `src/util/log.ts` — `debug/info/warn/error` wrappers (forbidden-patterns permits these)
- `tests/setup.ts` — `@tauri-apps/api/core` mock for Vitest
- `src/app.test.tsx`, `src/state/ui.test.ts` — smoke tests

## Open threads (must land before Phase 0 exits)

1. **Install + verify the tree compiles.** Run in a fresh bash in repo root:
   ```
   pnpm install
   pnpm typecheck
   pnpm test
   cargo check --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path src-tauri/Cargo.toml
   ```
   Expect a few friction items — resolve them before declaring Phase 0 green:
   - `pnpm install` may complain about OneDrive file locking on Windows; workaround: close VS Code file watchers or move repo off OneDrive.
   - `cargo check` may fail if Tauri v2 MSRV requires a newer rustc — bump `rust-toolchain.toml` if needed (not yet written).
   - `sqlx` `sqlite` feature will want `DATABASE_URL` at compile time if any `query!` macros get added later — use offline mode `.sqlx/` when that happens; Phase 0 only ships models structs, so not yet a problem.
   - `tauri.conf.json` has `pubkey: "REPLACE_WITH_TAURI_UPDATER_PUBKEY"` — for local dev this is fine; before any release workflow runs, generate with `pnpm tauri signer generate` and store the private key in GitHub Secrets as `TAURI_UPDATER_PRIVATE_KEY`.

2. **Tauri dev smoke**: `pnpm tauri dev` should show the frameless Chronimage shell (titlebar + rail + status + empty main). Verify rail item clicks swap the main placeholder label.

3. **Missing workflow helper script**: `scripts/publish-updater-manifest.js` is referenced by all release workflows but not yet written. Either write it (publishes signed JSON to Cloudflare R2/Pages) or replace with a simpler GitHub-Pages approach. Block beta/stable/nightly releases until this exists.

4. **Missing `scripts/record-size-baseline.js`** — referenced by `release-stable.yml`. Trivial helper to read MSI size and emit JSON. Write before first stable tag.

5. **Tauri config variants**: `release-beta.yml` / `release-stable.yml` / `insider.yml` reference `tauri.beta.conf.json`, `tauri.insider.conf.json`, `tauri.nightly.conf.json`. Only base `tauri.conf.json` exists. Create the 3 channel-specific configs (each overrides `plugins.updater.endpoints` + optional bundle identifier/icon/product-name tweaks).

6. **Rust toolchain pinning**: add `rust-toolchain.toml` at repo root pinning `channel = "1.83"` + components `["rustfmt", "clippy"]` so CI and local dev use the same version.

7. **Branch setup**: the user's current branch is `master` per gitStatus. Repo expects `main` + `develop`. Before first PR: create `main` and `develop` branches (push to remote), then set default branch to `develop` and protect `main`. Document the steps in `docs/release/branch-protection.md`.

8. **Icons**: `tauri.conf.json` references `icons/32x32.png`, `icons/icon.ico`, etc. Run `cargo tauri icon <source.png>` to generate once an app icon exists. Placeholder: skip icon generation and let tauri-build warn.

9. **First probe commit** — plant a deliberate `console.log` and a deliberate `unwrap()` in separate files, attempt to commit, confirm lefthook rejects both. This validates the "only clean code gets committed" rule end-to-end.

10. **Tag push validation** — push `v0.1.0-beta.0` to a feature branch, confirm `release-beta.yml` rejects it (wrong branch), move to `develop`, re-tag, confirm release actually produces MSI. Don't publish the GitHub Release.

## Next session start

Priority order (highest leverage first):

1. **Branches**: `git checkout -b develop && git push -u origin develop`; create `main` later. Set default branch.
2. **`pnpm install`** + fix any dep resolution issues. Commit `pnpm-lock.yaml`.
3. **`pnpm typecheck && pnpm test`** — fix anything the first run surfaces.
4. **`cargo check` + `cargo test`** in `src-tauri/` — fix MSRV/dep issues. Add `rust-toolchain.toml` pin.
5. **Write `rust-toolchain.toml`** (1 line).
6. **Write `scripts/publish-updater-manifest.js`** + `scripts/record-size-baseline.js` (both ~30 lines each).
7. **Write `tauri.beta.conf.json`, `tauri.nightly.conf.json`, `tauri.insider.conf.json`** — each a tiny overlay on the base config (different `plugins.updater.endpoints` URL).
8. **`pnpm tauri dev`** — confirm the shell renders.
9. **Probe commits** to prove lefthook + forbidden-patterns work.
10. **First `feat(ci): add Phase 0 scaffolding` commit** on `develop`. Then PR → CI green → squash-merge.
11. **Port rest of the design primitives** (Chip, Seg, Slider, Toggle, Placeholder) — spawn the `ux-porter` agent (`model: sonnet`).
12. **Port full icon set** — the design has ~40 icons; `src/primitives/Icon.tsx` currently has ~12. Spawn `ux-porter` to finish.

## Notes

- The repo directory is `halide/` for historical reasons (Halide was the design's placeholder name, trademarked by Lux Optics). The product name everywhere in code/configs/copy is **Chronimage**. Never check in "Halide" product strings.
- User preference: **Sonnet** for all subagent invocations. Every `.claude/agents/*.md` sets `model: sonnet` in frontmatter.
- User's dev dir is on OneDrive (`C:\Users\jayas\OneDrive\Documents\GitHub\halide\`). Watch out for sync temp files and occasional file-locking. `.gitignore` already excludes `*~` and `.~*` patterns.
- Target deps in `package.json` are 2026-current; `pnpm install` should resolve cleanly but exact Tauri v2 minor versions may need a bump.
- The design's `styles.css` was copied verbatim to `src/styles/global.css` (424 lines). It uses the same CSS class names the JSX expects, so ported screens work out of the box. A future pass (ux-porter agent) can refactor it into per-screen CSS modules if preferred.
- `src/app.tsx` currently renders a placeholder main panel. Full screens arrive in Phase 1. Phase 0 exit needs only the chrome shell to render.
- Everything gate-able (prompt edit, cloud backup, bulk cleanup, semantic face search, insider updates) uses `Entitlements` (Rust) + will use `useEntitlement` (TS, not yet written). In v1 every check returns `true`. Adding a paid tier later is a one-file change in `src-tauri/src/entitlements.rs`.
- `release-please.yml` expects versions to be managed in `package.json` + `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json`. Keep those three in sync when manually bumping (or let release-please do it).
- Phase 0 exit criteria: see `docs/prds/phase-0.md`. Eight items; most are "typecheck/clippy pass + MSI builds from tag + pre-commit rejects bad code". Do NOT flip the phase marker in `CLAUDE.md` to Phase 1 until all eight are green and a full dry-run of each release channel has produced a signed MSI.
