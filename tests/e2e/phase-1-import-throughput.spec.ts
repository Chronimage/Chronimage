import { expect, test } from '@playwright/test';

// Phase 1 exit criterion: 100k-photo test library imports in ≤ 180 minutes on
// a mid-tier laptop (i5 + 16 GB, no GPU). PRD: docs/prds/phase-1.md § Exit
// criteria.
//
// ## How this spec actually runs
//
// Chronimage's e2e suite has two modes:
//  1. Default (vite dev server): no Tauri runtime, `invoke()` calls fail. We
//     skip with a reason so CI stays green.
//  2. `CHRONIMAGE_E2E_TAURI=1` (requires tauri-driver + a packaged debug
//     binary): the `invoke` calls hit real Rust. This path exercises the
//     pipeline end-to-end against a synthetic fixture we generate via the
//     debug-only `__test_generate_fixture` Tauri command.
//
// `CHRONIMAGE_E2E_IMPORT_COUNT` trims the fixture size — 10 000 locally,
// 100 000 on nightly CI. Threshold scales with count so the assertion stays
// meaningful across both sizes.

const TAURI_MODE = process.env.CHRONIMAGE_E2E_TAURI === '1';
const FIXTURE_COUNT = Number.parseInt(process.env.CHRONIMAGE_E2E_IMPORT_COUNT ?? '10000', 10);
const MS_PER_PHOTO_BUDGET = 108; // 180 min / 100k photos = 108 ms/photo ceiling
const TIMEOUT_MS = Math.ceil(FIXTURE_COUNT * MS_PER_PHOTO_BUDGET * 1.5) + 60_000;

test.describe('phase-1 import throughput', () => {
  test.skip(!TAURI_MODE, 'requires CHRONIMAGE_E2E_TAURI=1 + tauri-driver');

  test(`imports ${FIXTURE_COUNT} synthetic photos under budget`, async ({ page }) => {
    test.setTimeout(TIMEOUT_MS);

    // Resolve the Tauri `invoke` from the page context. The app exposes it
    // on `window.__TAURI__.core.invoke` once the webview boots.
    await page.goto('/');
    await page.waitForFunction(() => Boolean((globalThis as unknown as { __TAURI__?: unknown }).__TAURI__));

    const fixtureDir = await page.evaluate(async () => {
      const tauri = (
        globalThis as unknown as {
          __TAURI__: { core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> } };
        }
      ).__TAURI__;
      const dir = await tauri.core.invoke<string>('resolve_temp_fixture_dir').catch(() => {
        // Fallback: synthesize a path under the OS temp. __test_generate_fixture
        // will create the directory if it doesn't exist.
        return `${Date.now()}-phase-1-import-throughput`;
      });
      return dir;
    });

    const written = await page.evaluate(
      async ({ dir, count }) => {
        const tauri = (
          globalThis as unknown as {
            __TAURI__: { core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> } };
          }
        ).__TAURI__;
        return tauri.core.invoke<number>('__test_generate_fixture', { dir, count });
      },
      { dir: fixtureDir, count: FIXTURE_COUNT },
    );
    expect(written).toBe(FIXTURE_COUNT);

    // Create a source row pointing at the fixture dir.
    const sourceId = await page.evaluate(
      async ({ root }) => {
        const tauri = (
          globalThis as unknown as {
            __TAURI__: { core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> } };
          }
        ).__TAURI__;
        const row = await tauri.core.invoke<{ id: number }>('create_source', {
          name: 'phase-1-e2e-fixture',
          kind: 'local',
          rootPath: root,
        });
        return row.id;
      },
      { root: fixtureDir },
    );

    // Kick off the import + poll until imports.finished_at is populated.
    const t0 = Date.now();
    const importId = await page.evaluate(
      async ({ sourceId: sid, root }) => {
        const tauri = (
          globalThis as unknown as {
            __TAURI__: { core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> } };
          }
        ).__TAURI__;
        const resp = await tauri.core.invoke<{ import_id: number }>('start_import', {
          sourceId: sid,
          root,
        });
        return resp.import_id;
      },
      { sourceId, root: fixtureDir },
    );

    await expect
      .poll(
        async () =>
          page.evaluate(async (sid: number) => {
            const tauri = (
              globalThis as unknown as {
                __TAURI__: { core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> } };
              }
            ).__TAURI__;
            const rows = await tauri.core.invoke<
              Array<{
                id: number;
                imported_count: number;
                finished_at: string | null;
              }>
            >('list_imports', { sourceId: sid });
            return rows.find((r) => r.id === importId) ?? null;
          }, sourceId),
        {
          // Poll every 2 s; overall budget set via test.setTimeout above.
          intervals: [2_000],
          timeout: TIMEOUT_MS - 30_000,
          message: `import ${importId} did not finish in time`,
        },
      )
      .toMatchObject({ finished_at: expect.any(String) });

    const elapsed = Date.now() - t0;
    const budget = FIXTURE_COUNT * MS_PER_PHOTO_BUDGET;
    expect(
      elapsed,
      `imported ${FIXTURE_COUNT} photos in ${elapsed} ms; budget ${budget} ms`,
    ).toBeLessThanOrEqual(budget);
  });
});
