-- Migration: phase3_develop
-- Phase: 3 — RAW Develop
--
-- Adds:
--   1. `edits` — non-destructive edit history. Each save appends one row
--      with `parent_edit_id` forming a DAG. Undo walks to parent; redo
--      walks to the child by `saved_at` ASC.
--   2. `presets` — user-saved presets (built-ins live in code).
--   3. `photos.current_edit_id` — per-photo pointer to the latest
--      committed edit. Kept denormalised so the develop screen opens
--      without a tree walk.
--
-- Forward-only. Do NOT edit once merged.

-- ── edits ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS edits (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  parent_edit_id  INTEGER REFERENCES edits(id) ON DELETE SET NULL,
  operations_json TEXT    NOT NULL CHECK (json_valid(operations_json)),
  saved_at        TEXT    NOT NULL,
  is_snapshot     INTEGER NOT NULL DEFAULT 0 CHECK (is_snapshot IN (0, 1)),
  label           TEXT
);

CREATE INDEX IF NOT EXISTS idx_edits_photo  ON edits(photo_id);
CREATE INDEX IF NOT EXISTS idx_edits_parent ON edits(parent_edit_id);
CREATE INDEX IF NOT EXISTS idx_edits_saved  ON edits(saved_at);

-- ── presets ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS presets (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL UNIQUE,
  group_name      TEXT    NOT NULL,       -- 'Face' | 'Scene' | 'Quality' | 'Style'
  description     TEXT,
  operations_json TEXT    NOT NULL CHECK (json_valid(operations_json)),
  is_system       INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL,
  scope           TEXT    NOT NULL DEFAULT 'global' CHECK (scope IN ('global', 'mask')),
  mask_source     TEXT,
  mask_options_json TEXT CHECK (mask_options_json IS NULL OR json_valid(mask_options_json)),
  local_operations_json TEXT CHECK (local_operations_json IS NULL OR json_valid(local_operations_json)),
  fallback_operations_json TEXT CHECK (fallback_operations_json IS NULL OR json_valid(fallback_operations_json)),
  confidence_threshold REAL CHECK (confidence_threshold IS NULL OR confidence_threshold BETWEEN 0 AND 1)
);

CREATE INDEX IF NOT EXISTS idx_presets_group ON presets(group_name);
CREATE INDEX IF NOT EXISTS idx_presets_scope ON presets(scope);

-- ── photos.current_edit_id ────────────────────────────────────────────────

ALTER TABLE photos ADD COLUMN current_edit_id INTEGER
    REFERENCES edits(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_photos_current_edit ON photos(current_edit_id);
