-- Backfill the RAW → JPG side of RAW+JPG pairing.
--
-- The import pipeline used to set only the JPG → RAW direction
-- (`UPDATE photos SET paired_photo_id = raw_id WHERE id = jpg_id`),
-- but every consumer (thumbnail generator, develop preview, AI stage
-- routing) reads the RAW row's `paired_photo_id` to decide whether
-- to open the paired JPG instead of demosaicing the RAW. With only
-- one direction set, every develop-open of a paired ARW fell through
-- to rawler — which on Sony A7 IV files returns no embedded preview
-- and ends up byte-scanning for a small embedded JPEG.
--
-- This one-time backfill walks every JPG that points at a RAW and
-- sets the RAW's `paired_photo_id` back to the JPG so the existing
-- catalog stops hitting the rawler fallback. Going forward the
-- import pipeline writes both directions in the same transaction
-- so the migration is idempotent — re-running it is a no-op.

UPDATE photos
SET    paired_photo_id = (
  SELECT j.id
  FROM   photos j
  WHERE  j.paired_photo_id = photos.id
  LIMIT  1
)
WHERE  is_raw = 1
  AND  paired_photo_id IS NULL
  AND  EXISTS (
    SELECT 1 FROM photos j WHERE j.paired_photo_id = photos.id
  );
