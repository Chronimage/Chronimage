---
description: Open, read, or create a phase PRD at docs/prds/phase-N.md.
argument-hint: <phase-N e.g. phase-1, phase-2>
---

Operate on `docs/prds/$ARGUMENTS.md`.

If the file exists:
- Read and display its contents.
- Summarize in 3 bullets what the phase delivers and how close it is to done (by scanning checkbox / "✓" / "DONE" markers in the exit criteria section).

If the file does not exist:
- Create it from this template, filling in the phase number. For phases already sketched in the master plan (`.claude/plans/fetch-this-design-file-virtual-donut.md`), copy the relevant sections instead of guessing.

```markdown
# $ARGUMENTS · {phase-title}

> {one-line pitch for this phase}

## Context
{why this phase · what prior phase assumed · what this enables next}

## Personas & stories
- **Jay (hobbyist, ~200k photos, Sony A7 IV, no GPU)**
  - As Jay, I can {...}, so that {...}
- **Priya (event photographer, ~250k photos, 3070 GPU)**
  - As Priya, I can {...}, so that {...}

## Must-have deliverables
- [ ] ...

## Non-goals
- ...

## Non-functional requirements
- Throughput / latency / precision numbers — these become the test-file assertions.

## Schema changes (if any)
\`\`\`sql
-- migration: src-tauri/migrations/YYYYMMDDHHMMSS_<name>.sql
\`\`\`

## API surface
Tauri commands this phase adds:
- `#[tauri::command] async fn …`

Events emitted:
- `chronimage.import.progress`

## Entitlements (gate-ready even though free in v1)
- Features to wrap in `ensure_entitlement(Feature::…)`: ...

## Exit criteria
- [ ] ... (bound to `tests/…` file whenever testable)

## Open questions
- ...

## TODO log (living)
- [ ] ...
```

Then write it to `docs/prds/$ARGUMENTS.md` and print a 3-line summary.
