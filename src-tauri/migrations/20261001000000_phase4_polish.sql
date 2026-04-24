-- Migration: phase4_polish
-- Phase: 4 — Prompt editing + polish
--
-- Adds:
--   1. `photos.rating` (0..=5) — Lightroom-style integer rating. Distinct
--      from `star_rating` added in Phase 2 so XMP round-tripping stays
--      clean (xmp:Rating maps to this column; the Phase 2 Rate button
--      keeps using star_rating to avoid breaking existing UI).
--   2. `photos.color_label` — red/yellow/green/blue/purple/NULL
--      (optional XMP label).
--   3. `shortcuts` — user-overridable keyboard bindings.
--   4. `trips` + `trip_photos` — GPS cluster result cached on disk so
--      the map view doesn't recompute every open.
--
-- Forward-only. Do NOT edit once merged.

ALTER TABLE photos ADD COLUMN rating INTEGER NOT NULL DEFAULT 0
    CHECK (rating BETWEEN 0 AND 5);
ALTER TABLE photos ADD COLUMN color_label TEXT;
CREATE INDEX IF NOT EXISTS idx_photos_rating ON photos(rating)
    WHERE rating > 0;
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

-- Bump schema_version.
INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '5', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
