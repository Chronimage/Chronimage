---
description: End-to-end PR pipeline — run local CI gates, commit, push, open PR, watch CI, merge on green. Uses GitHub token from `.env.local`.
argument-hint: "[optional commit subject]"
---

Ship the current uncommitted work as a merged PR against `develop`. Do **all** of the following in order; abort with a clear summary if any step fails.

## 0. Pre-flight (read state, don't mutate)

Run these checks; bail with a one-line summary if any precondition fails:

- **Working tree must have changes.** `git status --porcelain` must be non-empty. If clean, say "nothing to ship" and stop.
- **Token must be present.** `test -f .env.local && grep -E '^(GH_TOKEN|GITHUB_TOKEN)=' .env.local` must return a row. If missing, say so and stop — don't fall back to interactive `gh auth login`.
- **Note the starting branch.** If on `main` or `develop`, you'll create a feature branch in step 3. If already on a `feature/...` branch with an open PR, ask before overwriting.

## 1. Run local CI gates in parallel

CI minutes on Windows Rust + Tauri are expensive — get it right the first push. Run all of these in **parallel** (single message, multiple Bash tool calls). Bail if any fail.

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm typecheck
pnpm exec vitest run --reporter=basic
pnpm test:cov     # coverage gate — same one CI runs as `Unit tests (vitest)`
```

If any fail, print the failing tool's tail-of-output and stop. Don't try to "auto-fix" — surface the failure and let the user decide.

## 2. Decide the commit subject

If `$ARGUMENTS` is set, use it verbatim **after** validating commitlint rules below.
Otherwise, infer one from `git diff --stat` and the most-changed file paths. The subject must satisfy commitlint:

- **Conventional Commits.** Format `type(scope): subject`. Pick `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore` based on the diff.
- **Subject must be lowercase or sentence-case** — commitlint rejects camelCase identifiers and uppercase acronyms in the subject. Examples that **fail**: `feat(catalog): rename sourceDelete store` (camelCase D), `feat(ai): heic and HEIF retry` (uppercase HEIF). Reword: `feat(catalog): rename source-delete store`, `feat(ai): heic retry on timeout`.
- **Length ≤ 72 chars.**

If you're not sure the subject lints, run `echo "<subject>" | npx commitlint` first.

## 3. Branch + commit + push

- If currently on `develop`/`main`, create a feature branch named after the dominant theme: `git checkout -b feature/<kebab-case-slug>` (≤ 50 chars). Slug from the commit subject.
- **Stage explicitly.** Never `git add -A` or `git add .`. List the changed files (from `git status --porcelain`) and add them by name. Always exclude:
  - `docs/context-bundles/*` (session artifacts)
  - `.env*` (credentials)
  - `models/`, `src-tauri/target/`, `dist/`, `.vite/`, `tests/fixtures/photos/*` (binary / large artifacts)
- Compose the commit body. Lead with bullet points covering each major change in the diff. Add a blank line, then `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- Use a HEREDOC to pass the message: `git commit -m "$(cat <<'EOF' ... EOF)"`. If lefthook's `commit-msg` rejects it, fix the subject (lowercase / sentence-case) and re-commit. **Never `--no-verify`.**
- Push: `set -a; source .env.local; set +a; git push -u origin <branch>`.

## 4. Open the PR

```bash
set -a; source .env.local; set +a
gh pr create --base develop --title "<commit subject>" --body "$(cat <<'EOF'
## Summary

<2-4 sentence prose describing the *why* — not the file list. Lead with the user-visible change.>

<Optional: subsections for distinct themes if the PR bundles them.>

## Test plan
- [x] `cargo fmt --check` clean
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] `cargo test --lib` — N passed
- [x] `pnpm typecheck` clean
- [x] `pnpm exec vitest run` — N passed
- [x] `pnpm test:cov` — coverage thresholds met
- [ ] <manual verification step the human should do, if any>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture the PR URL from the output. Tell the user the PR number + URL.

## 5. Watch CI

Run `gh pr checks <PR#> --watch --interval 30` in the **background** so the conversation isn't blocked. Schedule a wakeup ~10 minutes out (Windows Rust + Tauri smoke commonly takes 8–16 min):

```
ScheduleWakeup(delaySeconds=600, reason="Windows Rust + Tauri smoke ~10–16m; checking back at 10m for first signal", prompt="<the same /ship-pr invocation>")
```

When you wake up, run `gh pr checks <PR#>` once (no `--watch`) and inspect the table.

## 6. Handle CI outcomes

**All green:** proceed to step 7.

**Failed checks:** pull the failing log:
```bash
set -a; source .env.local; set +a
gh run view <run-id> --log-failed | tail -120
```
Common failure modes and the right response:
- **Coverage threshold under 70%** for `src/state/**` or `src/tauri/**` — a new module wasn't tested. Add a test file mirroring the existing `state/import.test.ts` pattern, commit with `test(<scope>): cover ...`, push. The watch loop should pick up the new run.
- **commitlint** — subject case issue. Amend? No — make a new commit; `commit --amend` is forbidden by repo policy.
- **Rust clippy on Windows-only paths** — local clippy ran on the host platform, but the CI runner may surface `cfg(windows)`-gated lint errors. Read the failure, fix, push.
- **Tauri smoke build** — usually a real bug or a missing `tauri.conf.json` field. Surface the error.
Re-run the watch loop after pushing the fix.

**Still queued / running after wakeup:** schedule another wakeup with a longer delay (1200s — past the 5-min cache window since this is an idle wait) and continue.

## 7. Merge

Squash-merge and delete the branch:
```bash
set -a; source .env.local; set +a
gh pr merge <PR#> --squash --delete-branch
```

Verify and sync:
```bash
gh pr view <PR#> --json state,mergedAt,mergeCommit -q '"state=\(.state) mergedAt=\(.mergedAt) commit=\(.mergeCommit.oid[:7])"'
git checkout develop
git fetch origin develop
git merge --ff-only origin/develop   # never `git reset --hard` — repo hook blocks it
```

## 8. Report

End with a tight summary:
- PR # and URL
- Squash commit short SHA on `develop`
- Number of CI runs (1 if first-push green; 2+ if a fix was needed)
- Anything left for the user to verify manually (from the test plan's unchecked items)

## Things to never do
- Push to `main` or `develop` directly.
- Force-push.
- `--no-verify` on commits.
- `git reset --hard`, `git push --force`, `git checkout .` (repo hook blocks these anyway, but don't try).
- `git add -A` / `git add .` — always list files.
- Print the GitHub token to stdout. Read it via `set -a; source .env.local; set +a` and let `gh` / `git` pick it up via `GH_TOKEN` / `GITHUB_TOKEN` from the environment.
- Sleep-poll the CI status — use `ScheduleWakeup` for waits ≥ 5 minutes, background `--watch` otherwise.
