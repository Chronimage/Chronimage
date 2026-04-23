# Phase 0 · Foundation & quality backbone

> Make the repository walkable, the chrome shell runnable, and every quality gate + release channel operational — before writing a single line of feature code.

## Context

Chronimage is starting from an empty repo. Before shipping features, every later phase must inherit: type safety, test-first discipline, automated releases across four channels (stable / beta / nightly / insider), and a session-resumable development loop via Claude Code scaffolding. Every quality gate missed in Phase 0 becomes technical debt that compounds across phases 1–5.

## Personas & stories

- **Jay (solo builder / user)**
  - As Jay, I can `pnpm tauri dev` on a fresh clone and see the Chronimage shell within 2 minutes, so that I know the repo is working.
  - As Jay, I can push a tag and have CI produce a signed MSI on the right release channel, so that releases don't require local build rituals.
  - As Jay, I can open a new Claude Code session and have it read CLAUDE.md + latest checkpoint + active PRD automatically, so that I never re-explain project state.

- **Claude Code agent (itself)**
  - As an agent, I can use `/phase-checkpoint` at session end and `/verify-phase` to confirm exit criteria, so that I don't declare a phase done prematurely.

## Must-have deliverables

Repository scaffold:
- [ ] `CLAUDE.md` with agent onboarding, phase marker, resume protocol ✅ (created)
- [ ] `.claude/{settings.json, agents/, commands/, hooks/}` — 7 commands, 6 agents, 5 hook scripts ✅
- [ ] `.gitignore`, `.gitattributes` (Git LFS config for photo fixtures) ✅
- [ ] `README.md`, `LICENSE` (TBD) ✅

Config:
- [ ] `package.json`, `tsconfig.json`, `vite.config.ts`, `tailwind.config.ts`, `biome.json`, `vitest.config.ts`, `playwright.config.ts` ✅
- [ ] `lefthook.yml`, `commitlint.config.js`, `cliff.toml`, `cargo-deny.toml` ✅
- [ ] `scripts/` — forbidden-patterns, detect-secrets, bundle-size-check, new-migration ✅

Tauri backend (pending):
- [ ] `src-tauri/Cargo.toml` — workspace with `chronimage-app` + `chronimage-cli` binaries
- [ ] `src-tauri/tauri.conf.json` — frameless window, Mica on Win11, single-instance plugin, updater plugin wired to 4 channels
- [ ] `src-tauri/src/main.rs` — Tauri bootstrap, one sample `#[tauri::command] fn ping() -> &'static str`
- [ ] `src-tauri/src/lib.rs` — re-exports for testability
- [ ] `src-tauri/src/catalog/mod.rs` + `models.rs` — SQLite connection pool + photo/source/imports/source_copies/settings model stubs
- [ ] `src-tauri/src/entitlements.rs` — all-true Entitlements returned in v1
- [ ] `src-tauri/src/telemetry.rs` — no-op event() stub
- [ ] `src-tauri/src/bin/chronimage-cli.rs` — clap-driven CLI with `migrate`, `import`, `ai audit`, `doctor` subcommands (all no-op stubs in Phase 0)
- [ ] `src-tauri/migrations/20260419_000000_initial.sql` — `photos`, `sources`, `imports`, `source_copies`, `settings`

React frontend (pending):
- [ ] `index.html`, `src/main.tsx`, `src/app.tsx`
- [ ] `src/styles/tokens.css` — oklch vars, fonts, accent palette
- [ ] `src/styles/global.css` — sidepanel, toolbar, canvas, chip, etc.
- [ ] `src/chrome/{Titlebar,Rail,StatusBar}.tsx` — frameless chrome, typed
- [ ] `src/primitives/{Chip,Seg,Slider,Toggle,Icon,Placeholder}.tsx`
- [ ] `src/state/{ui,fixtures}.ts` — Zustand screen store + stub photo/album data
- [ ] `src/tauri/invoke.ts` — typed wrappers around `@tauri-apps/api/invoke`
- [ ] `src/util/log.ts` — `debug()` / `warn()` / `error()` wrappers
- [ ] `src/routes/__root.tsx`, `src/routes/index.tsx` — TanStack Router skeleton with 6 stub routes (onboard, catalog, cull, cullbin, develop, settings)

Docs:
- [ ] `docs/prds/phase-0.md` (this file) ✅ (being written)
- [ ] `docs/prds/phase-1.md` — full Phase 1 PRD (pending)
- [ ] `docs/checkpoints/latest.md` — seed pointing at Phase 0 state (pending)
- [ ] `docs/adr/` — empty, ready for records
- [ ] `docs/release/bundle-size-baseline.json` — records first MSI size

CI/CD (pending):
- [ ] `.github/workflows/ci.yml` — PR: typecheck, lint, test, clippy, rust test, bundle build, coverage upload
- [ ] `.github/workflows/nightly.yml` — cron: full suite + mutation tests + perf + signed nightly MSI
- [ ] `.github/workflows/release-beta.yml` — tag `v*-beta.*` → signed beta MSI + updater manifest
- [ ] `.github/workflows/release-stable.yml` — tag `v*.*.*` → signed stable MSI + updater manifest + changelog
- [ ] `.github/workflows/insider.yml` — manual dispatch → signed insider MSI (license-gated)
- [ ] `.github/workflows/security.yml` — weekly + PR: cargo audit, cargo deny, pnpm audit, CodeQL
- [ ] `.github/workflows/release-please.yml` — maintains perpetual release PR on develop
- [ ] `.github/dependabot.yml` — npm + cargo weekly
- [ ] Repo branch protection rules documented in `docs/release/branch-protection.md` (to be configured on GitHub side)

Model/fixture plumbing:
- [ ] `models/.gitkeep` ✅
- [ ] `tests/fixtures/README.md` — LFS setup instructions + list of expected fixtures
- [ ] `tests/setup.ts` — vitest setup (mocks for Tauri invoke)

## Non-goals

- Any actual feature code (import, tag, search, dedupe, etc. — all Phase 1+).
- Sound design, marketing site, landing page.
- macOS / Linux ports.
- AI model downloads.
- Code-signing cert acquisition (stub with self-signed for nightly; EV cert acquisition is a Phase 5 task).

## Non-functional requirements

- `pnpm tauri dev` → visible shell in ≤ 120 s cold, ≤ 4 s after first build
- `pnpm typecheck` passes on `src/` + `tests/`
- `cargo clippy -- -D warnings` passes
- `pnpm test` runs and exits 0 with at least 1 passing test per layer (primitive, hook, screen, invoke)
- A deliberately planted `console.log` or `unwrap()` in staging is rejected by pre-commit
- `git tag v0.1.0-beta.0` + push triggers CI → produces a working MSI on release-beta.yml artifacts within 15 min

## Schema changes

See `src-tauri/migrations/20260419_000000_initial.sql` (pending). Initial tables:

```sql
CREATE TABLE photos (...);
CREATE TABLE sources (...);
CREATE TABLE source_copies (...);
CREATE TABLE imports (...);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

Exact DDL deferred to catalog-architect agent invocation during implementation.

## API surface

- `#[tauri::command] fn ping() -> &'static str` — smoke
- `#[tauri::command] async fn app_version() -> String` — for statusline/about
- `#[tauri::command] async fn current_channel() -> UpdaterChannel` — enum: stable/beta/nightly/insider

## Entitlements

- `Entitlements::current()` returns all `true`
- Feature enum registered but unused (gates land in Phase 1+)

## Exit criteria

- [ ] `pnpm tauri dev` opens a 1440×900 frameless window showing the dark shell (titlebar + rail + status)
- [ ] Clicking the 5 rail items + settings gear switches the main panel placeholder
- [ ] `pnpm typecheck` + `pnpm lint` + `pnpm test` + `cargo clippy -D warnings` + `cargo test` all pass
- [ ] `/phase-checkpoint` produces a valid markdown file at `docs/checkpoints/<ts>.md` + updates `latest.md`
- [ ] Dummy commit on `feature/0-smoke` → opens PR → CI green → merge to `develop` → nightly cron runs → nightly MSI artifact downloadable
- [ ] Push tag `v0.1.0-beta.0` → release-beta.yml produces signed MSI + updater manifest deployed to the beta channel URL
- [ ] Push tag `v0.1.0` on `main` → release-stable.yml produces signed MSI + updater manifest deployed to stable
- [ ] Pre-commit rejects a probe commit containing `console.log` in a TS file and `unwrap()` in a Rust file
- [ ] Bundle-size baseline recorded at `docs/release/bundle-size-baseline.json`

## Open questions

- **Windows code-signing cert** for v0.1.0 stable — use a self-signed cert until EV acquired, or delay v0.1.0 stable until cert exists? Lean toward self-signed + warn users; EV comes in Phase 5.
- **Updater manifest hosting** — Cloudflare Pages (separate repo) vs. GitHub Pages (same repo, gh-pages branch). Lean CF Pages for cache control. Decide before first beta release.
- **Insider-channel license format** — Ed25519-signed JWT vs. custom signed JSON. Decide before wiring insider.yml.

## TODO log

- [x] Scaffolding directories created
- [x] CLAUDE.md + .claude/ + scripts + configs
- [ ] Write initial SQL migration
- [ ] Write src-tauri scaffold
- [ ] Port chrome shell + primitives from design
- [ ] Write all 7 GitHub Actions workflows
- [ ] First end-to-end tag push on a test repo to validate pipeline
