# ADR 0004 — Import pipeline perf & model-integration fixes

**Status:** Accepted
**Date:** 2026-04-23
**Phase:** Phase 2 week 1 (performance hotfix pulled ahead of the Cull+Export work)

## Supersedes / amends

- Amends [ADR 0003](./0003-bundled-default-models.md) with the concrete ONNX integration details (input/output tensor names + axis conventions) the bundled models actually need.
- No prior ADR on execution providers; this establishes DirectML as the default on Windows.

## Context

The first Phase-2 dev-build import against a real 112-JPG folder (`C:\Users\jayas\OneDrive\Pictures\ugadi 2026`) clocked **82 minutes** — 44 s per photo. Local file/stdout traces ruled out the obvious culprits:

- OneDrive hydration (files were `Archive`, fully hydrated locally)
- Tokio scheduler (Stage 2 hash parallelism was behaving correctly)
- Disk throughput (SHA-256 streamed at ~64 MB/s — normal SSD)

Per-stage per-photo breakdowns from the `debug-import` skill ([tests/debug_import.rs](../../src-tauri/tests/debug_import.rs)) surfaced three independent root causes that the `tracing::debug!("siglip embed failed")` / `tracing::debug!("nima score failed")` lines had been masking behind silent retries.

## Decision

Ship four correlated fixes in one pass — they're each individually small but only give their payoff when applied together.

### 1. Wire DirectML as the default ONNX Runtime execution provider on Windows

New module [`src-tauri/src/ai/providers.rs`](../../src-tauri/src/ai/providers.rs) exposes `session_builder_with_ep(session_name)`.

- On Windows + `HardwareTier::GpuLow | GpuHigh`: attempts `DirectML::default().with_device_id(0).build()`. On `ort::Error<SessionBuilder>` (DLL missing, D3D12 unavailable), calls `e.recover()` to unwrap the original builder and falls through to CPU silently.
- On non-Windows or `CpuOnly` tier: plain CPU EP.
- Every path emits a `tracing::info!` line naming the EP so local logs show which accelerator each of the 5 sessions actually picked at boot.

All five `Session::builder()` call sites migrated: SigLIP image, SigLIP text, SigLIP image-only fallback ([siglip.rs](../../src-tauri/src/ai/siglip.rs)), SCRFD, ArcFace ([faces.rs](../../src-tauri/src/ai/faces.rs)), NIMA ([aesthetic.rs](../../src-tauri/src/ai/aesthetic.rs)).

Cargo feature: `ort = { features = ["download-binaries", "directml"] }` — Microsoft's prebuilt `onnxruntime.dll` already ships `DirectML.dll`, no extra dependency.

### 2. Fix the NIMA NHWC / NCHW axis mismatch

The bundled `nima.onnx` (MobileNetV2, community export) expects **NHWC** `[1, 224, 224, 3]`. The Phase-1 code constructed **NCHW** `[1, 3, 224, 224]` and a channels-first pixel buffer. Every NIMA inference in Phase 1 silently errored with:

```
ort run: Got invalid dimensions for input: input
  index 1: Got 3, Expected 224
  index 3: Got 224, Expected 3
```

The error bubbled up as `tracing::debug!("nima score failed")` — never visible in the dev terminal at the default log level, and zero aesthetic scores landed in the DB.

Fix in [aesthetic.rs](../../src-tauri/src/ai/aesthetic.rs):

- Tensor shape: `[1, 224, 224, 3]`
- `preprocess_image` packing: row-major over `(y, x, c)` instead of `(c, y, x)`.

### 3. Robust SigLIP output-name extraction

The bundled `siglip2-b16-image.onnx` (`onnx-community/siglip2-base-patch16-224-ONNX` → `onnx/vision_model.onnx`) went through multiple re-exports upstream — the output tensor name has flipped between `image_embeds`, `pooler_output`, and `sentence_embedding` at various points in its history. Phase-1 code hardcoded `"image_embeds"`, which was silently failing on every photo in the version currently bundled.

Fix: [`extract_embedding` in siglip.rs:269](../../src-tauri/src/ai/siglip.rs#L269) now iterates a candidate list (`image_embeds`, `pooler_output`, `sentence_embedding`) and takes the first that exists AND matches the expected 768-dim shape. On a miss it logs which names the model actually exposes, so a future model swap surfaces the needed name immediately instead of silent zeroed-out embeddings.

### 4. Per-crate `opt-level = 3` overrides for hot compute dependencies in the `dev` profile

This is the biggest fix by wall-clock impact. Root cause: `[profile.dev] opt-level = 0` in Cargo.toml applies to the entire dep graph including compute-heavy crates (`image`, `zune-jpeg`, `image_hasher`, `rustdct`, `rustfft`, `ort`, `ndarray`, `rawler`, `libheif-rs`, `sha2`). JPEG decoding at `-O0` runs 50–100× slower than `-O3`, and every stage of the pipeline decodes each JPG one or more times.

[Cargo.toml](../../src-tauri/Cargo.toml):

```toml
[profile.dev]
incremental = true
opt-level = 0

[profile.dev.package."image"]
opt-level = 3
# ...15 more hot deps at opt-level = 3
```

App code stays at `opt-level = 0` for fast iteration. Cargo ignores overrides for crates not in the graph so extra entries are harmless.

## Measured impact

One debug-import run against the same 112-JPG, 8 MB/photo folder:

| Stage                         | Before (O0)  | After (O3 hot deps) | Speedup |
|-------------------------------|--------------|---------------------|---------|
| 1 — scan                      | 1 ms         | 1 ms                | —       |
| 2 — hash + EXIF + pHash + thumb | 735 s / 6.6 s/photo | 58 s / 520 ms/photo | 12.7× |
| 3 — pairs                     | 0 ms         | 0 ms                | —       |
| 4 — NIMA + SigLIP             | 780 s / 7.0 s/photo | 95 s / 852 ms/photo | 8.3× |
| 5 — RetinaFace + ArcFace      | 3 392 s / 30.6 s/photo | 78 s / 698 ms/photo | 43.7× |
| **Total**                     | **4 908 s (82 min)** | **230 s (3.8 min)** | **21.3×** |

NIMA scores produced: **0 → 111 / 111**. SigLIP embeddings: **0 → 111 / 111**. Face detections: 145.

Per-photo p50 sub-stage cost post-fix:
- `hash_ms` 18, `meta_ms` 2 029, `thumb_ms` 0 (cache hit)
- `nima_ms` 2 296, `siglip_ms` 962

## Alternatives considered

1. **Leave `opt-level = 0` on image crates; rewrite decoders in hand-optimised code.** Rejected — fixes one crate, not the root cause, and trades maintenance for tiny wins versus the profile override.
2. **Build every dev run in release mode (`cargo build --release` for `tauri dev`).** Rejected — adds ~60 s to every first-compile and destroys iteration speed on app code. The per-crate override is the best of both worlds.
3. **Bundle a different SigLIP ONNX (the all-in-one CLIP-style export with `image_embeds`).** Rejected — the `onnx-community` `vision_model.onnx` + `text_model.onnx` split we already bundle is what gives us query-time text embedding for search. The one-line candidate list is cheaper than a model swap + re-verification.
4. **Keep CPU EP as the default; make DML opt-in.** Rejected — on any dGPU-equipped machine DML is 5–10× faster than CPU for SigLIP and face models, and the `Err.recover()` fallback makes the risk asymmetric in favour of trying DML first.

## Follow-ups (not yet shipped)

- **Decode-once, infer-many.** Stages 2+4+5 each independently call `image::open(path)` on the same JPG. A shared decoded `DynamicImage` per photo — passed through all models in the same task — would cut the remaining ~2 s/photo in half. Tracked for a Phase-2 perf pass.
- **Per-photo progress through stages 4–5 in the UI.** Today the progress event only advances during Stage 2. User sees `done == total` but AI work keeps going. Needs a second event stream.
- **GPU utilisation verification under load.** Local logs confirm DML is *registered*, but we don't have a proof we're GPU-bound rather than decode-bound during Stage 4. A quick nvidia-smi trace during a debug-import run would settle it.

## References

- `src-tauri/src/ai/providers.rs` — EP helper
- `src-tauri/src/ai/aesthetic.rs` — NIMA NHWC fix
- `src-tauri/src/ai/siglip.rs` — output-name fallback
- `src-tauri/Cargo.toml` — profile overrides
- `src-tauri/tests/debug_import.rs` + `.claude/commands/debug-import.md` + `scripts/debug-import.sh` — the diagnostic tooling this ADR was measured against
