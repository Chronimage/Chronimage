-- Migration: smart_albums_kind
-- Phase: 1 — Deep AI Catalog (rediscovery + background re-evaluator)
--
-- Adds a `kind` discriminator to smart_albums so the re-evaluator can identify
-- special system albums (e.g. 'rediscovery_today') that need their rule_json
-- recomputed on every pass rather than treated as static.
--
-- NULL means the album's rule is static and should be evaluated as-is.
-- 'rediscovery_today' means the evaluator must recompute the MM-DD value in
-- rule_json to match today's date before counting/matching.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

ALTER TABLE smart_albums ADD COLUMN kind TEXT;  -- NULL | 'rediscovery_today'

CREATE INDEX IF NOT EXISTS idx_smart_albums_kind ON smart_albums(kind) WHERE kind IS NOT NULL;
