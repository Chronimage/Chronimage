-- Migration: lightroom_parity_foundation
-- Phase: post-v1 — Lightroom parity foundation
--
-- Adds persistent local-adjustment mask layers, AI edit artifact tracking,
-- merge jobs, and watched-folder tether sources.
-- Forward-only. Do NOT edit once merged.

-- ── Develop masks ───────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS develop_masks (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  edit_id         INTEGER REFERENCES edits(id) ON DELETE CASCADE,
  name            TEXT    NOT NULL,
  source          TEXT    NOT NULL CHECK (source IN (
                    'brush',
                    'linear_gradient',
                    'radial_gradient',
                    'prompt',
                    'subject',
                    'sky',
                    'background',
                    'foreground',
                    'object',
                    'person',
                    'landscape',
                    'color_range',
                    'luminance_range',
                    'depth_range'
                  )),
  mode            TEXT    NOT NULL DEFAULT 'normal' CHECK (mode IN (
                    'normal',
                    'add',
                    'subtract',
                    'intersect'
                  )),
  visible         INTEGER NOT NULL DEFAULT 1 CHECK (visible IN (0, 1)),
  order_index     INTEGER NOT NULL,
  payload_storage TEXT    NOT NULL DEFAULT 'inline' CHECK (payload_storage IN ('inline', 'file')),
  mask_payload    TEXT    NOT NULL CHECK (json_valid(mask_payload)),
  operations_json TEXT    NOT NULL CHECK (json_valid(operations_json)),
  confidence      REAL    CHECK (confidence IS NULL OR confidence BETWEEN 0 AND 1),
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_develop_masks_photo_order ON develop_masks(photo_id, order_index, id);
CREATE INDEX IF NOT EXISTS idx_develop_masks_edit ON develop_masks(edit_id);
CREATE INDEX IF NOT EXISTS idx_develop_masks_visible ON develop_masks(photo_id, visible);

-- ── AI edit artifact tracking ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS ai_edits (
  id                INTEGER PRIMARY KEY,
  photo_id          INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  feature           TEXT    NOT NULL,
  model_id          TEXT    NOT NULL,
  source_edit_hash  TEXT    NOT NULL,
  output_path       TEXT,
  output_b64        TEXT,
  state             TEXT    NOT NULL DEFAULT 'current' CHECK (state IN (
                      'current',
                      'stale',
                      'running',
                      'failed'
                    )),
  params_json       TEXT    NOT NULL CHECK (json_valid(params_json)),
  error             TEXT,
  created_at        TEXT    NOT NULL,
  updated_at        TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_edits_photo_feature ON ai_edits(photo_id, feature);
CREATE INDEX IF NOT EXISTS idx_ai_edits_state ON ai_edits(state);
CREATE INDEX IF NOT EXISTS idx_ai_edits_hash ON ai_edits(source_edit_hash);

-- ── Merge jobs + tether sources ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS merge_jobs (
  id                INTEGER PRIMARY KEY,
  kind              TEXT    NOT NULL CHECK (kind IN ('hdr', 'panorama')),
  photo_ids_json    TEXT    NOT NULL CHECK (json_valid(photo_ids_json)),
  options_json      TEXT    NOT NULL CHECK (json_valid(options_json)),
  state             TEXT    NOT NULL DEFAULT 'queued' CHECK (state IN (
                      'queued',
                      'running',
                      'completed',
                      'failed',
                      'cancelled'
                    )),
  output_photo_id   INTEGER REFERENCES photos(id) ON DELETE SET NULL,
  error             TEXT,
  created_at        TEXT    NOT NULL,
  updated_at        TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_merge_jobs_state ON merge_jobs(state, created_at);
CREATE INDEX IF NOT EXISTS idx_merge_jobs_output ON merge_jobs(output_photo_id);

CREATE TABLE IF NOT EXISTS tether_sources (
  id           INTEGER PRIMARY KEY,
  name         TEXT    NOT NULL,
  folder_path  TEXT    NOT NULL UNIQUE,
  vendor       TEXT,
  enabled      INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  options_json TEXT    NOT NULL CHECK (json_valid(options_json)),
  created_at   TEXT    NOT NULL,
  updated_at   TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tether_sources_enabled ON tether_sources(enabled);
