---
description: Load every bundled/installed ONNX model and report shape + throughput on the current host.
---

Run `cargo run -p chronimage-cli -- ai audit` and report:

| Model | Path | Params | Input shape | Device | Throughput (items/s) | Notes |
|---|---|---|---|---|---|---|

Fail loudly if any model fails to load, including the DirectML / CUDA / CPU fallback chain.

The audit should:
1. Load each model via `ort`
2. Warm up with 5 inferences
3. Benchmark 100 inferences
4. Print memory delta before/after load
5. Verify output shape matches expected constants in `src-tauri/src/ai/models.rs`

Report anomalies (e.g., model loaded on CPU when DirectML should have been available).
