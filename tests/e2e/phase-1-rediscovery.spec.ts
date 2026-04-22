import { expect, test } from '@playwright/test';

// Phase 1 exit criterion: "On this day" and "Unseen in 2 years" rediscovery
// rows populate against a dated fixture.
// PRD reference: docs/prds/phase-1.md § Exit criteria.
//
// Seeds directly into the catalog via `__test_seed_dated_photos` (debug-only
// Tauri command) then drives the `on_this_day` + `unseen_photos` commands
// that the Catalog home renders from.

const TAURI_MODE = process.env.CHRONIMAGE_E2E_TAURI === '1';
const ON_THIS_DAY = 8;
const UNSEEN = 12;

interface Tauri {
  core: { invoke: <T>(cmd: string, args?: unknown) => Promise<T> };
}
function tauri(): Tauri {
  return (globalThis as unknown as { __TAURI__: Tauri }).__TAURI__;
}

test.describe('phase-1 rediscovery rows', () => {
  test.skip(!TAURI_MODE, 'requires CHRONIMAGE_E2E_TAURI=1 + tauri-driver');

  test(`populates on-this-day (${ON_THIS_DAY}) + unseen (${UNSEEN}) rows`, async ({ page }) => {
    test.setTimeout(60_000);

    await page.goto('/');
    await page.waitForFunction(() => Boolean((globalThis as unknown as { __TAURI__?: unknown }).__TAURI__));

    // 1. Seed the dated fixture.
    const counts = await page.evaluate(
      async ({ onThisDayCount, unseenCount }) => {
        return tauri().core.invoke<{ on_this_day: number; unseen: number }>('__test_seed_dated_photos', {
          onThisDayCount,
          unseenCount,
        });
      },
      { onThisDayCount: ON_THIS_DAY, unseenCount: UNSEEN },
    );
    expect(counts.on_this_day).toBe(ON_THIS_DAY);
    expect(counts.unseen).toBe(UNSEEN);

    // 2. On-this-day row — query the backend command the Catalog home uses.
    const onThisDay = await page.evaluate(async () => {
      return tauri().core.invoke<Array<{ id: number; captured_at: string | null }>>('on_this_day', {
        limit: 50,
      });
    });
    expect(onThisDay.length).toBeGreaterThanOrEqual(ON_THIS_DAY);
    const today = new Date();
    const monthDay = `${String(today.getUTCMonth() + 1).padStart(2, '0')}-${String(today.getUTCDate()).padStart(2, '0')}`;
    for (const photo of onThisDay) {
      if (photo.captured_at) {
        expect(photo.captured_at).toContain(`-${monthDay}T`);
      }
    }

    // 3. Unseen row — our seed inserts photos with `last_viewed_at` > 2y ago
    //    and `aesthetic_score` = 7.5, above the default 0.0 threshold.
    const unseen = await page.evaluate(async () => {
      return tauri().core.invoke<Array<{ id: number; aesthetic_score: number | null }>>('unseen_photos', {
        limit: 50,
      });
    });
    expect(unseen.length).toBeGreaterThanOrEqual(UNSEEN);
  });
});
