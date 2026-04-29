---
description: Iterate SAM2.1 mask prompt strategies on a single image. Encode once, decode many strategies, save alpha + overlay PNGs side-by-side for visual inspection. Use when "the {subject,sky,background,person} mask is wrong" reports come in.
argument-hint: <path-to-image>
---

# mask-debug — iterate SAM2.1 prompt strategies against a real image

Runs the `chronimage-mask-debug` bin against `$ARGUMENTS`. The bin loads SAM2.1 + SCRFD-10g from `src-tauri/models/bundled/`, encodes the image once (slow, ~5–30 s on CPU), then decodes every prompt strategy in `STRATEGIES` against the cached features (sub-second per strategy). Each strategy emits two PNGs into `src-tauri/target/mask-debug/`:

- `<name>.alpha.png` — the raw 8-bit luma mask at full image resolution.
- `<name>.overlay.png` — the original image with the mask region red-tinted, the rest desaturated to grey, plus markers (green = positive prompt, magenta = negative, cyan = SCRFD face box).

Built specifically because the `develop_mask_generate` Tauri command bakes the prompt set into the `develop::sam::build_prompts` switch — there's no way to A/B prompt strategies through the UI without restarting the app each time.

## When to reach for this

- A user report says "subject / sky / background mask is wrong on this photo" and you need to see what SAM saw.
- You want to sanity-check that SCRFD-10g is detecting the face you expect — strategies 02–08 only run if a face is found, and the cyan box on the overlay shows you exactly where SCRFD landed.
- You're tuning `build_prompts` and want a tight before/after comparison without rebuilding the full Tauri app.

## Precondition

The bundled SAM2.1 + SCRFD models must already be in `src-tauri/models/bundled/`:

```bash
ls src-tauri/models/bundled/sam2.1_hiera_large.encoder.onnx \
   src-tauri/models/bundled/sam2.1_hiera_large.decoder.onnx \
   src-tauri/models/bundled/det_10g.onnx \
   src-tauri/models/bundled/w600k_r50.onnx
```

If any are missing, run `pwsh scripts/fetch-bundled-models.ps1` first.

## How to run

```bash
# Bash / Git Bash — runs every strategy
cargo run --manifest-path src-tauri/Cargo.toml --bin chronimage-mask-debug -- \
    --image "$ARGUMENTS" \
    --out src-tauri/target/mask-debug

# Run a single named strategy
cargo run --manifest-path src-tauri/Cargo.toml --bin chronimage-mask-debug -- \
    --image "$ARGUMENTS" \
    --strategy 02_face_center_plus_torso

# Dump SAM decoder logit stats (shape, min/max) for diagnostics
CHRONIMAGE_DEBUG_SAM=1 cargo run --manifest-path src-tauri/Cargo.toml \
    --bin chronimage-mask-debug -- --image "$ARGUMENTS"
```

After the run, **open the overlays in your IDE** — `Read` the PNGs back so you can see what SAM produced. Coverage % is printed per strategy; near-zero coverage usually means the encoder/decoder pipeline itself is broken (see "Decoder shape" below).

## What to report

For each strategy, summarise in this format (one line each):

```
<name>  coverage=N.N%  decoded=N ms  verdict=<good|bleeds-into-X|misses-Y|empty>
```

Then pick the winning strategy and either:

1. **Port it to `develop::sam::build_prompts`** if it beats what's already there. Keep `build_prompts` as the canonical source of truth — the bin's `STRATEGIES` is for iteration, not production.
2. **Add it as a new strategy in the bin** if you want to compare it against future variants.

## Adding new strategies

Each strategy is a `(name, prompts)` pair pushed into `build_strategies` in `src-tauri/src/bin/chronimage-mask-debug.rs`. Coordinates are normalised `(x, y, label)` triples; `label = 1.0` is foreground, `0.0` is background.

```rust
out.push((
    "09_my_new_idea".into(),
    with_dense_border_negatives(vec![
        (cx, cy, 1.0),
        (some_x, some_y, 1.0),
    ]),
));
```

Names sort alphabetically in the output dir, so prefix with a number to control display order.

## Reusing this loop for other features

The pattern — *encode the expensive thing once, vary the cheap thing many ways, write side-by-side artefacts for visual diff* — works for any pipeline where a hand-tuned heuristic decides what gets fed to a model. Likely candidates:

- **Sky / foreground / background masks** — same bin, different `source` arg in `build_prompts`. The bin's strategy list is currently subject-only; extend `build_strategies` to vary `source` if those need attention.
- **Face-clustering thresholds** — adapt the pattern to `chronimage-face-labeler`: cluster once with HDBSCAN, sweep the `min_cluster_size` / `epsilon` knobs, save per-cluster contact sheets.
- **NIMA aesthetic gating** — score once, sweep the cutoff, save kept-vs-rejected grids.
- **Dedupe similarity threshold** — embed once, sweep the SigLIP cosine cutoff, save false-positive / false-negative pairs.

For each new application: copy `chronimage-mask-debug.rs` to a sibling bin, swap the model/decoder, keep the "encode once, decode many, save side-by-side" loop, and write a sibling slash command that mirrors this one.

## Decoder shape — known-good baseline

The vietanhdev SAM2.1 ONNX export emits masks at `[1, num_masks, 256, 256]`. `develop::sam::threshold_to_alpha` upsamples that 256×256 logit grid back to original image resolution, accounting for the 1024×1024 letterbox. If you ever see every strategy report 0% coverage, the upsample path is the first thing to check — that bug ate every mask SAM produced before 2026-04-29.

## Source

- Bin: `src-tauri/src/bin/chronimage-mask-debug.rs`
- Public API exercised: `develop::sam::SamSession::{encode_features, decode_normalized, generate_bitmap_mask}`
- Production prompt builder: `develop::sam::build_prompts`
- Production decoder + upsample: `develop::sam::SamSession::decode` + `threshold_to_alpha`
