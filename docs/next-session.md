# Next session · Phase 1 — closing the remaining exit tests

Branch: `develop` (post PR #35 + #36). All four NFR-bearing Rust integration tests are green; CI model packaging, Google Photos OAuth scaffolding, and the `phase-1-import-throughput` e2e skeleton all landed. What's left is the last Rust stress stub, three e2e fixtures, and finishing the Google Photos connector + verifying the beta release pipeline.

## Scoreboard on develop (2026-04-21, post-PR #36)

| Test | Status |
|---|---|
| `phase_1_raw_jpg_pair` | ✅ 1.0000 precision (5 000 pairs) |
| `phase_1_catalog_size` | ✅ 0.00202 ratio |
| `phase_1_face_clustering` | ✅ F1 1.0000 (LFW) |
| `phase_1_search_latency` | ✅ 607 ms p95 (int8 vec0 KNN) |
| `phase_1_stress` | ❌ `unimplemented!()` |
| `phase-1-import-throughput.spec.ts` | 🟡 wired, skips unless `CHRONIMAGE_E2E_TAURI=1` |
| `phase-1-source-cleanup.spec.ts` | ❌ top-level `test.skip()` |
| `phase-1-rediscovery.spec.ts` | ❌ top-level `test.skip()` |
| `phase-1-search-latency.spec.ts` | ❌ top-level `test.skip()` |

## Five starts — pick one

### 1. `phase_1_stress` 8-hour nightly loop · ~3 h
Last Rust integration test stub. `#[ignore]`-gated → zero CI cost.
**First action:** edit [phase_1_stress.rs:8](src-tauri/tests/phase_1_stress.rs#L8). Reuse `synthesize_jpeg` from [util/synthetic.rs](src-tauri/src/util/synthetic.rs) (added in PR #36) + `run_pipeline_headless`. Interleave search queries + face cluster rebuilds; RSS sample via `sysinfo`. Assert no panic hook fired, peak RSS < 2 GB.

### 2. Cut first beta tag + verify CI packaging · ~45 min
PR #36 added `Fetch bundled models` to release workflows but the step hasn't been exercised yet. Ship a `v0.1.0-beta.1` tag from `develop` and watch the `release-beta` workflow produce a signed `.msi` — this is the cheapest way to confirm the packaging step works on a cold Windows runner before it matters.
**First action:** `git tag -s v0.1.0-beta.1 -m 'first beta smoke'` → `git push origin v0.1.0-beta.1` → watch `gh run watch` on the `release-beta` workflow → confirm MSI has non-zero `models/bundled/` resource and installs.

### 3. Remaining three e2e specs · ~4 h bundled
[phase-1-source-cleanup.spec.ts](tests/e2e/phase-1-source-cleanup.spec.ts), [phase-1-rediscovery.spec.ts](tests/e2e/phase-1-rediscovery.spec.ts), [phase-1-search-latency.spec.ts](tests/e2e/phase-1-search-latency.spec.ts) are all `test.skip()` TODOs. Mirror the pattern PR #36 established for import-throughput: un-skip under `CHRONIMAGE_E2E_TAURI=1`, drive via `page.evaluate(() => __TAURI__.core.invoke(...))`.
**First action:** add `__test_seed_catalog(rows)` + `__test_seed_source_copies(tmpdir)` debug-only Tauri commands next to `__test_generate_fixture` in [commands.rs:1043](src-tauri/src/commands.rs#L1043). Each seeder backfills one e2e's prerequisites; the specs become `describe > beforeAll(seed) > test(assert)`.

### 4. Google Photos: loopback HTTP listener + first `mediaItems.list` · ~4 h
PR #36 landed the OAuth PKCE + keyring store; frontend hands off to the system browser but the redirect has nowhere to land. Add a tiny loopback server that binds `127.0.0.1:<ephemeral>`, captures the `?code=&state=` query, and returns a "you can close this tab" HTML page. Then `user_initiated_list_media_items(page_size, page_token)` behind the existing access-token refresh logic.
**First action:** extend [src-tauri/src/sources/google_photos.rs](src-tauri/src/sources/google_photos.rs) with a `spawn_redirect_listener()` helper (tokio + `hyper`) that returns `(Url, oneshot::Receiver<(code, state)>)`. Thread it through [gphotos_start_oauth:992](src-tauri/src/commands.rs#L992).

### 5. sqlite-vec 0.1.10+ diskann spike (Phase 2 prep) · ~90 min research
`docs/prds/phase-2.md` § 9 carries this. Confirm 0.1.10+ ships diskann on Windows before committing to Phase 2 scope.
**First action:** `cargo update -p sqlite-vec --precise <latest>` in a scratch worktree, add a `vec_photo_embeddings_diskann` table, re-run `phase_1_search_latency`, capture p95 + index-build time in a new ADR under [docs/adr/](docs/adr/).

## Recommended order

**#1** is safe high-leverage code (last Rust stub). **#2** is a 45-minute verification pass with real-world consequences if the packaging step is subtly broken — do it next if #36's CI confidence matters. **#3** + **#4** are roughly equal-weight; pick by where the UX pressure is. Defer **#5** until Phase 1 fully exits.
