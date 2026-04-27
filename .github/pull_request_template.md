<!--
Thanks for the PR. Fill out the sections below — one-line answers are
fine when that's all there is to say.
-->

## Summary

<!-- 1-3 bullets: what changed and why. -->

## Test plan

<!-- Checklist for how you verified this works. -->

- [ ] `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo test --lib` green
- [ ] `pnpm typecheck` + `pnpm exec biome check .` clean
- [ ] `pnpm exec vitest run` green
- [ ] `cargo deny check` clean (if you touched deps)
- [ ] Manual walkthrough of the happy path (describe below)
- [ ] Manual walkthrough of one edge case (describe below)

## Migration / secret notes

<!--
- If you added a schema change: migration file updated; sqlx migration metadata remains the source of truth.
- If you added a new credential / env var: updated docs/manual-setup.md.
- If you added a command that hits the network: ensured it's behind a
  user_initiated_* gate.
-->

## Screenshots / recordings

<!-- Only if you touched UI. -->

## Related

<!-- Linked issues, PRs, or PRD sections. e.g. "Closes #42 · Phase 5 §7". -->
