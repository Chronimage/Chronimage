-- Migration: prompt_history
-- Phase: 4 §1 — history for generative edits.
--
-- Captures every `prompt_edit` submission so users can scroll past
-- generations + accept/reject without losing the prompt that produced
-- them. Kept separate from `edits` because generative edits have
-- structurally different fields (prompt + seed + model + rendered
-- bytes) from the Phase-3 slider Operations struct.
--
-- Forward-only. Do NOT edit once merged.

CREATE TABLE IF NOT EXISTS prompt_edits (
  id              INTEGER PRIMARY KEY,
  photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
  prompt          TEXT    NOT NULL,
  strength        INTEGER NOT NULL CHECK (strength BETWEEN 0 AND 100),
  constraints_json TEXT   NOT NULL DEFAULT '[]' CHECK (json_valid(constraints_json)),
  mask_b64        TEXT,
  rendered_b64    TEXT    NOT NULL,
  model_id        TEXT    NOT NULL,
  seed            INTEGER NOT NULL DEFAULT 0,
  latency_ms      INTEGER NOT NULL DEFAULT 0,
  state           TEXT    NOT NULL DEFAULT 'pending'
                  CHECK (state IN ('pending', 'accepted', 'rejected')),
  created_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompt_edits_photo ON prompt_edits(photo_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_edits_state ON prompt_edits(state) WHERE state = 'pending';
