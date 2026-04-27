# Chronimage

> Windows-first, on-device AI photo organizer for hobbyist photographers.
>
> **Consolidate** your fragmented library (Google Photos / iCloud / iPhone / local). **Reclaim** cloud storage. **Rediscover** forgotten photos. **Cull** duplicates. **Develop** RAWs with non-destructive edits. All on your machine, no subscription.

Status: **Phase 6 — launch infrastructure + distribution** (Phases 1–5 closed; pre-release, running daily on dev machines).

---

## Why Chronimage

Photographers with scattered libraries pay monthly for cloud storage they don't trust, hesitate to consolidate because every tool locks them in, and never cull the 50% of shots that are duplicates. Darktable is too complex; Lightroom is too expensive and subscription-bound.

Chronimage is the middle ground: on-device AI that organizes, deduplicates (including RAW+JPG pairs from a Sony A7 IV), surfaces for rediscovery ("on this day", "unseen in 2 years"), develops RAWs non-destructively, and **safely frees up the source cloud/device after verifying a local copy**.

## Roadmap

| Phase | Scope | Status |
|---|---|---|
| 0 | Foundation, quality backbone, 4 release channels wired | ✅ shipped |
| 1 | Deep AI catalog: import, tag, face-cluster, dedupe, search, source-side cleanup, rediscovery | ✅ shipped |
| 2 | Cull + Cull Bin + Export | ✅ shipped |
| 3 | RAW Develop — non-destructive edits, 9-stage CPU pipeline, tone curves, presets, copy-paste sync | ✅ shipped |
| 4 | Prompt editing + map view + shortcut overlay + XMP sidecar import/write-out | ✅ shipped |
| 5 | Release hardening (in-code) — license + telemetry opt-in + governance docs | ✅ shipped |
| 6 | Launch infrastructure — EV cert, Cloudflare Pages, Sentry, Insider signup, Microsoft Store | ⏳ in progress |

Each phase has an authoritative PRD at [`docs/prds/phase-N.md`](./docs/prds/).

## Tech stack

Tauri v2 · React 19 · TypeScript · Tailwind v4 · Radix UI · Zustand · TanStack Router/Query · Leaflet · Rust · SQLite (+ sqlite-vec, FTS5) · rawler · libheif-rs · wgpu · ort (ONNX Runtime) · llama.cpp sidecar · SigLIP-B · ArcFace · RetinaFace · HDBSCAN · NIMA · Ed25519.

## Build

```bash
# Node ≥ 20.11, pnpm ≥ 9, Rust stable.
pnpm install
cargo fetch --manifest-path src-tauri/Cargo.toml

# Populate bundled AI models (Windows):
pwsh scripts/fetch-bundled-models.ps1
# Linux / macOS:
# bash scripts/fetch-bundled-models.sh

# Run dev:
pnpm tauri dev
```

Full onboarding: [`CLAUDE.md`](./CLAUDE.md) (agent-readable) — humans can read it too.

### Local logs

Chronimage writes backend and frontend logs to rolling local files under the app data directory (`%LOCALAPPDATA%\app.chronimage.desktop\logs` on Windows). Dev builds also mirror logs to the terminal for convenience; no log shipping service is required.

## Release channels

- **Stable** (`v1.2.3`) — monthly, signed with EV cert.
- **Beta** (`v1.2.3-beta.N`) — weekly, signed.
- **Nightly** (`v1.2.3-nightly.YYYYMMDD`) — daily from `develop`, test-signed.
- **Insider** — private invite list, Ed25519-licence-gated ([`src-tauri/src/license/`](./src-tauri/src/license/mod.rs)).

Choose channel in Settings → Updates.

## CLI

A `chronimage` CLI ships alongside the app for power users + ops:

```bash
chronimage doctor              # env + catalog + model inventory
chronimage scan <path>         # dry-run scan report
chronimage import <path> --source "My NAS"
chronimage ai audit            # all models + install state
chronimage catalog stats       # photos / tags / faces / trips / edits
chronimage license show
chronimage trips recompute
chronimage xmp rescan
```

See [`src-tauri/src/bin/chronimage-cli.rs`](./src-tauri/src/bin/chronimage-cli.rs) for the full surface.

## Quality

- **Pre-commit** (lefthook): biome + cargo fmt + clippy + typecheck + forbidden-patterns + detect-secrets.
- **PR CI**: 7 workflows — coverage, visual regression, accessibility (axe-core), security audit (cargo-deny + cargo-audit), bundle-size diff, license check, mutation tests (nightly).
- **No subscription telemetry.** Crash reports are opt-in and PII-scrubbed (Phase 6 Sentry wire-up).
- **No `unwrap()` / `expect()` / `panic!()`** in production Rust. Enforced by `scripts/forbidden-patterns.cjs` + pre-commit.

## License

Dual [MIT](./LICENSE-MIT) / [Apache-2.0](./LICENSE-APACHE) at your option. `SPDX-License-Identifier: MIT OR Apache-2.0`. See [LICENSE](./LICENSE).

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Security issues: [SECURITY.md](./SECURITY.md) (please **do not** open a public issue).
