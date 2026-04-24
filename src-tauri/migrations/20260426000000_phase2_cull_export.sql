-- Migration: phase2_cull_export
-- Phase: 2 — Cull + Cull Bin + Export
--
-- Adds:
--   1. `photos.star_rating` (0..5) for the detail-view Rate action (phase-2 §1)
--   2. `photos.is_flagged` shortcut (phase-2 §2 Flag button from detail view)
--   3. `cull_bin` — rejected photos pending permanent delete (phase-2 §3/§4)
--   4. `export_jobs` + `export_job_items` — persisted export queue (phase-2 §5/§6)
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

-- ── photos: star_rating + is_flagged ────────────────────────────────────────

ALTER TABLE photos ADD COLUMN star_rating INTEGER NOT NULL DEFAULT 0
    CHECK (star_rating BETWEEN 0 AND 5);

ALTER TABLE photos ADD COLUMN is_flagged INTEGER NOT NULL DEFAULT 0
    CHECK (is_flagged IN (0, 1));

ALTER TABLE photos ADD COLUMN flagged_at TEXT;   -- RFC3339 when flagged, NULL otherwise

-- Partial indexes: scans over the (rare) non-zero rows only.
CREATE INDEX IF NOT EXISTS idx_photos_star_rating
    ON photos(star_rating) WHERE star_rating > 0;
CREATE INDEX IF NOT EXISTS idx_photos_is_flagged
    ON photos(is_flagged) WHERE is_flagged = 1;

-- ── cull_bin ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS cull_bin (
  photo_id                   INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  rejected_at                TEXT    NOT NULL,  -- RFC3339
  reason                     TEXT    NOT NULL,  -- 'near_dup' | 'blur' | 'eyes_closed' | 'exposure' | 'user' | 'flag' | 'duplicate' | 'other'
  source_copies_frozen_json  TEXT    NOT NULL CHECK (json_valid(source_copies_frozen_json)),
  retention_days             INTEGER NOT NULL DEFAULT 30 CHECK (retention_days > 0),
  permanent_delete_after     TEXT    NOT NULL  -- denormalised for the daily sweep
);

CREATE INDEX IF NOT EXISTS idx_cull_bin_permanent_delete
    ON cull_bin(permanent_delete_after);
CREATE INDEX IF NOT EXISTS idx_cull_bin_rejected_at
    ON cull_bin(rejected_at);
CREATE INDEX IF NOT EXISTS idx_cull_bin_reason
    ON cull_bin(reason);

-- ── export_jobs + export_job_items ──────────────────────────────────────────

CREATE TABLE IF NOT EXISTS export_jobs (
  id              INTEGER PRIMARY KEY,
  created_at      TEXT NOT NULL,
  preset_json     TEXT NOT NULL CHECK (json_valid(preset_json)),
  total_photos    INTEGER NOT NULL,
  done_count      INTEGER NOT NULL DEFAULT 0,
  error_count     INTEGER NOT NULL DEFAULT 0,
  status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued','running','paused','done','cancelled','error')),
  output_dir      TEXT NOT NULL,
  upload_targets  TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(upload_targets)),
  finished_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_export_jobs_status ON export_jobs(status);
CREATE INDEX IF NOT EXISTS idx_export_jobs_created ON export_jobs(created_at);

CREATE TABLE IF NOT EXISTS export_job_items (
  id           INTEGER PRIMARY KEY,
  job_id       INTEGER NOT NULL REFERENCES export_jobs(id) ON DELETE CASCADE,
  photo_id     INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  status       TEXT NOT NULL DEFAULT 'queued'
                 CHECK (status IN ('queued','running','done','error')),
  output_path  TEXT,
  error_msg    TEXT,
  started_at   TEXT,
  finished_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_export_job_items_job    ON export_job_items(job_id);
CREATE INDEX IF NOT EXISTS idx_export_job_items_status ON export_job_items(status);

-- Bump schema_version to reflect phase-2 tables.
INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '3', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
