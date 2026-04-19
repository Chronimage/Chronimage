-- Migration: photos_starred
-- Phase: 1 — Deep AI Catalog (unblocks Phase 1 §13 "unflagged favorites" rule)
--
-- Adds `is_starred` and `starred_at` to `photos`.
-- Resolves ADR 0001 Open issue: `is_starred` column absent from Phase 1 schema.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

ALTER TABLE photos ADD COLUMN is_starred INTEGER NOT NULL DEFAULT 0
    CHECK (is_starred IN (0, 1));

ALTER TABLE photos ADD COLUMN starred_at TEXT;   -- RFC3339 when starred, NULL otherwise

-- Partial index: only indexes the minority of starred photos, keeping scans fast.
CREATE INDEX IF NOT EXISTS idx_photos_is_starred ON photos(is_starred) WHERE is_starred = 1;
