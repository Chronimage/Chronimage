# Chronimage

> Windows-first, on-device AI photo organizer for hobbyist photographers.
>
> **Consolidate** your fragmented library (Google Photos / iCloud / iPhone / local). **Reclaim** cloud storage. **Rediscover** forgotten photos. **Cull** duplicates. All on your machine, no subscription.

Status: **Phase 0 — foundation & quality backbone** (pre-alpha; not yet runnable).

---

## Why Chronimage

Photographers with scattered libraries pay monthly for cloud storage they don't trust, hesitate to consolidate because every tool locks them in, and never cull the 50% of shots that are duplicates. Darktable is too complex; Lightroom is too expensive and subscription-bound.

Chronimage is the middle ground: on-device AI that organizes, deduplicates (including RAW+JPG pairs from a Sony A7 IV), surfaces for rediscovery ("on this day", "unseen in 2 years"), and **safely frees up the source cloud/device after verifying a local copy**.

## Roadmap

| Phase | Scope | Status |
|---|---|---|
| 0 | Foundation, quality backbone, 4 release channels wired | ⏳ in progress |
| 1 | Deep AI catalog: import, tag, face-cluster, dedupe, search, source-side cleanup, rediscovery | ⏸ pending |
| 2 | Cull + Cull Bin + Export | ⏸ |
| 3 | RAW Develop (GPU pipeline, curves, masks, presets) | ⏸ |
| 4 | Prompt editing + tweaks + map view | ⏸ |
| 5 | Release hardening, signing, store, website | ⏸ |

Each phase has an authoritative PRD at `docs/prds/phase-N.md`.

## Tech stack

Tauri v2 · React 19 · TypeScript · Tailwind v4 · Radix UI · Zustand · TanStack Router/Query · Rust · SQLite (+ sqlite-vec, FTS5) · rawler · libheif-rs · wgpu · ort (ONNX Runtime) · llama.cpp sidecar · SigLIP-B · ArcFace · RetinaFace · HDBSCAN · NIMA.

## Build

```bash
pnpm install
cargo fetch --manifest-path src-tauri/Cargo.toml
pnpm tauri dev
```

Full onboarding: [`CLAUDE.md`](./CLAUDE.md) (agent-readable) — humans can read it too.

## Release channels

- **Stable** (`v1.2.3`) — monthly, signed with EV cert.
- **Beta** (`v1.2.3-beta.N`) — weekly, signed.
- **Nightly** (`v1.2.3-nightly.YYYYMMDD`) — daily from `develop`, test-signed.
- **Insider** — private invite list, license-gated.

Choose channel in Settings → Updates.

## Quality

- Pre-commit: lefthook runs biome + cargo fmt + clippy + typecheck + forbidden-patterns.
- PR CI: 7 workflows. Coverage, visual regression, accessibility (axe), security audit, bundle-size diff, license check, mutation tests (nightly).
- No subscription telemetry. Crash reports opt-in only, Phase 5+.

## License

TBD. See [LICENSE](./LICENSE).

## Contributing

This is currently a solo build. Public contribution guidelines land in Phase 5.
