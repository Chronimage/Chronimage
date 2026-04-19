---
description: Save a session handoff note to docs/checkpoints/ and update CLAUDE.md's phase marker if needed.
argument-hint: "[optional note]"
---

Write a session checkpoint so the next session can resume cleanly.

Do this now:

1. Run `git status --porcelain` and `git diff --stat` to understand what changed this session. Also run `git log --oneline -10` so you remember recent commits.

2. Compose a new checkpoint file at `docs/checkpoints/YYYY-MM-DD-HHMM.md` (use today's UTC date/time) with this structure:

```markdown
# Checkpoint {YYYY-MM-DD HH:MM UTC} · {current phase from CLAUDE.md}

## Just finished
- {bullet per meaningful change this session, tied to files}

## Open threads
- {bullets: anything started but not complete, blockers, TODOs}

## Next session start
1. {first concrete action, ideally with the exact command}
2. {second if obvious}

## Notes
- {anything non-obvious: hardware observations, test flakiness, model perf numbers, decisions}
```

3. Overwrite `docs/checkpoints/latest.md` with the same content (it's the always-current pointer read by SessionStart hook and CLAUDE.md resume protocol).

4. If the current phase's exit criteria have ALL landed, propose flipping the "Current phase:" line in `CLAUDE.md` to the next phase. Do this only with explicit confirmation in the checkpoint note — do not auto-advance.

5. Echo back a 3-line summary: phase, files touched, next action.

If the user passed an argument `$ARGUMENTS`, include it as a final "User note:" section in the checkpoint.
