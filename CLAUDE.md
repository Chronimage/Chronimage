# Chronimage — Agent onboarding

> **Resume protocol:** Read this file → read `docs/checkpoints/latest.md` → read the active PRD in `docs/prds/phase-N.md`. Do this on *every* session start before touching code.

**Current phase:** Phase 1 — Deep AI Catalog (week 1 · scaffolding landed, feature implementation next)

---

## What Chronimage is

A Windows-first, on-device AI photo organizer for hobbyist photographers with scattered libraries (Google Photos + iCloud + iPhone + local disks + Sony A7 IV ARW files). The product solves four things in order:

1. **Consolidate** fragmented sources into one owned catalog
2. **Free up cloud storage** by safely deleting from source once local copies are verified
3. **Resurface** photos for rediscovery (on this day, unseen in 2y, unflagged favorites)
4. **Cull + light develop** the backlog so it becomes a resource

Between Darktable (free but too complex) and Lightroom (too expensive, cloud lock-in). Sits closest to Excire Foto in market terms, but adds cull + light editor + source-side cleanup.

**Non-goals (v1):** not a pro Lightroom replacement; not cloud-first; not mobile; not subscription-only.

---

## Tech stack (locked; don't re-derive)

- **Shell:** Tauri v2 (Rust host + WebView2)
- **Frontend:** React 19 + TypeScript + Vite + Tailwind v4 + Radix UI + Zustand + TanStack Router + TanStack Query + @tanstack/react-virtual
- **Backend:** Rust · tokio · rayon · sqlx (SQLite + sqlite-vec) · rawler · libheif-rs · image-rs · wgpu · ort (ONNX Runtime)
- **AI:** SigLIP-B (embeddings, CPU-fast), ArcFace+RetinaFace via FaceONNX (faces), gemma4-9b via llama.cpp sidecar (captions, GPU-opt), HDBSCAN (face clustering), pHash + SigLIP cosine (dedupe)
- **Testing:** Vitest + RTL + Playwright + tauri-driver + axe-core + criterion + proptest + cargo-fuzz + cargo-mutants
- **Quality:** lefthook (pre-commit) + biome + cargo fmt/clippy + commitlint + cargo-deny + cargo-audit
- **Release:** 4 channels (stable / beta / nightly / insider) via GitHub Actions; signed MSI via Tauri v2 + EV cert; auto-update via `tauri-plugin-updater`

Full rationale: `docs/prds/phase-0.md` § Tech stack. Plan document: `.claude/plans/fetch-this-design-file-virtual-donut.md` (reference only; repo-local PRDs are authoritative going forward).

---

## Repository map

```
.claude/        agents, commands, hooks, settings for Claude Code
.github/        CI/CD workflows (ci, nightly, release-*, security, release-please)
design-handoff/ READ-ONLY reference — original design bundle from claude.ai/design
docs/
  prds/         one PRD per phase (phase-0.md … phase-5.md) — authoritative
  adr/          architecture decision records
  checkpoints/  session handoff notes; latest.md is always current
src/            React frontend (TypeScript)
src-tauri/      Rust backend + Tauri shell
tests/          fixtures (LFS), E2E (Playwright), visual goldens
scripts/        local tooling (forbidden-patterns, bundle-size-check, etc.)
models/         gitignored — downloaded on first run
```

---

## Rules (enforced by hooks + CI)

### Rust
- **No `unwrap()` / `expect()` / `panic!()`** outside `#[cfg(test)]`. Use `Result` + `?` + `thiserror` error types.
- **No `dbg!()`** in committed code.
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` must be clean.
- Tests live next to modules (`mod tests { … }`) for unit, `src-tauri/tests/` for integration.
- Every `#[tauri::command]` gets ≥ 2 tests (happy + edge).

### TypeScript
- **No `any`** in committed code. `unknown` + type narrowing is fine.
- **No `console.log`** (use `debug` from `src/util/log.ts`).
- `pnpm typecheck` must pass.
- Biome handles fmt + lint on save.

### Commits
- **Conventional Commits** enforced by commitlint: `feat(catalog): …`, `fix(import): …`, `perf(raw): …`, etc.
- Signed commits required on `main` and `develop`.
- No commits to `main` or `develop` directly — PRs only.
- Branches: `feature/xyz` off `develop`, `hotfix/xyz` off `main`.

### CI minutes are costly — get it right the first push
GitHub Actions minutes are metered; a failed CI run that burns 10+ minutes on Windows Rust builds is a real cost. **Before every `git push`, run the same gates CI runs locally** and only push when they're all green: make sure to have meaningful amount of work before pushing to remote.

```bash
# Rust
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test   --manifest-path src-tauri/Cargo.toml --lib
cargo deny   --manifest-path src-tauri/Cargo.toml check   # license / advisory gate

# Frontend
pnpm typecheck
pnpm exec biome check .
pnpm exec vitest run
```

Don't rely on the pre-commit / pre-push hook alone — it skips `cargo deny` and sometimes skips `cargo test`. A 5-minute local check saves 15+ minutes of CI pain and avoids the "push, fail, fix, push, fail" cycle. If a push does fail CI, investigate the root cause (read the failing log, not just the summary) before re-pushing.

### Security / privacy
- **No network calls** from Rust without an explicit user-triggered flow (import from cloud source, auto-update check, opt-in telemetry). Searchable enforcement: reqwest/ureq usage must be gated behind a function whose name contains `user_initiated_`.
- **No telemetry** in v1. Call sites exist (`telemetry::event(…)`) but no-op.
- **Source-side deletion** is always two-step + SHA256-gated + ≥2× free-space-gated. See `src-tauri/src/commands/source_cleanup.rs` doc comments.

### Paths
- Never commit `models/`, `catalog.db`, `tests/fixtures/photos/*.arw`-`*.heic` (LFS-only), `src-tauri/target/`, `dist/`, `.vite/`, OneDrive temp files, or anything in `tmp/`.

---

## Design handoff (read-only)

The design bundle at `design-handoff/chronimage/` is the source of truth for visual and interaction specs. **Do not edit files there.** Port JSX → TSX into `src/screens/` using the `/port-screen` command, preserving visual parity against `design-handoff/chronimage/project/Chronimage.html` and `styles.css`.

Layout tokens are in `src/styles/tokens.css` (ported from the design's `styles.css`). If a new token is needed, add it here first, not inline.

---

## Build commands

```bash
# Install
pnpm install
cargo fetch --manifest-path src-tauri/Cargo.toml

# Dev
pnpm tauri dev

# Typecheck + lint (pre-commit runs these automatically)
pnpm typecheck
pnpm exec biome check .
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# Tests
pnpm exec vitest run
pnpm exec playwright test
cargo test --manifest-path src-tauri/Cargo.toml

# Migrations
pnpm migrate:new <name>   # generate a new SQL migration
pnpm migrate:run          # apply pending migrations to dev DB

# Release (don't do this manually — push a tag)
# git tag v0.1.0-beta.1 && git push --tags
```

---

## How to resume a session (concretely)

1. Read this file (you just did).
2. `cat docs/checkpoints/latest.md` — contains: what was just done, what's open, exact next action.
3. Open the active PRD: `docs/prds/phase-0.md` for Phase 0, `phase-1.md` once Phase 0 exits.
4. Check `git status` + `git log -5` for uncommitted / recent work.
5. Run `pnpm typecheck && cargo check --manifest-path src-tauri/Cargo.toml` to confirm the baseline is green before starting.

At session end: run `/phase-checkpoint` to update `docs/checkpoints/latest.md` + flip the phase marker at the top of this file if appropriate.

## Cross-session / cross-machine handoff

Claude Code sessions don't share memory. To move work between them:

- **`/context-dump [label]`** — writes a self-contained bundle at `docs/context-bundles/YYYY-MM-DD-HHMM[-label].md` containing CLAUDE.md, the active PRD, the latest checkpoint, memory, git state, open TODOs, and the exact next action. Portable; one file.
- **`/context-load <path>`** — reads a bundle in a fresh session, cross-checks against the live repo, prints a mismatch report, and runs the verification gauntlet. Does not auto-mutate files.

Use `/context-dump` whenever you're about to pause work and expect to resume in a different Claude Code session (different machine, different browser tab, fresh chat after compaction).

---

## Subagents you should default to

**Model:** use **`sonnet`** for subagents by default (pass `model: "sonnet"` in the Agent tool call — user preference). Escalate to `opus` only for architectural trade-off analysis or cross-cutting bug investigation; flag the escalation to the user first.

- **Broad codebase exploration or "where does X live"** → spawn `Explore` (or the `general-purpose` agent if multi-step). Don't burn the main context on sequential greps.
- **Schema changes** → spawn `catalog-architect` (`.claude/agents/catalog-architect.md`).
- **Porting a design screen to TSX** → spawn `ux-porter`.
- **ONNX model choice or llama.cpp sidecar tuning** → spawn `ai-wrangler`.
- **RAW pipeline / wgpu shaders** → spawn `raw-pipeline-expert`.
- **Perf regression investigation** → spawn `perf-cop`.
- **Release-time packaging/signing** → spawn `release-captain`.

Full agent definitions: `.claude/agents/*.md`.

---

## Open cross-cutting decisions (not blocking)

- Model distribution: bundle SigLIP in installer vs. download on first run — currently leaning first-run download.
- Business model: deliberately unscoped. Features use `useEntitlement()` / `ensure_entitlement()` gates that always return `true` in v1. See `src-tauri/src/entitlements.rs`.
- iCloud auth: rely on iCloud-for-Windows sync folder in v1; revisit `pyicloud` later if API access is stable.

See `docs/prds/phase-1.md` § Open questions for the full list.
