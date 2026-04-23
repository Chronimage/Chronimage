# Checkpoint 2026-04-24 · Phase 2 week 1 — perf hotfix + onboarding kill

Branch: `develop` · 50 files changed · 0 commits yet (working tree)

## Summary

Week 1 of Phase 2 turned into a hotfix sprint after a debug-import run on a real 112-JPG folder measured **82 minutes wall clock** — effectively unusable. Root-cause investigation via new diagnostic tooling (debug-import skill + Loki queries) surfaced four independent bugs, all now fixed. Side effect: user feedback during testing drove a full onboarding redesign + deletion infrastructure build-out, both pulled forward from Phase-2 backlog.

End state: **22× speedup** (82 min → 3.8 min on the same 112-JPG folder), onboarding wizard deleted (catalog is now the home), three explicit deletion scopes with Recycle-Bin routing, and 262/262 Rust tests + 85/85 frontend tests green.

## What shipped

### Performance & AI model integration — [ADR 0004](../adr/0004-import-pipeline-perf-and-model-integration.md)

- **DirectML EP wired for all 5 ort sessions** — new `src-tauri/src/ai/providers.rs::session_builder_with_ep()`. Registers DML on Windows + GpuLow/GpuHigh tier, silently falls back to CPU on DLL/D3D12 errors. Measured on RTX 5070 Laptop (7891 MB VRAM) per Loki boot logs.
- **NIMA NHWC axis fix** — bundled `nima.onnx` expects `[1, 224, 224, 3]`, Phase-1 code sent NCHW `[1, 3, 224, 224]`. Every aesthetic score in Phase 1 was silently erroring with a `tracing::debug!` that never surfaced. Fixed in `src-tauri/src/ai/aesthetic.rs:57` + preprocess loop repacked row-major `(y, x, c)`.
- **SigLIP output-name fallback** — bundled `vision_model.onnx` was upstream-re-exported with a different output name (`pooler_output`) vs. Phase-1's hard-coded `image_embeds`. `extract_embedding` now iterates a candidate list and reports actual exposed names on total miss.
- **Dev-profile `opt-level = 3` on 17 hot crates** — `image`, `zune-jpeg`, `image_hasher`, `rustdct`, `rustfft`, `ort`, `ndarray`, `rawler`, `libheif-rs`, `sha2`, etc. Biggest single fix by wall-clock: JPEG decode at `-O0` runs 50–100× slower than `-O3`, and every pipeline stage decodes each JPG at least once. App code stays at `-O0` for fast iteration.

### Onboarding removal + catalog-is-home redesign — [ADR 0005](../adr/0005-onboarding-removal-catalog-as-home.md)

- `OnboardScreen.tsx` (1452 LOC) + `OnboardScreen.test.tsx` (10 tests) deleted.
- Default screen flipped from `onboard` to `catalog`; `'onboard'` removed from `ScreenId` + `SCREENS` + Rail.
- `src/state/import.ts` — Zustand store for active imports, `useImportProgressListener()` mounted once at app root.
- `src/state/settings.ts` — plugin-store wrappers for `default_import_mode` ('index_in_place' | 'consolidate' | null) and `catalog_home_path`.
- `src/primitives/ConfirmDialog.tsx` — reusable modal with checkbox-row scope options, ESC + backdrop-click cancel.
- Four new catalog sub-screens: `CatalogEmptyState`, `AddSourcePopover`, `SourcesPanel`, `ImportProgressCard`.
- `CatalogSidePanel` rewired: hardcoded `99.6%` fixture replaced with real import progress; inert sources list replaced with `SourcesPanel` (click-to-disconnect + `+` button for new sources).
- Settings gains a Library section (default import mode radio + catalog home path + Change picker).
- `.body` grid CSS regression fix: screens without a side panel (Settings, People) were landing in the `auto` column instead of `1fr`. Fixed via `{sidePanel ?? <div />}` in app.tsx.

### Two-scope deletion infrastructure — [ADR 0006](../adr/0006-explicit-scope-deletion.md)

Five new / rewritten Rust commands in `src-tauri/src/commands.rs`:

- `source_deletion_preview(source_id) → { photos_total, orphan_photos, local_files, total_bytes, cloud_only }`
- `remove_photos_preview(photo_ids)`
- `remove_photos_from_catalog(photo_ids)` — transactional, DB-only, with FTS5 + sqlite-vec cleanup + thumbnail-cache file removal
- `recycle_source_copies(photo_ids)` — routes to Windows Recycle Bin via new `trash = "5.2.5"` dep
- `delete_source` — rewritten with `recycle_files` + `remove_orphan_photos` flags; transactional cascade via orphan detection (photos with no remaining source_copies after this source's rows are deleted)

FTS5 trigger fix: new migration `20260425000000_photos_fts_delete_trigger.sql` drops a dormant Phase-1 bug (`DELETE FROM photos_fts` on contentless FTS5, SQLite rejects). Application code now issues the proper FTS5 `'delete'` command BEFORE removing photo rows.

### Data cleanup

Migration `20260425000001_drop_mock_smart_albums.sql` removes 10 personalised placeholder albums ("Portraits", "Kids — Ari & Leo", "Japan · Autumn '25", "Milo (golden retriever)", etc.) that Phase-1 seeded with rule_json referencing tags/clusters the pipeline doesn't populate. Only the two actually-computed rule albums remain (`Night & Low Light`, `Out-of-focus`).

### Diagnostic tooling

- `src-tauri/tests/debug_import.rs` — tokio integration test that drives the real pipeline against `$CHRONIMAGE_DEBUG_IMPORT_SRC` with a throwaway DB, real bundled models. Opt-in via `-- --ignored`.
- `.claude/commands/debug-import.md` — skill definition with Loki LogQL queries for per-stage extraction.
- `scripts/debug-import.sh` — wrapper that runs the test and computes p50/p95 per sub-stage from Loki.
- Per-stage + per-photo `tracing` logs added throughout `src-tauri/src/import/pipeline.rs` (stage begin/end + `hash_ms` / `meta_ms` / `thumb_ms` / `nima_ms` / `siglip_ms`).

## Measured impact

```
Stage                              Before (O0)  After (O3 hot)  Speedup
1 — scan                                    1 ms            1 ms    —
2 — hash + EXIF + pHash + thumb           735 s           58 s    12.7×
3 — pairs                                   0 ms            0 ms    —
4 — NIMA + SigLIP                         780 s           95 s    8.3×
5 — RetinaFace + ArcFace                 3392 s           78 s   43.7×
────────────────────────────────────────────────────────────────────
Total                                    4908 s          230 s   21.3×
                                       (82 min)        (3.8 min)
```

- NIMA aesthetic scores: 0 / 111 → **111 / 111**
- SigLIP embeddings: 0 / 111 → **111 / 111**
- Face detections: 145 (unchanged — pipeline was reaching this stage before too; DML + O3 just made it 44× faster)

## Gates

- `cargo test --lib`: **262 passed** · 0 failed · 4 ignored
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `pnpm typecheck`: clean
- `pnpm exec vitest run`: **85 passed** / 85
- `pnpm exec biome check .`: 0 errors (20 pre-existing CSS `noDescendingSpecificity` warnings unchanged)

## Working tree (uncommitted)

50 files modified / added / deleted. Highlights:
- Added: `src-tauri/src/ai/providers.rs`, 4 new catalog sub-screens, `ConfirmDialog`, `state/import.ts`, `state/settings.ts`, 2 new migrations, `debug-import` test + skill + script, ADRs 0004 / 0005 / 0006.
- Deleted: `src/screens/OnboardScreen.tsx`, `src/screens/OnboardScreen.test.tsx`.
- Rewritten: `src-tauri/src/commands.rs` (+~840 LOC for deletion commands), `src-tauri/src/import/pipeline.rs` (instrumentation), `src/screens/catalog/CatalogScreen.tsx`, `CatalogSidePanel.tsx`, `SettingsScreen.tsx`, `app.tsx`, `state/ui.ts`, `src-tauri/Cargo.toml` (DML feature + 17 per-crate `opt-level = 3` overrides).
- Docs: ADRs 0004 / 0005 / 0006, `docs/prds/phase-2.md` "shipped early" section, this checkpoint, `docs/next-session.md`.

Suggested commit split (for when the squash happens):
1. `feat(ai): DirectML EP + NIMA NHWC fix + SigLIP output-name fallback + dev opt-level overrides` (ADR 0004)
2. `feat(catalog): remove onboarding wizard; catalog empty-state + global import store + sidepanel restructure` (ADR 0005)
3. `feat(catalog): two-scope deletion commands + trash crate + FTS5 delete trigger fix + mock-album cleanup` (ADR 0006)
4. `test(import): debug-import integration test + skill + runner script`
5. `docs: ADRs 0004–0006 + Phase-2 PRD update + checkpoint`

## Next session should focus on

Now that imports are usable, the original Phase 2 backlog resumes:

1. **DB migration + `cull/` Rust skeleton** — `pnpm migrate:new phase2_cull_export`, paste the Phase 2 DDL from `docs/prds/phase-2.md:115`, create `src-tauri/src/cull/verdict.rs` with `apply_verdict` + `#[tauri::command]`. Unblocks everything else.
2. **Port `screens_cull.jsx`** — `/port-screen cull` for the three-mode Cull screen (Compare / Grid / Swipe) with keyboard verdict stubs.
3. **Export engine skeleton** — `src-tauri/src/export/mod.rs` + tokio task pool + `mozjpeg` vendored dep + `export/jpeg.rs`. Safe to do in parallel with #2.
4. **Port `screens_cullbin.jsx`** — `/port-screen cullbin`.
5. **Scaffold 7 Phase 2 exit-criterion test files** — `#[ignore]`-gated stubs so CI can track them.

See `docs/next-session.md` for detail.

### Follow-ups flagged during this sprint (not blocking Phase 2 core)

- **Decode-once, infer-many.** Stages 2 + 4 + 5 each `image::open(path)` independently. A shared decoded `DynamicImage` per photo would cut another 2–3× off Stage 2/4. Noted in ADR 0004.
- **iPhone USB pairing UI** isn't reachable from the catalog screen anymore (Phase-1 flow was inline in `OnboardScreen`). Needs porting into `AddSourcePopover` or a Settings sub-panel.
- **Per-photo progress through stages 4 + 5** — the `IMPORT_PROGRESS_EVENT` stream currently only advances during Stage 2. UI shows `done == total` while AI work still runs.
- **GPU utilisation verification under load** — Loki confirms DML is *registered*; haven't proven we're GPU-bound vs decode-bound during Stage 4. A quick nvidia-smi capture during a debug-import run would settle it.
