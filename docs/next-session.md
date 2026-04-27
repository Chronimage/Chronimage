# Next Session — Phase 6 Reset Cleanup

_Refreshed 2026-04-27_

Start from `docs/checkpoints/latest.md` and `docs/prds/phase-6.md`. The repo is in a reset-OK cleanup state: local dev catalogs may need to be recreated because migration history was intentionally normalized.

## First Move

Run the cleanup verification gauntlet:

```bash
pnpm typecheck
pnpm exec biome check .
pnpm exec vitest run
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Then validate a fresh catalog:

```bash
pnpm migrate:run
pnpm tauri dev
```

## Watch Points

- Existing `catalog.db` files can fail after the reset because migration checksums changed. Recreate the dev catalog rather than adding compatibility repair migrations.
- Runtime logs are file/stdout based. Do not reintroduce Docker, compose, Loki, or Grafana assumptions for local debugging.
- The active schema uses `photos.rating`; do not add `star_rating` shims.
- Use sqlx migration metadata for schema truth; do not revive `settings.schema_version`.
