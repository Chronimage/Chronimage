# Checkpoint 2026-04-21 · Phase 1 — exit-criteria numbers

Branch: `feature/phase1-week5-followup` (uncommitted; about to commit + PR).

## Exit-criteria scoreboard (PRD § Non-functional requirements)

| Test | Status | Measured | Threshold | Headroom |
|---|---|---|---|---|
| `phase_1_raw_jpg_pair` | ✅ | precision **1.0000** (5 000 / 5 000) | ≥ 0.995 | ~0.5 pt |
| `phase_1_catalog_size` | ✅ | ratio **0.00202** (7.4 MB / 3.66 GB) | ≤ 0.02 | 10× |
| `phase_1_face_clustering` | ✅ | primary-cluster F1 **1.0000** (200 LFW photos, 10 identities, prealigned) | ≥ 0.95 | 0.05 pt |
| `scrfd_detects_faces_in_real_group_photo` | ✅ | ≥ 3 faces in `tests/fixtures/face-detect/group.jpg` | ≥ 3 | — |
| `phase_1_search_latency` | ❌ | p95 **9 396 ms** (200 k synthetic, BLOB-fallback linear scan) | ≤ 500 ms | 19× over |
| `phase_1_stress` | ⏳ | `unimplemented!()` — nightly only, 8 h loop | — | — |
| `tests/e2e/phase-1-import-throughput.spec.ts` | ⏳ | `.skip()` — needs 100 k fixture | — | — |
| `tests/e2e/phase-1-source-cleanup.spec.ts` | ⏳ | `.skip()` — needs 100-photo fixture | — | — |
| `tests/e2e/phase-1-rediscovery.spec.ts` | ⏳ | `.skip()` — needs dated fixture | — | — |

**4 of 8 green with numbers; 1 hard-failing; 4 still unscaffolded.**

## The one failing number — what it means

`phase_1_search_latency` measures `search_photos` on a 200 k catalog via its current implementation (linear BLOB cosine over `photo_embeddings.embedding` column). p95 of 9 396 ms confirms the PRD design assumption that NL search at 200 k catalog scale **requires vec0 KNN** (the pipeline now writes into `vec_photo_embeddings` per commit `09876xxx`, but `search_photos` hasn't been ported to the KNN query yet). Next session: swap `search_photos` to `SELECT rowid FROM vec_photo_embeddings WHERE embedding MATCH ? ORDER BY distance LIMIT ?`. Expected p95 drops to ~10–50 ms.

## Other work landed this session

- **ort bindings fixed on Windows** — switched from `load-dynamic` (which was silently resolving to `C:\Windows\System32\onnxruntime.dll` — Windows AI Foundry's germanium build, ABI-incompatible with ort) to `download-binaries` (pins Microsoft's 1.24.2 runtime via pyke's CDN). `Session::commit_from_file` now initialises deterministically. DirectML.dll copied next to test binaries because Windows Developer Mode is off; build script warns but works.
- **Real SigLIP image + text inference** — replaced stubs with real ort sessions for both encoders. Added `tokenizers = "0.21"` (MIT/Apache-2.0). Bundled + ModelSpec rows for `siglip2-b16-text.onnx` and `siglip2-b16-tokenizer.json`. Pipeline stage-4 now also populates `vec_photo_embeddings`. `search_photos` routes through `global_siglip_session()`. 217 → 219 lib tests (+2 for stub-contract retention).
- **LFW fixture builder** — `scripts/fetch-lfw-fixture.ps1` downloads lfwcrop_color.zip (mirror — primary umass mirror DNS unresponsive), parses flat-PPM layout into 200 photos across top-10 identities, writes `labels.json`. Gitignored — regenerate locally.
- **`FacesSession::embed_prealigned_face`** — new helper for pre-cropped face fixtures like LFW that bypass SCRFD (the 64×64 crops are out of SCRFD's training distribution; detector returns 0 on every one). Resize → 112×112, normalise, run ArcFace. Test auto-falls-back to this mode after 10 consecutive empty detections.
- **`chronimage-face-labeler` binary + `tests/fixtures/face-clusters/label.html`** — dev-only labeling tool. Scans a directory, runs real SCRFD + ArcFace, saves thumbnails + `candidates.json`; HTML page shows thumbnail grid with per-face cluster-id input, writes `labels.json`. Deferred to end-of-phase-1 per user call — LFW fixture carries us through.
- **Per-photo logs in `phase_1_face_clustering`** — diagnostic output flushes to stderr immediately (via explicit `stderr().flush()`) so hangs are visible.

## Current branch state

`feature/phase1-week5-followup` — uncommitted changes:
- `src-tauri/Cargo.toml` (ort feature swap)
- `src-tauri/src/ai/faces.rs` (embed_prealigned_face)
- `src-tauri/src/ai/siglip.rs` (real inference — agent commit)
- `src-tauri/src/ai/download.rs` (text encoder + tokenizer entries)
- `src-tauri/src/commands.rs` (search_photos → global_siglip_session)
- `src-tauri/src/main.rs` (init_global_siglip_session in boot spawn_blocking)
- `src-tauri/src/import/pipeline.rs` (vec_photo_embeddings insert + memoised faces session)
- `src-tauri/tests/phase_1_catalog_size.rs` (sources.created_at fix; 512×512 synthesise)
- `src-tauri/tests/phase_1_search_latency.rs` (sources.created_at fix)
- `src-tauri/tests/phase_1_face_clustering.rs` (per-photo logs + prealigned-mode fallback)
- `src-tauri/src/bin/chronimage-face-labeler.rs` (new)
- `tests/fixtures/face-clusters/label.html` (new)
- `scripts/fetch-lfw-fixture.ps1` (new)
- Various doc updates (ADR 0003, PRD phase-1)

All gates green: cargo fmt/clippy/test (219 lib)/deny, pnpm typecheck/vitest (64)/biome.

## Next session should focus on

1. **Migrate `search_photos` to vec0 KNN** — single PRD NFR still unmet; vec_photo_embeddings already populated by stage-4.
2. **Re-run `phase_1_search_latency` with vec0 path** — prove p95 ≤ 500 ms.
3. **3 remaining e2e fixtures** — import-throughput, source-cleanup, rediscovery (lowest urgency given Rust integration tests covering the same NFRs).
4. **CI packaging job** — fetch bundled models + LICENSE files ahead of `tauri build`.
5. **`phase_1_stress`** — 8 h nightly loop.

Once (1) + (2) pass, Phase 1 exit is a matter of shipping the remaining fixtures + stress test. The functional Phase 1 scope is effectively complete.
