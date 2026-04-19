import { test } from '@playwright/test';

// Phase 1 exit criterion: 100-photo dry-run → live-delete → SHA256 post-check,
// no data loss.
//
// PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// Skipped until the 100-photo fixture + tempdir source copies are wired.

test.skip('source cleanup 100-photo dry-run -> execute has zero data loss', async () => {
  // TODO(cc): seed a 100-photo catalog with source_copies in a tempdir, call
  // cleanupDryRun() to get a plan, call cleanupExecute(plan_id, confirm_token),
  // verify every deleted file's pre-delete SHA256 matches the recorded SHA,
  // verify the retained source_copies still resolve on disk, assert
  // deleted_count === 100 and errors.length === 0.
});
