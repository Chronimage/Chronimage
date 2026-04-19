import { test } from '@playwright/test';

// Phase 1 exit criterion: 200k-photo search ≤ 500 ms 95p over 10 seed queries.
//
// PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// Skipped until the 200k-photo synthetic catalog is seeded (fixture path TBD).

test.skip('200k-photo search hits 500ms 95p across 10 seed queries', async () => {
  // TODO(cc): seed a synthetic 200k catalog (no real image bytes needed —
  // synthesize embeddings + EXIF rows directly), run each query 20x,
  // compute the 95th-percentile latency over the 10 queries × 20 iterations,
  // assert p95 <= 500ms.
});
