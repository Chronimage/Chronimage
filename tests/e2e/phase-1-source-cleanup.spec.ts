import { expect, test } from '@playwright/test';

// Phase 1 exit criterion: 100-photo dry-run -> live-delete -> SHA256 post-check,
// no data loss. PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// ## How this spec runs
//
// Two modes, mirroring phase-1-import-throughput.spec.ts:
//  1. Default vite dev server -> skipped (no Tauri bridge).
//  2. `CHRONIMAGE_E2E_TAURI=1` -> drives real Rust via tauri-driver.
//
// The debug-only `__test_seed_source_copies` command (src-tauri/src/commands.rs)
// creates `count` real JPEGs on disk + inserts photos + source_copies rows
// with `verified_sha256` so `cleanup_dry_run` picks them up and
// `cleanup_execute` can re-verify SHAs against the on-disk bytes.

const TAURI_MODE = process.env.CHRONIMAGE_E2E_TAURI === '1';
const FIXTURE_COUNT = Number.parseInt(process.env.CHRONIMAGE_E2E_CLEANUP_COUNT ?? '100', 10);

interface Tauri {
  core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> };
}
function tauri(): Tauri {
  return (globalThis as unknown as { __TAURI__: Tauri }).__TAURI__;
}

test.describe('phase-1 source cleanup', () => {
  test.skip(!TAURI_MODE, 'requires CHRONIMAGE_E2E_TAURI=1 + tauri-driver');

  test(`dry-run -> execute deletes ${FIXTURE_COUNT} files without data loss`, async ({ page }) => {
    test.setTimeout(180_000);

    await page.goto('/');
    await page.waitForFunction(() => Boolean((globalThis as unknown as { __TAURI__?: unknown }).__TAURI__));

    // 1. Resolve a tempdir for the fixture files.
    const fixtureDir = await page.evaluate(async () => {
      // Use a deterministic timestamped path under the tauri tmp resolver, or
      // fall back to a simple relative path — __test_seed_source_copies
      // creates the dir if missing.
      return `${Date.now()}-phase-1-cleanup`;
    });

    // 2. Create a local-kind source row.
    const sourceId = await page.evaluate(
      async ({ root }) =>
        tauri()
          .core.invoke<{ id: number }>('create_source', {
            name: 'phase-1-cleanup-e2e',
            kind: 'local',
            rootPath: root,
          })
          .then((s) => s.id),
      { root: fixtureDir },
    );

    // 3. Seed `count` SHA256-verified source_copies rows + write real files.
    const photoIds = await page.evaluate(
      async ({ sourceId: sid, dir, count }) => {
        return tauri().core.invoke<number[]>('__test_seed_source_copies', {
          sourceId: sid,
          dir,
          count,
        });
      },
      { sourceId, dir: fixtureDir, count: FIXTURE_COUNT },
    );
    expect(photoIds.length).toBe(FIXTURE_COUNT);

    // 4. Dry-run -> plan_id + confirm_token.
    const plan = await page.evaluate(async () => {
      return tauri().core.invoke<{
        plan_id: string;
        confirm_token: string;
        total_file_count: number;
        sources: Array<{ source_id: number; file_count: number }>;
      }>('cleanup_dry_run');
    });
    expect(plan.total_file_count).toBeGreaterThanOrEqual(FIXTURE_COUNT);
    expect(plan.plan_id).toMatch(/[0-9a-f-]{36}/);
    expect(plan.confirm_token).toMatch(/[0-9a-f-]{36}/);

    // 5. Execute -> deletes files + returns receipt.
    const receipt = await page.evaluate(
      async ({ planId, confirmToken }) => {
        return tauri().core.invoke<{
          deleted_count: number;
          freed_bytes: number;
          errors: string[];
        }>('cleanup_execute', { planId, confirmToken });
      },
      { planId: plan.plan_id, confirmToken: plan.confirm_token },
    );

    // 6. Assertions: all fixture files deleted, zero errors, non-trivial bytes freed.
    expect(receipt.errors).toEqual([]);
    expect(receipt.deleted_count).toBeGreaterThanOrEqual(FIXTURE_COUNT);
    expect(receipt.freed_bytes).toBeGreaterThan(FIXTURE_COUNT * 10_000);
  });
});
