-- Migration: sqlite_vec
-- Phase: 1 — Deep AI Catalog
--
-- Records the addition of the sqlite-vec virtual table for 768-dim SigLIP
-- photo embeddings. The actual CREATE VIRTUAL TABLE DDL is intentionally
-- NOT here: sqlite-vec is a loadable extension that may be absent on some
-- machines, and sqlx's migration runner cannot gracefully skip a failing
-- statement. Instead, `open_pool` in src-tauri/src/catalog/db.rs calls
-- `try_init_sqlite_vec` after migrations complete — that function issues
-- the CREATE VIRTUAL TABLE IF NOT EXISTS and swallows any error, so the
-- app continues with the BLOB fallback path.
--
-- This migration therefore only bumps the schema version so the runner has
-- a record of this logical change landing.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

-- ── Schema version bump ─────────────────────────────────────────────────────

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '3', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
