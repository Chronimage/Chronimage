-- Migration: place_label
-- Phase: 4 §5 — per-photo reverse-geocoded label.
--
-- Caches the nearest-bundled-city label ("Bengaluru, IN") on every
-- photo that has GPS at import time. A future Catalog Places facet
-- reads this column; Map popups already pick it up via the trips-level
-- auto_name logic.
--
-- Forward-only. Do NOT edit once merged.

ALTER TABLE photos ADD COLUMN place_label TEXT;
CREATE INDEX IF NOT EXISTS idx_photos_place_label
    ON photos(place_label) WHERE place_label IS NOT NULL;
