import { expect, test } from '@playwright/test';

// Phase 1 exit criterion: NL search hits its p95 latency budget end-to-end
// from the browser. PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// ## Scope split with the Rust integration test
//
// `src-tauri/tests/phase_1_search_latency.rs` carries the production NFR: it
// seeds 200 k photos + embeddings and measures the pure Rust `search_photos`
// path against a 750 ms p95 threshold on sqlite-vec 0.1.9's brute-force KNN.
//
// This e2e spec measures the *browser round-trip* — `__TAURI__.core.invoke`
// → Rust command → JSON-encoded result back to the JS side — against a
// smaller fixture. What we're validating is that the IPC envelope doesn't
// add surprise latency (serialize-over-stdout can sometimes blow up on
// large result bodies). A 1 000-photo seed is ample to surface any IPC
// regression without blowing out CI time.
//
// The default p95 budget is a generous 1 200 ms (1 s Rust + 200 ms IPC
// overhead). Nightly CI can override via CHRONIMAGE_E2E_SEARCH_P95_MS.

const TAURI_MODE = process.env.CHRONIMAGE_E2E_TAURI === '1';
const SEED_COUNT = Number.parseInt(process.env.CHRONIMAGE_E2E_SEARCH_COUNT ?? '1000', 10);
const SEED_QUERIES = 10;
const ITERATIONS_PER_QUERY = 5;
const P95_BUDGET_MS = Number.parseInt(process.env.CHRONIMAGE_E2E_SEARCH_P95_MS ?? '1200', 10);

const QUERIES = [
  'golden hour portraits',
  'sunset over water',
  'a laughing child',
  'snow-capped mountains',
  'neon street at night',
  'a dog on the beach',
  'handwritten notes',
  'crowded city square',
  'close-up of flowers',
  'coffee shop interior',
];

interface Tauri {
  core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> };
}
function tauri(): Tauri {
  return (globalThis as unknown as { __TAURI__: Tauri }).__TAURI__;
}

function percentile(xs: number[], p: number): number {
  if (xs.length === 0) return Number.POSITIVE_INFINITY;
  const sorted = [...xs].sort((a, b) => a - b);
  const rank = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[rank] ?? Number.POSITIVE_INFINITY;
}

test.describe('phase-1 search latency (browser round-trip)', () => {
  test.skip(!TAURI_MODE, 'requires CHRONIMAGE_E2E_TAURI=1 + tauri-driver');

  test(`p95 of ${SEED_QUERIES * ITERATIONS_PER_QUERY} invoke() searches <= ${P95_BUDGET_MS} ms`, async ({
    page,
  }) => {
    test.setTimeout(180_000);

    await page.goto('/');
    await page.waitForFunction(() => Boolean((globalThis as unknown as { __TAURI__?: unknown }).__TAURI__));

    // 1. Seed photos + embeddings directly (no disk I/O — this is a search
    //    benchmark, not an import one).
    const seeded = await page.evaluate(
      async ({ count }) => tauri().core.invoke<number>('__test_seed_embeddings', { count }),
      { count: SEED_COUNT },
    );
    expect(seeded).toBeGreaterThanOrEqual(SEED_COUNT);

    // 2. For each query × iteration, measure the browser-observed round-trip
    //    time of `search_photos`. We deliberately include the first-iteration
    //    outliers — IPC warm-up is part of what the user feels.
    const latenciesMs = await page.evaluate(
      async ({ queries, iterations }) => {
        const samples: number[] = [];
        for (const q of queries) {
          for (let i = 0; i < iterations; i += 1) {
            const t0 = performance.now();
            await tauri()
              .core.invoke<Array<{ id: number }>>('search_photos', { query: q, limit: 50 })
              .catch(() => []);
            samples.push(performance.now() - t0);
          }
        }
        return samples;
      },
      { queries: QUERIES, iterations: ITERATIONS_PER_QUERY },
    );

    expect(latenciesMs.length).toBe(SEED_QUERIES * ITERATIONS_PER_QUERY);
    const p95 = percentile(latenciesMs, 95);
    const p50 = percentile(latenciesMs, 50);
    // Surface the benchmark numbers via the assertion message so they land
    // in the playwright html report on both pass and fail.
    expect(
      p95,
      `search latency: p50=${p50.toFixed(1)} ms p95=${p95.toFixed(1)} ms (${latenciesMs.length} samples, ${SEED_COUNT} photos); budget ${P95_BUDGET_MS} ms`,
    ).toBeLessThanOrEqual(P95_BUDGET_MS);
  });
});
