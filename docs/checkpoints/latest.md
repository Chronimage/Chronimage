# Checkpoint 2026-04-27 · Phase 6 — reset cleanup in progress

Branch: `develop` · working tree intentionally dirty for whole-repo reset cleanup.

## Summary

The repo is being cleaned as a pre-release reset. Existing local `catalog.db` files are disposable for this cleanup because migration history has been squashed and normalized.

Completed in the current pass:

- Repair/no-op migrations were deleted, FTS trigger fixes were folded into the base catalog migration, and the Lightroom parity follow-up migration was merged back into the foundation migration.
- `settings.schema_version` breadcrumbs were removed from live migrations; sqlx migration metadata is now the source of truth.
- `photos.star_rating` was collapsed into the canonical `photos.rating` column and Rust/TS call sites were updated.
- Loki/Docker-era logging has been replaced with local file/stdout guidance. Frontend logs route through the file-backed logger.
- The top-level Cull Bin route, entitlement scaffold, unused placeholder/toggle primitives, and orphan frontend debug/merge/tether wrappers were removed.
- QueryClient ownership is consolidated at the React root, and thumbnail cache generation now shares one backend writer using `thumbnail_cache_path`.
- Catalog/Cull/Develop UI no longer advertises stale phase-gated actions that do nothing.

## Current Product Shape

- Active PRD: `docs/prds/phase-6.md`.
- Launch work is about EV signing, updater/release hosting, Insider signup/licensing, Cloudflare Pages/R2, Sentry wiring, and Microsoft Store packaging.
- Runtime logs are local rolling files from the Tauri host plus stdout for debug tooling. No Docker, compose, Loki, or Grafana dev stack is current.

## Verification Required

Because migrations were intentionally rewritten, verify against a fresh dev catalog.

Run:

```bash
pnpm typecheck
pnpm exec biome check .
pnpm exec vitest run
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm migrate:run
pnpm tauri dev
```

## Next Action

Finish the verification pass above, fixing any type/lint/test failures from the cleanup. If app startup fails on an existing local catalog, delete/recreate the dev catalog and rerun `pnpm migrate:run`.
