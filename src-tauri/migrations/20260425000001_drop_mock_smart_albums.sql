-- Drop the personalised mock smart albums seeded by the old `catalog::seed`
-- list (Portraits, Golden Hour, Kids — Ari & Leo, Food & Kitchen, Japan ·
-- Autumn '25, Loop 2 · product shots, Weddings & Events, Milo (golden
-- retriever), Screenshots & Docs, Burst & Duplicates). These were design
-- placeholders that didn't reflect real user content.
--
-- "Night & Low Light" and "Out-of-focus" are kept because their rules
-- compute against real EXIF / quality data and populate automatically.
-- The four rediscovery albums (kind = 'rediscovery_*') are also kept.
--
-- Safe on fresh catalogs: the DELETE is a no-op when the rows don't exist.
-- User-created smart albums (is_system = 0) with the same name are
-- deliberately NOT deleted — the filter is gated on is_system = 1.
--
-- Forward-only. Do NOT edit once merged.

DELETE FROM smart_albums
WHERE is_system = 1
  AND name IN (
    'Portraits',
    'Golden Hour',
    'Kids — Ari & Leo',
    'Food & Kitchen',
    'Japan · Autumn ''25',
    'Loop 2 · product shots',
    'Weddings & Events',
    'Milo (golden retriever)',
    'Screenshots & Docs',
    'Burst & Duplicates'
  );
