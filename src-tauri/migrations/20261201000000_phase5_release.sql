-- Migration: phase5_release
-- Phase: 5 — Release hardening + public launch.
--
-- Adds a single-row `license_state` table for the Insider program's
-- signed licence flow (§7 of the PRD). `community` is the default
-- state for every install; Insiders drop a signed `license.json` into
-- the app data dir which populates this row after Ed25519 verification.
--
-- Forward-only. Do NOT edit once merged.

CREATE TABLE IF NOT EXISTS license_state (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  plan            TEXT    NOT NULL DEFAULT 'community',
  email           TEXT,
  issued_at       TEXT,
  expires_at      TEXT,
  signature       TEXT,
  verified_at     TEXT,
  last_checked_at TEXT
);

INSERT OR IGNORE INTO license_state(id, plan) VALUES (1, 'community');

-- Telemetry opt-in default: OFF. `telemetry::event()` is a no-op unless
-- the user flips this explicitly via the first-run dialog or Settings.
INSERT OR IGNORE INTO settings(key, value, updated_at)
VALUES ('telemetry.enabled', '0', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '8', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
