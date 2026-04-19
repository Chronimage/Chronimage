-- Migration: phase1_catalog
-- Phase: 1 — Deep AI Catalog
--
-- Adds: tags, photo embeddings (via sqlite-vec when available), faces,
-- clusters, smart albums, photo views (for rediscovery), source deletions
-- (audit log), and an AI models registry.
--
-- Also extends photos + sources with Phase 1 columns.
--
-- Forward-only. Do NOT edit once merged.
-- sqlx wraps each migration in its own transaction; no BEGIN/COMMIT here.

-- ── Photos: additional columns for Phase 1 ──────────────────────────────────

ALTER TABLE photos ADD COLUMN phash                TEXT;           -- 64-bit perceptual hash, hex
ALTER TABLE photos ADD COLUMN camera_make          TEXT;
ALTER TABLE photos ADD COLUMN camera_model         TEXT;
ALTER TABLE photos ADD COLUMN lens_model           TEXT;
ALTER TABLE photos ADD COLUMN aperture             REAL;
ALTER TABLE photos ADD COLUMN shutter              TEXT;           -- "1/500" etc
ALTER TABLE photos ADD COLUMN iso                  INTEGER;
ALTER TABLE photos ADD COLUMN focal_mm             REAL;
ALTER TABLE photos ADD COLUMN orientation          INTEGER NOT NULL DEFAULT 1;
ALTER TABLE photos ADD COLUMN captured_at_local    TEXT;           -- naive local-time string
ALTER TABLE photos ADD COLUMN gps_lat              REAL;
ALTER TABLE photos ADD COLUMN gps_lng              REAL;
ALTER TABLE photos ADD COLUMN aesthetic_score      REAL;           -- 0–10 (NIMA)
ALTER TABLE photos ADD COLUMN sharpness_score      REAL;           -- 0–1
ALTER TABLE photos ADD COLUMN size_bytes           INTEGER;
ALTER TABLE photos ADD COLUMN raw_format           TEXT;           -- 'ARW' | 'CR3' | 'NEF' | …

CREATE INDEX IF NOT EXISTS idx_photos_phash              ON photos(phash);
CREATE INDEX IF NOT EXISTS idx_photos_captured_local     ON photos(captured_at_local);
CREATE INDEX IF NOT EXISTS idx_photos_camera             ON photos(camera_make, camera_model);
CREATE INDEX IF NOT EXISTS idx_photos_aesthetic          ON photos(aesthetic_score);

-- ── Tags (AI and user-assigned) ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS tags (
  id          INTEGER PRIMARY KEY,
  photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  label       TEXT    NOT NULL,
  kind        TEXT    NOT NULL,            -- 'people'|'place'|'object'|'event'|'color'|'camera'|'auto_scene'|'user'
  confidence  REAL    NOT NULL DEFAULT 1.0 CHECK (confidence BETWEEN 0 AND 1),
  model_id    INTEGER REFERENCES models(id) ON DELETE SET NULL,
  created_at  TEXT    NOT NULL,
  UNIQUE(photo_id, label, kind)
);

CREATE INDEX IF NOT EXISTS idx_tags_photo ON tags(photo_id);
CREATE INDEX IF NOT EXISTS idx_tags_label ON tags(label);
CREATE INDEX IF NOT EXISTS idx_tags_kind  ON tags(kind);

-- ── AI model registry ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS models (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL UNIQUE,
  kind            TEXT    NOT NULL,        -- 'embedding'|'face_detect'|'face_embed'|'aesthetic'|'caption'
  version         TEXT    NOT NULL,
  sha256          TEXT    NOT NULL,
  installed_path  TEXT,
  installed_at    TEXT,
  size_bytes      INTEGER
);

-- ── Photo embeddings (join table onto sqlite-vec virtual table) ─────────────
--
-- sqlite-vec is optional at load time; when unavailable we fall back to
-- brute-force cosine over a plain BLOB column. photo_embeddings always exists;
-- the virtual table is created conditionally in Rust at startup.

CREATE TABLE IF NOT EXISTS photo_embeddings (
  photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  model_id    INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  vec_rowid   INTEGER,                      -- rowid into vec_photo_embeddings (nullable during fallback)
  embedding   BLOB,                         -- fallback: raw f32 array
  updated_at  TEXT    NOT NULL,
  PRIMARY KEY (photo_id, model_id)
);

CREATE INDEX IF NOT EXISTS idx_photo_embeddings_model ON photo_embeddings(model_id);

-- ── Faces ────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS clusters (
  id              INTEGER PRIMARY KEY,
  name            TEXT,                      -- NULL = unnamed
  is_named        INTEGER NOT NULL DEFAULT 0 CHECK (is_named IN (0,1)),
  cover_face_id   INTEGER,                   -- FK added after faces table
  photo_count     INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS faces (
  id                   INTEGER PRIMARY KEY,
  photo_id             INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  cluster_id           INTEGER REFERENCES clusters(id) ON DELETE SET NULL,
  bbox_x               REAL    NOT NULL,
  bbox_y               REAL    NOT NULL,
  bbox_w               REAL    NOT NULL,
  bbox_h               REAL    NOT NULL,
  quality              REAL    NOT NULL DEFAULT 0,
  eyes_open            REAL,
  embedding            BLOB,                 -- 512-dim f32
  embedding_vec_rowid  INTEGER,
  created_at           TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_faces_photo   ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);

-- ── Smart albums ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS smart_albums (
  id                  INTEGER PRIMARY KEY,
  name                TEXT    NOT NULL UNIQUE,
  description         TEXT,
  rule_json           TEXT    NOT NULL CHECK (json_valid(rule_json)),
  cover_photo_ids     TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(cover_photo_ids)),
  photo_count         INTEGER NOT NULL DEFAULT 0,
  tag                 TEXT,                  -- facet for UI grouping (people|lighting|cull|etc.)
  is_system           INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL
);

-- ── Photo views (rediscovery surfaces rely on this) ─────────────────────────

CREATE TABLE IF NOT EXISTS photo_views (
  photo_id        INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
  last_viewed_at  TEXT,
  view_count      INTEGER NOT NULL DEFAULT 0
);

-- ── Source deletions (audit log for cloud-unload) ───────────────────────────

CREATE TABLE IF NOT EXISTS source_deletions (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER REFERENCES photos(id) ON DELETE SET NULL,  -- nullable in case photo later purged
  source_id       INTEGER NOT NULL REFERENCES sources(id) ON DELETE RESTRICT,
  source_kind     TEXT    NOT NULL,
  external_id     TEXT,
  deleted_at      TEXT    NOT NULL,
  pre_sha256      TEXT    NOT NULL,
  pre_size_bytes  INTEGER NOT NULL,
  confirm_token   TEXT    NOT NULL,          -- matches the dry-run plan id for audit
  dry_run         INTEGER NOT NULL DEFAULT 0 CHECK (dry_run IN (0,1))
);

CREATE INDEX IF NOT EXISTS idx_source_deletions_source ON source_deletions(source_id);
CREATE INDEX IF NOT EXISTS idx_source_deletions_time   ON source_deletions(deleted_at);

-- ── FTS5: searchable text across filename + tags ─────────────────────────────

CREATE VIRTUAL TABLE IF NOT EXISTS photos_fts USING fts5(
  filename,
  tags,
  content='',   -- external content; synced via triggers
  tokenize = 'porter unicode61 remove_diacritics 2'
);

-- Trigger: on photo insert/update, refresh fts row (tags concatenated later by a trigger on tags too)
CREATE TRIGGER IF NOT EXISTS photos_fts_insert AFTER INSERT ON photos BEGIN
  INSERT INTO photos_fts(rowid, filename, tags) VALUES (new.id, new.filename, '');
END;

CREATE TRIGGER IF NOT EXISTS photos_fts_delete AFTER DELETE ON photos BEGIN
  DELETE FROM photos_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS photos_fts_update AFTER UPDATE OF filename ON photos BEGIN
  UPDATE photos_fts SET filename = new.filename WHERE rowid = new.id;
END;

-- Rebuild FTS tags cell whenever tags table changes (cheap: re-join)
CREATE TRIGGER IF NOT EXISTS tags_fts_insert AFTER INSERT ON tags BEGIN
  UPDATE photos_fts
    SET tags = (SELECT group_concat(label, ' ') FROM tags WHERE photo_id = new.photo_id)
    WHERE rowid = new.photo_id;
END;

CREATE TRIGGER IF NOT EXISTS tags_fts_delete AFTER DELETE ON tags BEGIN
  UPDATE photos_fts
    SET tags = COALESCE((SELECT group_concat(label, ' ') FROM tags WHERE photo_id = old.photo_id), '')
    WHERE rowid = old.photo_id;
END;

-- ── Imports: track last seen path for resume ─────────────────────────────────
-- (last_seen_path already added in initial migration)

-- ── Schema version bump ─────────────────────────────────────────────────────

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '2', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
