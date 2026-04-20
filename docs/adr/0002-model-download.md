# ADR 0002 — Model Download Strategy

**Status:** Accepted  
**Date:** 2026-04-20  
**Phase:** 1 (Deep AI Catalog)

---

## Supersedes

The original model set (all hosted on the private `Chronimage/models` HuggingFace repo
with placeholder `"tbd"` hashes) has been replaced with freely-available community models
that require no HuggingFace token:

| Replaced | New | Notes |
|---|---|---|
| RetinaFace-R50 (private HF repo) | SCRFD-10g (InsightFace MIT, buffalo_l.zip) | Gated private URL → freely-downloadable GitHub release |
| ArcFace-R100 (private HF repo) | ArcFace W600K R50 (InsightFace MIT, buffalo_l.zip) | Gated private URL → freely-downloadable GitHub release |
| Gemma-4-9B-it Q4_K_M (gated HF repo) | Moondream2 1.9B f16 (Apache 2.0, vikhyatk/moondream2) | 5.8 GB gated model replaced with 1.7 GB open model |
| SigLIP-1 B/16 (private HF repo) | SigLIP-2 B/16 naflex (Apache 2.0, onnx-community) | ~5pt retrieval improvement; same ~375 MB footprint |

---

## Context

Chronimage ships five on-device AI models covering embeddings, aesthetics, face
detection/embedding, and scene captioning. Each must reach the user's machine
without being bundled in the installer (constraint: installer size and model
licensing), and must degrade gracefully when absent.

---

## Models

| Name | Kind | Format | Size | Licence | Notes |
|---|---|---|---|---|---|
| `siglip2-b16-image.onnx` | embedding | ONNX | ~375 MB | Apache 2.0 | onnx-community/siglip2-base-patch16-naflex |
| `nima.onnx` | aesthetic | ONNX | ~14 MB | — | MobileNet; very fast on CPU |
| `det_10g.onnx` | face-detect | ONNX | ~30 MB (extracted from buffalo_l.zip ~275 MB) | MIT | SCRFD-10g with keypoints (named `det_10g.onnx` inside the buffalo_l bundle; was `scrfd_10g_bnkps.onnx` in the standalone release) |
| `w600k_r50.onnx` | face-embed | ONNX | ~130 MB (extracted from buffalo_l.zip ~275 MB) | MIT | ArcFace W600K R50, 512-dim |
| `moondream2-text-model-f16.gguf` | caption-gguf | GGUF | ~1.7 GB | Apache 2.0 | Vision-language; llama.cpp sidecar |

**First-run download total:** ~2.3 GB (down from ~6.6 GB with Gemma-4-9B).
SigLIP + NIMA + face models total ~660 MB; Moondream2 adds ~1.7 GB.

---

## Rationale: model swaps

### RetinaFace-R50 → SCRFD-10g

- **Licensing:** RetinaFace was on a private repo requiring token access. SCRFD-10g
  ships in InsightFace's public `buffalo_l.zip` GitHub release under MIT.
- **Size:** SCRFD-10g is ~30 MB extracted vs. ~110 MB for RetinaFace-R50.
- **Performance:** Per InsightFace benchmarks, SCRFD-10g achieves higher average
  precision on WiderFace Hard than RetinaFace-R50 at similar inference latency.
- **Preprocessing change:** SCRFD expects `(x - 127.5) / 128.0` RGB normalisation,
  not RetinaFace's BGR mean-subtract. The Phase-1b wiring must account for this.

### ArcFace-R100 → ArcFace W600K R50

- **Licensing:** Same as above — private HF repo replaced by the public buffalo_l.zip.
- **Size:** W600K R50 is ~130 MB vs. ~260 MB for R100.
- **Performance:** W600K R50 is the community-standard checkpoint used in most
  InsightFace integrations. Per author-reported numbers, verification accuracy on
  IJB-C is comparable to R100 while being 2× smaller.
- **Output:** Same 512-dim L2-normalised embedding; downstream cosine-similarity
  code is unchanged.

### Gemma-4-9B-it Q4_K_M → Moondream2 1.9B f16

- **Licensing:** Gemma-4-9B requires a HuggingFace token (gated model agreement).
  Moondream2 is Apache 2.0 with no token needed.
- **Size:** 1.7 GB vs. 5.8 GB — reduces first-run download by 4.1 GB.
- **Capability:** Moondream2 is a purpose-built vision-language model optimised for
  "describe this photo" prompts. Gemma-4-9B is a text-only model that would have
  required a separate vision projector; Moondream2 handles both in one GGUF file.
- **CPU capability:** Moondream2 runs at ~1 s/image on CPU (author-reported). The
  GPU gate is retained for Phase 1 parity; relaxing to CPU is tracked for Phase 2.

### SigLIP-1 B/16 → SigLIP-2 B/16 naflex

- **Licensing:** Both are Apache 2.0; new model is from the `onnx-community` org,
  eliminating the private Chronimage HF repo dependency.
- **Size:** ~375 MB vs. ~350 MB (negligible increase).
- **Performance:** SigLIP-2 naflex shows ~5pt improvement in zero-shot retrieval
  benchmarks vs. SigLIP-1 (per Google's SigLIP-2 paper, 2025).

---

## Decision

### Distribution: community model hosting, download on first run

Face models are hosted at GitHub Releases (InsightFace). Caption and embedding
models are on public HuggingFace repos (no token required). NIMA remains on
the private Chronimage HF repo pending a community replacement.

Alternatives considered and rejected:
- **Bundle in installer** — inflates installer by 2+ GB; unacceptable for a
  ~100 MB target installer size.
- **Azure Blob / S3** — requires Chronimage to operate storage infrastructure
  and pay egress costs per user.
- **Torrent / IPFS** — no reliable Windows tooling in a Tauri context.

### "tbd" SHA256 policy

`ModelSpec.sha256` is set to `"tbd"` while hashes are being locked. When
`sha256 == "tbd"`:

- The download layer skips hash verification and accepts whatever the server returns.
- `ai_models_status` treats a present file as installed without hashing it.

Final SHA256 values are recorded by the release pipeline and committed to
`download.rs` before any stable release. The `"tbd"` policy is a **dev/nightly-only**
escape hatch.

### Zip-bundle downloads

InsightFace distributes SCRFD-10g and ArcFace W600K R50 together in
`buffalo_l.zip` (~275 MB). Both `KNOWN_MODELS` entries point at the same URL.

**Phase 1 implementation (simplicity):** the zip is re-downloaded once per
`ModelSpec` entry. After download, only `spec.filename` is extracted from
`buffalo_l/<filename>.onnx` inside the archive; the zip is deleted immediately.
This means the 275 MB zip is downloaded twice at onboarding if the user installs
both face models in the same session.

Rationale for accepting this in Phase 1: downloads are user-initiated once at
onboarding; the cost is ~275 MB of extra bandwidth, which is acceptable given
the simplicity of the implementation.

**Phase 2 plan:** cache the zip in a session-scoped temp path, extract all
requested files before deletion, reducing the redundant download to zero.

### First-run UX

1. The app starts with all sessions as stubs (`is_stub = true`).
2. Settings → Models shows each model with `installed: false` and a Download button.
3. The user triggers `download_models` (user-initiated, satisfying the
   no-background-network-calls rule).
4. Progress is streamed via `chronimage://download-progress` events.
5. For direct downloads: write to a `.onnx.tmp` / `.gguf.tmp` scratch file;
   final rename is atomic.
   For zip bundles: download to `.zip.tmp`, extract target file to `.extract.tmp`,
   rename to final destination, delete zip.
6. After successful download the app reports `installed: true` via `ai_models_status`.
   Sessions are upgraded from stub to real on next app restart (Phase 1 policy;
   live hot-swap tracked for Phase 2).

Model files land in:
- `%LOCALAPPDATA%\app.chronimage.desktop\models\` (default)
- `<catalog_root>\models\` when the user chose a custom drive as catalog root.

### Stub / real degradation

- CPU-only machines: SigLIP2, SCRFD, ArcFace run as stubs until ONNX files
  are downloaded. Caption session is always stub on CPU (GPU-only policy, Phase 1).
- GPU machines: all five models are available; the VRAM budgeter in
  `src-tauri/src/ai/budget.rs` selects the appropriate variant.
- Degradation is surfaced: `FacesSession.is_stub` and `CaptionSession.is_stub`
  are public fields. The status bar command `ai_models_status` reports
  `installed: false` so the UI can prompt the user rather than silently
  returning empty results.

---

## Open issues

- **NIMA** is still on the private `Chronimage/models` HF repo. A community
  NIMA ONNX export should be found or produced before stable release.
- **Moondream2 mmproj:** The llama.cpp Moondream2 integration may require a
  separate `moondream2-mmproj-*.gguf` vision projector file. Verify against
  llama.cpp docs in Phase-1b and add a second `ModelSpec` entry if needed.
- **Sidecar binary path** for `llama-server.exe` is not yet resolved from the
  Tauri bundle directory. `CaptionSession::load_or_stub` receives `None` for the
  binary path until Phase 1b wires `tauri::utils::platform::current_exe()` sibling
  lookup.
- **Live session hot-swap** after download completes is deferred to Phase 2.
  Phase 1 requires an app restart to activate newly downloaded models.
- **SHA256 hashes:** all five entries have `sha256 = "tbd"`. Lock real hashes
  before the first beta release. The release pipeline should compute them
  immediately after model files are verified and commit the values to `download.rs`.
