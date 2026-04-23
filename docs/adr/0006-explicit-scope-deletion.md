# ADR 0006 — Two-scope deletion: "remove from catalog" vs "move to Recycle Bin"

**Status:** Accepted
**Date:** 2026-04-23
**Phase:** Phase 2 week 1 (UX hotfix pulled ahead of Cull+Export work)

## Context

Phase 1 shipped with three deletion gaps that blocked real-world use:

1. **`delete_source` was half-working.** It ran the SQL deletes ([commands.rs:802-817](../../src-tauri/src/commands.rs)) but the TanStack Query wrapper [`useDeleteSource`](../../src/state/queries.ts) didn't invalidate the `['photos']` query. The UI appeared frozen after a successful delete — stale photos remained visible until manual refresh.
2. **No bulk-delete for photos.** The catalog multi-select toolbar had stub buttons for Develop / Cull / Export / Tag but no Remove. There was no Rust command `remove_photo(s)` to call.
3. **No Recycle-Bin path.** Every `std::fs::remove_file` in the codebase was a permanent delete; the `trash` crate wasn't a dependency. Users had no way to "free up the SD card" without committing to a destroy-forever action.

A fourth, pre-existing bug surfaced during the fix: the `photos_fts_delete` trigger in [migration 20260420000000_phase1_catalog.sql:169](../../src-tauri/migrations/20260420000000_phase1_catalog.sql) used `DELETE FROM photos_fts WHERE rowid = old.id`, which SQLite rejects on contentless FTS5 tables (`cannot DELETE from contentless fts5 table`). Migration 20260424000000 repaired the tags_fts_* and photos_fts_update triggers but missed this one. Any `DELETE FROM photos` would fail on a fresh schema.

## Decision

Ship a complete deletion story with **explicit user-visible scope** at every decision point. Two scopes, never implicit:

- **Remove from catalog** — SQL delete only. Originals stay on disk.
- **Also move files to Recycle Bin** — opt-in checkbox. Routes through the `trash` crate; recoverable from Windows' bin for the standard retention period.

Cloud-only photos (no local `source_copies.path`) never appear in the recycle scope. The modal shows real byte/file counts from a dry-run call so the user sees exactly what they're about to do.

### Backend

Four new commands in [commands.rs](../../src-tauri/src/commands.rs) + the `trash = "5"` dep in [Cargo.toml](../../src-tauri/Cargo.toml).

Each command has a tested pool-accepting `_impl` helper and a thin `#[tauri::command]` wrapper (matches the existing `execute_cleanup_plan` pattern) so integration tests can hit the core logic without an AppState / Tauri runtime.

| Command | Input | Returns | Notes |
|---|---|---|---|
| `source_deletion_preview(source_id)` | `i64` | `SourceDeletionPlan { photos_total, orphan_photos, local_files, total_bytes, cloud_only }` | Dry-run counts for the source-disconnect modal. |
| `delete_source(source_id, recycle_files, remove_orphan_photos)` | `i64, bool, bool` | `RemoveReceipt { removed_photos, removed_thumbnails, errors }` | Transactional. Collects orphan paths + metadata BEFORE mutating; commits DB changes; THEN best-effort recycles files (trash failure cannot roll back the catalog). |
| `remove_photos_preview(photo_ids)` | `Vec<i64>` | `RemovePreview { photo_count, local_files, cloud_only_photos, total_bytes }` | Dry-run for the bulk-remove modal. |
| `remove_photos_from_catalog(photo_ids)` | `Vec<i64>` | `RemoveReceipt` | Transactional. Deletes `photos` (FK cascade handles `source_copies`, `faces`, `tags`, `photo_embeddings`, `photo_views`). Explicitly deletes from `vec_photo_embeddings` + `vec_photo_embeddings_int8` virtual tables (FK cascade does NOT cover sqlite-vec). Best-effort nukes `{thumbnails_dir}/{sha256}_320.jpg`. |
| `recycle_source_copies(photo_ids)` | `Vec<i64>` | `RecycleReceipt { recycled_count, skipped_count, errors }` | File-only. Does NOT touch DB rows. Caller decides whether to follow with `remove_photos_from_catalog`. |

### FTS5 delete trigger fix

New migration [20260425000000_photos_fts_delete_trigger.sql](../../src-tauri/migrations/20260425000000_photos_fts_delete_trigger.sql) drops the broken trigger. Application handlers now issue the correct contentless-FTS5 delete command *before* deleting the photo row:

```sql
INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
SELECT 'delete', p.id, p.filename,
       COALESCE((SELECT group_concat(label, ' ') FROM tags WHERE photo_id = p.id), '')
FROM photos p WHERE p.id = ?1
```

Running this while the photo + tags still exist gives FTS5 the exact pre-delete values it needs for correct term-statistics bookkeeping. Downstream FK cascade then deletes tags; the tags_fts_* triggers from migration 20260424 fire but their no-op-if-missing behaviour means the already-deleted FTS row stays gone.

### Mock smart-album cleanup

Spotted while auditing the catalog screen: the seeded `smart_albums` table contained ten personalised placeholders from the design mock (`Kids — Ari & Leo`, `Milo (golden retriever)`, `Japan · Autumn '25`, `Food & Kitchen`, `Weddings & Events`, `Portraits`, `Golden Hour`, `Loop 2 · product shots`, `Screenshots & Docs`, `Burst & Duplicates`). None had rules that mapped to anything real users would generate.

- [`catalog::seed::SYSTEM_ALBUMS`](../../src-tauri/src/catalog/seed.rs) trimmed from 12 personalised entries to 2 rule-based ones:
  - `Night & Low Light` (`{"type":"exif","field":"iso","op":"gte","value":3200}`)
  - `Out-of-focus` (`{"type":"quality","field":"sharpness","op":"lt","value":0.3}`)
- New migration [20260425000001_drop_mock_smart_albums.sql](../../src-tauri/migrations/20260425000001_drop_mock_smart_albums.sql) deletes the ten mock albums from existing databases — gated on `is_system = 1` so user-created albums with the same names are untouched.

### Frontend

- New primitive [`ConfirmDialog`](../../src/primitives/ConfirmDialog.tsx) — reusable modal with title, description, optional checkbox rows, primary/cancel, `danger` tone, ESC-to-cancel, busy-state suppression of both backdrop-click and ESC.
- [`SourcesPanel`](../../src/screens/catalog/SourcesPanel.tsx) — click a source row → `DisconnectSourceModal` that reads `useSourceDeletionPreview(source_id)` and surfaces the orphan/recycle checkboxes only when applicable (0 orphans = no checkbox offered).
- [`CatalogScreen`](../../src/screens/catalog/CatalogScreen.tsx) — Remove button in the multi-select toolbar opens a `ConfirmDialog` with the two-scope choice. Preview call is keyed on the sorted selection so identical selections hit cache.
- All mutations now invalidate `['photos']`, `['sources']`, `['cleanup']`, `['albums']`, `['imports']`, `['duplicates']` as appropriate (was missing on the old `useDeleteSource`).

## Invariants

1. **Catalog mutation is transactional**; file-system side effects are not. If `trash::delete(path)` fails, the DB is still consistent — the file stays where it is and the error surfaces in `RemoveReceipt.errors`. Never the other way round.
2. **Cloud-only photos never appear in the recycle scope.** The `local_files` count in every preview excludes rows where `source_copies.path IS NULL`. The checkbox label explicitly says `N local files` — ambient cloud-only entries get a separate "N cloud-only (no file to recycle)" note.
3. **Thumb cache removal is best-effort.** A missing or locked cache file is logged to `RemoveReceipt.errors` but doesn't fail the mutation. The worst case is a dead cache entry that gets overwritten on next import.
4. **`recycle_source_copies` is DB-silent.** Users can recycle files without touching the catalog (rare, but the flow is clean). A subsequent call to `remove_photos_from_catalog` can then drop the DB rows. In the standard "Remove N photos" modal we always pair them.

## Tests

Eight new Rust tests in [`commands.rs`](../../src-tauri/src/commands.rs) (`#[tokio::test]` with the existing `test_pool()` helper):

- `source_deletion_preview_counts_orphans_and_shared`
- `delete_source_cascades_orphan_photos_and_keeps_shared`
- `delete_source_without_orphan_cleanup_leaves_orphan_photos`
- `remove_photos_preview_splits_local_and_cloud_only`
- `remove_photos_preview_empty_input_returns_zeros`
- `remove_photos_from_catalog_deletes_rows_and_source_copies`
- `recycle_source_copies_sends_local_files_to_trash_and_reports`
- `recycle_source_copies_skips_missing_and_empty_input`

`recycle_source_copies_sends_local_files_to_trash_and_reports` uses `tempfile::TempDir` — assertions check files are gone from the source dir AND DB rows are untouched. The exact destination (Windows Recycle Bin, `$HOME/.local/share/Trash` on Linux) is abstracted by the `trash` crate.

6 frontend tests in [`ConfirmDialog.test.tsx`](../../src/primitives/ConfirmDialog.test.tsx).

## Alternatives considered

1. **Single "Delete" action with a hidden "also remove files" setting.** Rejected — the whole point is that users don't build a mental model of hidden preferences. Every delete decision should be made at the moment of action with visible numbers.
2. **Recycle as a separate menu item (not an opt-in checkbox in the same flow).** Rejected — users who want to free disk space would have to do two actions, and "catalog-only remove" + "file recycle" on the same photo would race. The single modal with a checkbox is both simpler and atomic.
3. **Add `import_mode` to `sources` table.** Partially rejected — not in this ADR; see ADR 0005 §"No per-source mode persistence". The deletion flow is mode-agnostic.
4. **Use `tauri-plugin-fs` for the recycle call.** Rejected — that plugin is a general fs API; `trash` is purpose-built and routes through `IFileOperation` on Windows for correct Recycle Bin behaviour. Smaller surface, right tool.

## Follow-ups

- **Undo affordance for catalog-only removes.** Recycled files are recoverable from the OS bin; catalog-only removes are not. A 10-second undo snackbar in the UI is cheap and valuable. Not yet shipped.
- **Bulk-restore from Recycle Bin** as a first-class action — today users go to Explorer. Worth a cost/value review in Phase 2 alongside the Cull Bin work (the 30-day Cull Bin is a different mechanism; this is about OS-level trash).
- **Wire a similar two-scope confirm to the Google Photos cleanup path** (it currently prints manual instructions; could offer "mark for deletion in the next sync" once the API supports it).

## References

- [`src-tauri/src/commands.rs`](../../src-tauri/src/commands.rs) — the four new commands + tests
- [`src-tauri/migrations/20260425000000_photos_fts_delete_trigger.sql`](../../src-tauri/migrations/20260425000000_photos_fts_delete_trigger.sql) — FTS5 fix
- [`src-tauri/migrations/20260425000001_drop_mock_smart_albums.sql`](../../src-tauri/migrations/20260425000001_drop_mock_smart_albums.sql) — mock-album cleanup
- [`src-tauri/src/catalog/seed.rs`](../../src-tauri/src/catalog/seed.rs) — trimmed SYSTEM_ALBUMS
- [`src/primitives/ConfirmDialog.tsx`](../../src/primitives/ConfirmDialog.tsx)
- [`src/screens/catalog/SourcesPanel.tsx`](../../src/screens/catalog/SourcesPanel.tsx) — `DisconnectSourceModal`
- [`src/screens/catalog/CatalogScreen.tsx`](../../src/screens/catalog/CatalogScreen.tsx) — multi-select Remove flow
