# ADR 0002 — Model Download Strategy

**Status:** Accepted  
**Date:** 2026-04-20  
**Phase:** 1 (Deep AI Catalog)

---

## Context

Chronimage ships five on-device AI models covering embeddings, aesthetics, face
detection/embedding, and scene captioning. Each must reach the user's machine
without being bundled in the installer (constraint: installer size and model
licensing), and must degrade gracefully when absent.

---

## Models

| Name | Kind | Format | Size | Notes |
|---|---|---|---|---|
| `siglip-b16-image.onnx` | embedding | ONNX | ~350 MB | CPU-fast; int8 quantised |
| `nima.onnx` | aesthetic | ONNX | ~14 MB | MobileNet; very fast on CPU |
| `retinaface-r50.onnx` | face-detect | ONNX | ~110 MB | ResNet50 backbone |
| `arcface-r100.onnx` | face-embed | ONNX | ~260 MB | 512-dim L2-normalised output |
| `gemma-4-9b-it-q4_k_m.gguf` | caption-gguf | GGUF | ~5.8 GB | GPU-only; llama.cpp sidecar |

---

## Decision

### Distribution: HuggingFace, download on first run

All models are hosted at `https://huggingface.co/Chronimage/models/resolve/main/`.

Alternatives considered and rejected:
- **Bundle in installer** — inflates installer by 6+ GB; unacceptable for a
  ~100 MB target installer size.
- **Azure Blob / S3** — requires Chronimage to operate storage infrastructure
  and pay egress costs per user; HuggingFace provides free model hosting with
  LFS and CDN.
- **Torrent / IPFS** — no reliable Windows tooling in a Tauri context; adds
  significant complexity for no end-user benefit.

### "tbd" SHA256 policy

`ModelSpec.sha256` is set to `"tbd"` while the HuggingFace repo is being
populated. When `sha256 == "tbd"`:

- The download layer skips hash verification and accepts whatever the server
  returns.
- `ai_models_status` treats a present file as installed without hashing it.

Final SHA256 values are recorded by the release pipeline immediately after
model files are uploaded, then committed to `download.rs` before any stable
release. The `"tbd"` policy is explicitly a **dev/nightly-only** escape hatch.

### First-run UX

1. The app starts with all sessions as stubs (`is_stub = true`).
2. Settings → Models shows each model with `installed: false` and a Download
   button.
3. The user triggers `download_models` (user-initiated, satisfying the
   no-background-network-calls rule).
4. Progress is streamed via `chronimage://download-progress` events.
5. Downloads write to a `.onnx.tmp` / `.gguf.tmp` scratch file; the final
   rename is atomic. On failure or hash mismatch the scratch file is removed
   and the user can retry.
6. After successful download the app reports the new `installed: true` status
   via `ai_models_status`. Sessions are upgraded from stub to real on next
   app restart (Phase 1 policy; live hot-swap tracked for Phase 2).

Model files land in:
- `%LOCALAPPDATA%\app.chronimage.desktop\models\` (default)
- `<catalog_root>\models\` when the user chose a custom drive as catalog root.

### Stub / real degradation

- CPU-only machines: SigLIP, RetinaFace, ArcFace run as stubs until ONNX files
  are downloaded. Caption session is always stub on CPU (GPU-only policy).
- GPU machines: all five models are available; the VRAM budgeter in
  `src-tauri/src/ai/budget.rs` selects the appropriate variant.
- Degradation is surfaced: `FacesSession.is_stub` and `CaptionSession.is_stub`
  are public fields. The status bar command `ai_models_status` reports
  `installed: false` so the UI can prompt the user rather than silently
  returning empty results.

---

## Open issues

- **gemma GGUF is 5.8 GB.** On a slow connection (e.g. 10 Mbps) this takes
  ~90 minutes. Mitigation planned for Phase 2: HTTP Range + resumable downloads
  via a persistent `.partial` offset file. For Phase 1 the user must keep the
  app open or retry from the beginning.
- **Sidecar binary path** for `llama-server.exe` is not yet resolved from the
  Tauri bundle directory. `CaptionSession::load_or_stub` receives `None` for
  the binary path until Phase 1b wires `tauri::utils::platform::current_exe()`
  sibling lookup.
- **Live session hot-swap** after download completes is deferred to Phase 2.
  Phase 1 requires an app restart to activate newly downloaded models.
