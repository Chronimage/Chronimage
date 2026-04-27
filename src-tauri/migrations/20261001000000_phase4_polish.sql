-- Migration: phase4_polish
-- Phase: 4 — Prompt editing + polish
--
-- Adds:
--   1. `photos.color_label` — red/yellow/green/blue/purple/NULL
--      (optional XMP label).
--   2. `shortcuts` — user-overridable keyboard bindings.
--   3. `trips` + `trip_photos` — GPS cluster result cached on disk so
--      the map view doesn't recompute every open.
--
-- Forward-only. Do NOT edit once merged.

ALTER TABLE photos ADD COLUMN color_label TEXT;
CREATE INDEX IF NOT EXISTS idx_photos_color_label ON photos(color_label)
    WHERE color_label IS NOT NULL;

-- ── shortcuts ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS shortcuts (
  command_id   TEXT PRIMARY KEY,
  key_binding  TEXT NOT NULL,
  context      TEXT NOT NULL DEFAULT 'global',
  updated_at   TEXT NOT NULL
);

-- ── trips + trip_photos ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS trips (
  id              INTEGER PRIMARY KEY,
  name            TEXT,
  start_at        TEXT    NOT NULL,
  end_at          TEXT    NOT NULL,
  center_lat      REAL    NOT NULL,
  center_lng      REAL    NOT NULL,
  radius_km       REAL    NOT NULL,
  photo_count     INTEGER NOT NULL,
  auto_generated  INTEGER NOT NULL DEFAULT 1 CHECK (auto_generated IN (0, 1)),
  updated_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trips_start ON trips(start_at);

CREATE TABLE IF NOT EXISTS trip_photos (
  trip_id   INTEGER NOT NULL REFERENCES trips(id) ON DELETE CASCADE,
  photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  PRIMARY KEY (trip_id, photo_id)
);
