# Contributing to Chronimage

Thanks for wanting to help. Chronimage is small and focused; here's how
to work with the codebase without getting stuck.

## Code of conduct

Be kind, be specific. Technical disagreement is fine — personal attacks
are not. Maintainers reserve the right to close threads that drift.

## Before you open a PR

- Read [`CLAUDE.md`](CLAUDE.md) at the repo root — that's the agent
  onboarding doc but it doubles as the "what is this thing" brief for
  humans too.
- Skim the active PRD: `docs/prds/phase-N.md`. We ship phase-by-phase;
  Phase 5 is the release-hardening pass.
- Pick up an issue tagged `good-first-issue` or comment on any open
  issue before doing substantial work.

## Branch strategy

- `main` — release tags only; protected, signed commits, squash merges,
  no direct pushes.
- `develop` — integration branch. All PRs target here.
- `feature/<topic>` — your working branch off `develop`.
- `hotfix/<topic>` — urgent fixes off `main` (rare).

## Local setup

```bash
# Node 20.11+, pnpm 9+, Rust stable (check rust-toolchain.toml for the
# pinned version).
pnpm install
cargo fetch --manifest-path src-tauri/Cargo.toml

# Pull bundled AI models on Windows:
pwsh scripts/fetch-bundled-models.ps1
# Linux/macOS:
# bash scripts/fetch-bundled-models.sh

# Run dev:
pnpm tauri dev
```

## Running the gauntlet locally

Every PR must pass:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo deny --manifest-path src-tauri/Cargo.toml check
pnpm typecheck
pnpm exec biome check .
pnpm exec vitest run
```

A pre-commit hook (lefthook) runs the fast ones (fmt / biome / clippy /
typecheck) automatically on staged files. `cargo deny` and the full
test suite are not in the hook — please run them yourself before
pushing if you've touched dependencies or code that could regress
behaviour.

## Style

- **Rust**: no `unwrap()` / `expect()` / `panic!` outside `#[cfg(test)]`.
  Use `Result` + `?` + `thiserror`. Enforced by
  `scripts/forbidden-patterns.cjs`.
- **TypeScript**: no `any` in committed code. `unknown` + narrowing is
  fine. No `console.log` — use the `debug` helper.
- Follow the existing file. If a module already prefers a pattern,
  match it rather than introduce a competing one.

## Commit messages

Conventional Commits, enforced by commitlint. Valid scopes live in
`commitlint.config.cjs`. Subject line must be lowercase, ≤ 100 chars.

Good:

```
feat(map): cache osm tiles on disk
fix(import): handle arw files without exif thumbnail
docs(repo): clarify source-cleanup dashboard
```

Bad:

```
Updated stuff
WIP
Fixed bug #42
```

## PR checklist

- [ ] The gauntlet passes locally.
- [ ] Tests cover the new code (unit preferred; integration when the
      seam is a Tauri command).
- [ ] If you touched a schema, you added a forward-only migration and
      bumped `schema_version`.
- [ ] If you added a new external credential or env var, you updated
      `docs/manual-setup.md`.
- [ ] If you added a new feature, you updated the relevant PRD.

## What we don't want

- Silent disabling of forbidden-pattern checks / clippy lints without a
  `why` comment.
- Refactor-everything PRs (they're hard to review). Break them up.
- Dependencies that aren't audited by `cargo deny`. Open an issue
  before adding one.
- Code that calls the network without routing through a
  `user_initiated_*` function — the source-cleanup safety guarantees
  depend on that invariant.

## Security issues

Please don't open a public issue for security problems. See
[`SECURITY.md`](SECURITY.md) for disclosure process.

## License

By submitting a PR you agree that your contribution is licensed under
the same dual MIT-OR-Apache-2.0 terms as the rest of the codebase.
