---
description: Bundle the full working context (CLAUDE.md + latest checkpoint + active PRD + memory + git state + open threads) into a single portable markdown file that another Claude Code session can load.
argument-hint: "[optional-label]"
---

Produce a self-contained "context bundle" file at `docs/context-bundles/YYYY-MM-DD-HHMM[-label].md` that another Claude Code session — on a different machine, a different agent, or a fresh chat — can read to pick up the project state without any prior conversation.

## What the bundle must contain

Assemble in this exact order so `/context-load` can parse it predictably. Every section is required; emit empty placeholders if there's nothing to say.

```markdown
# Chronimage context bundle · {YYYY-MM-DD HH:MM UTC}[ · {$ARGUMENTS}]

> Self-contained handoff. Another Claude Code session can `/context-load` this file and resume work with no prior conversation context. Everything below is a snapshot at the dump time — always re-check the live repo state before acting.

## 1. Current phase marker
{verbatim copy of the "Current phase:" line from CLAUDE.md}

## 2. Working directory & branch
- path: {absolute repo path}
- branch: {git rev-parse --abbrev-ref HEAD}
- upstream: {git rev-parse --abbrev-ref --symbolic-full-name @{u} or "(none)"}
- head: {git log -1 --format="%h %s (%an, %ar)"}
- working tree: {count of dirty files} uncommitted changes

## 3. Resume protocol (verbatim from CLAUDE.md)
{copy the "How to resume a session" section from CLAUDE.md}

## 4. Tech stack (verbatim from CLAUDE.md)
{copy the "Tech stack" section}

## 5. Rules (verbatim from CLAUDE.md)
{copy the "Rules" section}

## 6. Active PRD summary
Path: `docs/prds/phase-{N}.md`
Goal (1 line): {extract the blockquote below the title}
Open deliverables: {count of unchecked [ ] items in "Must-have deliverables"}
Exit criteria done/total: {checked}/{total}
Full PRD contents inlined below for portability:

\`\`\`markdown
{paste the entire active PRD verbatim}
\`\`\`

## 7. Latest checkpoint
Path: `docs/checkpoints/latest.md`
{paste the entire latest checkpoint verbatim}

## 8. Memory index (for cross-project continuity)
{paste contents of ~/.claude/projects/<project-slug>/memory/MEMORY.md if it exists}
{for each memory file listed in MEMORY.md, include its frontmatter + body inline so the bundle stands alone}

## 9. Recent git history (last 20 commits)
\`\`\`
{output of `git log --oneline -20`}
\`\`\`

## 10. Uncommitted changes (diff stat)
\`\`\`
{output of `git status --short` followed by `git diff --stat`}
\`\`\`

## 11. Open TODOs in tracked source
{count + list of `TODO(cc)` and `TODO(blocker)` occurrences from `git grep -n "TODO(cc)\|TODO(blocker)" -- "*.ts" "*.tsx" "*.rs"`; cap at 50 entries}

## 12. Verification commands
These are the exact commands the next session should run to confirm baseline is green before starting work:

\`\`\`bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
pnpm exec biome check .
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
\`\`\`

## 13. Next concrete action
{copy the "Next session start" section from docs/checkpoints/latest.md}

## 14. Notes
{anything else worth carrying over — host-specific quirks, credential status, WIP flags, branch-protection status}
```

## Steps

1. Compute the target filename. If `$ARGUMENTS` is given, slugify it (lowercase, hyphens for spaces, strip non-alphanumeric) and append after the timestamp. Example: `docs/context-bundles/2026-04-19-1830-phase1-kickoff.md`.
2. Create `docs/context-bundles/` if it doesn't exist.
3. Read the source files (CLAUDE.md, the active PRD, the latest checkpoint, the memory index) and assemble the bundle in the exact template above.
4. Run the git commands listed inline and paste their outputs into the right sections.
5. Write the bundle file.
6. Print a three-line summary:
   - bundle path (absolute)
   - byte size
   - "Share this file with the receiving session and run `/context-load <path>`."

## Don'ts

- **Don't include credentials or secrets** — if you find anything that looks like an API key or private key, redact it as `[REDACTED secret]` and flag it in Notes.
- **Don't include binary content** — this is a markdown bundle.
- **Don't include the contents of `node_modules/`, `src-tauri/target/`, or `models/`** — the receiving session can re-obtain them from the repo.
- **Don't truncate the PRD or checkpoint** — they're load-bearing. If they're very long, split the bundle into multiple files (`-pt1.md`, `-pt2.md`) and cross-link, but don't omit content.
