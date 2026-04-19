import { test } from '@playwright/test';

// Phase 1 exit criterion: 100k-photo test library imports in ≤ 180 minutes on a
// mid-tier laptop (i5 + 16 GB, no GPU).
//
// PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// Skipped until the 100k-photo synthetic fixture exists at
// tests/fixtures/import-throughput/. Run against a packaged Tauri build via
// tauri-driver, not the vite dev server.

test.skip('100k-photo import completes in under 180 minutes', async () => {
  // TODO(cc): drive the packaged binary via tauri-driver, point it at the
  // fixture dir, start the import, measure wall-clock until the imports table
  // reaches 100000 rows, assert duration <= 180 * 60 * 1000.
});
