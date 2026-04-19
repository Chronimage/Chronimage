# Next session · Phase 1 week 2

Landed this session (2026-04-20): `ai/faces.rs`, `ai/cluster.rs`, extended `catalog/rules.rs` + ADR 0001, `lift_and_shift` module + Tauri commands, `is_starred` column (migration `20260422`), wired `chronimage-cli migrate`, dev DB recreated. 152 Rust tests green, clippy + typecheck clean.

## Starts (in order)

### 1. Frontend wiring for lift & shift · 45 min
First file: [src/tauri/invoke.ts](src/tauri/invoke.ts). Add `liftShiftDryRun()` + `liftShiftExecute(planId, confirmToken)` mirroring the existing `cleanupDryRun`/`cleanupExecute` typed wrappers, then a `useLiftShiftDryRun` mutation hook in `src/state/queries.ts`, then wire the "Lift & shift" step in `src/screens/OnboardScreen.tsx`.

Why highest leverage: Rust side landed + tested end-to-end this session with the manifest writer. Only missing piece is the TS wrappers; without them the PRD §2 onboarding flow can't complete. ~45 min because the cleanup wiring is a line-for-line template.

### 2. Rediscovery rows on Catalog home · 60 min
First file: `src-tauri/src/catalog/seed.rs` (or a new `src-tauri/src/rediscovery.rs`). Define 4 rule_json expressions per PRD §13 and seed them as system albums:
- "On this day" — `CapturedAt { op: "on_mmdd", value: today }` (needs one new rule variant: `CapturedAt.on_mmdd`)
- "Unseen in 2 years" — needs `last_viewed_at` column; if missing, blocker-note in ADR 0001 and skip
- "First time on new camera" — `All [CapturedAt.between, Camera.model_eq]`
- "Unflagged favorites" — `All [Quality.aesthetic gte 8.0, Not Starred]` ← now unblocked

Surface as horizontal rows on `CatalogScreen` when no query is active. Frontend-only once rule JSON seeded.

Why: the rule engine work this session was the single biggest unblocker for §13. Cashing it in now delivers user-visible value on the Catalog home — otherwise the extensions sit unused for another cycle.

### 3. Smart album re-evaluator (background) · 45 min
First file: `src-tauri/src/albums/reevaluator.rs` (new) + register a tokio task in [src-tauri/src/main.rs](src-tauri/src/main.rs).

PRD §7 asks for: re-run `count_matching` + `matching_photo_ids` every 10 min on new photos, nightly full rebuild. Use `tokio::time::interval(Duration::from_secs(600))`; write results back to `smart_albums.cover_photo_ids_json` + a `photo_count` column (add migration `20260423000000_smart_albums_photo_count.sql` if not present).

Why: the rule engine returns empty rows in the UI until this runs. Small file, big perceived completeness gain.

### 4. `ai/caption.rs` llama.cpp sidecar scaffold · 60 min
First file: `src-tauri/src/ai/caption.rs` (new). Spawn `ai-wrangler` (sonnet). Follow the `siglip.rs` stub/real pattern: `CaptionSession::load_or_stub`, `caption_image(path) -> String` returning `"(stub caption)"` when no sidecar. Wire to the existing `src-tauri/src/sidecar/` directory (already exists). GPU-only real path gated behind `ai::budget::detect().tier == Tier::Gpu`.

Why: Only remaining unscaffolded PRD §5 item. Low risk, high symbolic completeness — leaves faces + cluster + caption + embed + aesthetic all scaffolded.

### 5. `biome check --write --unsafe` cleanup · 10 min
Fix the 4 `useSortedClasses` warnings in `src/screens/OnboardScreen.tsx` (lines 211, 236, 1179). Non-blocking but CI-adjacent. Tack onto end of session.

## Deferred

- PeopleScreen / face clustering UI — blocked on real ArcFace inference (model download flow)
- Replacing `todo!()` in faces/cluster real paths — blocked on model download
- Settings screen
- `last_viewed_at` + `photo_album_membership` schema (Phase 2 scope per ADR 0001)
