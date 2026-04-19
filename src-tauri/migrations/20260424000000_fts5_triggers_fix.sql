-- Migration: fts5_triggers_fix
-- Phase: 1 — Deep AI Catalog (bug fix)
--
-- Bug A: `tags_fts_insert` and `tags_fts_delete` (created in
-- 20260420000000_phase1_catalog.sql) use `UPDATE photos_fts SET tags = …`
-- on a contentless FTS5 table (content=''). SQLite rejects UPDATE on column
-- values of contentless tables:
--   "cannot UPDATE contentless fts5 table: photos_fts"
--
-- Bug B: A naive fix using `DELETE FROM photos_fts WHERE rowid = …` also
-- fails on contentless tables:
--   "cannot DELETE from contentless fts5 table: photos_fts"
--
-- Fix: contentless FTS5 only supports two DML operations:
--   1. INSERT INTO fts(rowid, col1, col2) VALUES(…)   — add a row
--   2. INSERT INTO fts(fts, rowid, col1, col2) VALUES('delete', …)  — remove a row
--      (REQUIRES the EXACT original column values to update term statistics)
--
-- For `tags_fts_insert` (AFTER INSERT): the old tags concatenation is computed
-- as group_concat EXCLUDING the newly-inserted row (new.id is available in the
-- AFTER INSERT trigger context).
--
-- For `tags_fts_delete`: we switch to a BEFORE DELETE trigger so the row being
-- deleted is still present when we capture the current tags concatenation.
-- The reinsert then fires AFTER DELETE on a separate AFTER trigger.
--
-- Also replace `photos_fts_update` which used the same rejected UPDATE pattern.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

-- Drop all broken triggers.
DROP TRIGGER IF EXISTS tags_fts_insert;
DROP TRIGGER IF EXISTS tags_fts_delete;
DROP TRIGGER IF EXISTS photos_fts_update;

-- ── tags_fts_insert ───────────────────────────────────────────────────────────
-- AFTER INSERT ON tags: remove the stale FTS row (using exact old tags value —
-- all tags EXCEPT the newly inserted one) then reinsert with full updated tags.
CREATE TRIGGER tags_fts_insert AFTER INSERT ON tags BEGIN
  -- Issue the FTS5 'delete' command with the pre-insert tags concatenation.
  -- We exclude the just-inserted tag (id = new.id) to obtain the exact old value.
  INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
    SELECT 'delete', p.id, p.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags
              WHERE photo_id = new.photo_id AND id != new.id),
             ''
           )
    FROM photos p WHERE p.id = new.photo_id;
  -- Reinsert with the new full tags concatenation (including the new tag).
  INSERT INTO photos_fts(rowid, filename, tags)
    SELECT p.id, p.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags WHERE photo_id = p.id),
             ''
           )
    FROM photos p WHERE p.id = new.photo_id;
END;

-- ── tags_fts_delete (part 1 — BEFORE) ────────────────────────────────────────
-- Capture the current tags (including the row being deleted) and issue the
-- FTS5 'delete' command BEFORE the row is removed from the tags table.
-- This ensures we use the exact old value for correct term-statistics bookkeeping.
CREATE TRIGGER tags_fts_before_delete BEFORE DELETE ON tags BEGIN
  INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
    SELECT 'delete', p.id, p.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags WHERE photo_id = old.photo_id),
             ''
           )
    FROM photos p WHERE p.id = old.photo_id;
END;

-- ── tags_fts_delete (part 2 — AFTER) ─────────────────────────────────────────
-- After the row is gone, reinsert the FTS row with the remaining tags.
CREATE TRIGGER tags_fts_after_delete AFTER DELETE ON tags BEGIN
  INSERT INTO photos_fts(rowid, filename, tags)
    SELECT p.id, p.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags WHERE photo_id = p.id),
             ''
           )
    FROM photos p WHERE p.id = old.photo_id;
END;

-- ── photos_fts_update ─────────────────────────────────────────────────────────
-- On filename change: issue 'delete' with old filename, reinsert with new.
CREATE TRIGGER photos_fts_update AFTER UPDATE OF filename ON photos BEGIN
  INSERT INTO photos_fts(photos_fts, rowid, filename, tags)
    SELECT 'delete', old.id, old.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags WHERE photo_id = old.id),
             ''
           );
  INSERT INTO photos_fts(rowid, filename, tags)
    SELECT new.id, new.filename,
           COALESCE(
             (SELECT group_concat(label, ' ')
              FROM tags WHERE photo_id = new.id),
             ''
           );
END;
