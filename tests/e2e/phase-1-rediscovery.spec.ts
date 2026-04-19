import { test } from '@playwright/test';

// Phase 1 exit criterion: "On this day" and "Unseen in 2 years" rows populate
// against a dated fixture.
//
// PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// Skipped until the dated-photos fixture is wired.

test.skip('rediscovery rows populate against a dated fixture', async () => {
  // TODO(cc): seed a catalog where N photos have captured_at = today's MM-DD
  // in previous years, and M photos have last_viewed_at older than 2y, wait
  // for the 10-min re-evaluator tick (or call refresh_smart_albums manually),
  // open Catalog home, assert the "On this day" row shows N items and
  // "Unseen in 2 years" shows M items.
});
