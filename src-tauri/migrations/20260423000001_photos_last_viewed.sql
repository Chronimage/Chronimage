-- Migration: photos_last_viewed
-- Phase: 1 — Deep AI Catalog (unblocks Phase 1 §13 "Unseen in 2 years" rule)
--
-- Adds `last_viewed_at` directly to `photos` for rule-engine access without
-- a subquery join. The existing `photo_views` table remains the authoritative
-- write surface; a trigger keeps `photos.last_viewed_at` in sync.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

ALTER TABLE photos ADD COLUMN last_viewed_at TEXT;   -- RFC3339 when last viewed, NULL = never viewed

-- Partial index: only indexes photos that HAVE been viewed (the non-null minority initially).
-- Used by the LastViewed rule: WHERE last_viewed_at IS NULL OR last_viewed_at < datetime(...)
CREATE INDEX IF NOT EXISTS idx_photos_last_viewed ON photos(last_viewed_at)
    WHERE last_viewed_at IS NOT NULL;

-- Keep photos.last_viewed_at in sync when photo_views is written.
CREATE TRIGGER IF NOT EXISTS photo_views_sync_insert
AFTER INSERT ON photo_views
BEGIN
    UPDATE photos SET last_viewed_at = new.last_viewed_at WHERE id = new.photo_id;
END;

CREATE TRIGGER IF NOT EXISTS photo_views_sync_update
AFTER UPDATE OF last_viewed_at ON photo_views
BEGIN
    UPDATE photos SET last_viewed_at = new.last_viewed_at WHERE id = new.photo_id;
END;
