import { expect, type Page, test } from '@playwright/test';

/**
 * Phase 1 visual-goldens pass. Captures `toHaveScreenshot` baselines for
 * every routed screen so Phase 2 refactors have a regression baseline.
 *
 * Runs under `pnpm dev` (vite, no Tauri runtime). Since the screens
 * depend on `invoke()` returning typed data, we install a stub on
 * `window.__TAURI_INTERNALS__.invoke` that hands back empty/default values
 * keyed by command name. The result is the "connected-but-empty" state of
 * each screen — which is the cleanest golden baseline to regress against.
 *
 * Fixtures live under `tests/visual-goldens/phase-1-screens.spec.ts-snapshots/`
 * (Playwright's default per-file snapshot dir). Tolerance is set globally
 * in `playwright.config.ts` (`maxDiffPixelRatio: 0.02`).
 */

/** Canned invoke responses keyed by Tauri command name. */
const STUBBED: Record<string, unknown> = {
  ping: 'pong',
  app_version: '0.1.0-dev',
  current_channel: { channel: 'dev' },
  // Source surface
  list_sources: [],
  detect_icloud_path: null,
  list_iphone_devices: [],
  detect_hardware: { cpu_cores: 8, ram_gb: 16, gpu_name: null, vram_gb: null },
  // Catalog
  list_albums: [],
  list_photos: { photos: [], next_cursor: null },
  list_imports: [],
  list_tags: [],
  photo_quality: null,
  photo_location: null,
  find_duplicates: [],
  search_photos: [],
  search_suggestions: [],
  get_thumbnail: null,
  // Face clusters
  face_clusters_list: [],
  list_photos_for_cluster: [],
  // Rediscovery
  on_this_day: [],
  unseen_photos: [],
  first_time_on_new_camera: [],
  unflagged_favorites: [],
  refresh_smart_albums: { rebuilt: 0 },
  // AI models
  ai_models_status: [],
  // Google Photos
  gphotos_auth_status: false,
  // Cleanup
  cleanup_dry_run: { plan_id: '', confirm_token: '', sources: [], total_bytes: 0 },
};

async function installInvokeStub(page: Page): Promise<void> {
  await page.addInitScript((stubbed) => {
    const responses = stubbed as Record<string, unknown>;
    // Minimal Tauri v2 runtime shim. @tauri-apps/api/core reads from
    // `__TAURI_INTERNALS__`; event listeners also look up
    // `transformCallback`, which we return a fake id for (no events
    // ever fire, which is fine for static screenshots).
    let callbackId = 0;
    (globalThis as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd in responses) {
          return Promise.resolve(responses[cmd]);
        }
        // Unknown commands → null so .then() chains don't reject and
        // components fall back to empty states.
        return Promise.resolve(null);
      },
      transformCallback: () => {
        callbackId += 1;
        return callbackId;
      },
      unregisterCallback: () => {},
      convertFileSrc: (path: string) => path,
      // `@tauri-apps/api/window` + plugin-window reach into this
      // metadata bag at import time; a missing `currentWindow` makes
      // React's StrictMode double-render crash with "Cannot read
      // properties of undefined (reading 'currentWindow')".
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main', windowLabel: 'main' },
        windows: [{ label: 'main' }],
        webviews: [{ label: 'main', windowLabel: 'main' }],
      },
      plugins: {},
    };
  }, STUBBED);
}

/** Wait for the fonts + initial network-like settlement before snapshotting. */
async function waitForStable(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded');
  await page
    .evaluate(
      () =>
        new Promise<void>((resolve) => {
          const d = document as any;
          if (d.fonts && typeof d.fonts.ready?.then === 'function') {
            d.fonts.ready.then(() => resolve());
          } else {
            resolve();
          }
        }),
    )
    .catch(() => {
      /* fonts API unavailable — best-effort */
    });
  // Give React queries + transitions one frame to settle into empty states.
  await page.waitForTimeout(300);
}

test.describe('phase-1 visual goldens', () => {
  test.beforeEach(async ({ page }) => {
    // Block Google Fonts — unreachable on offline CI machines makes the
    // render-blocking <link rel="stylesheet"> stall indefinitely.
    await page.route(/fonts\.googleapis\.com|fonts\.gstatic\.com/, (route) => route.abort());
    // Surface console errors to the test log so hydration crashes are
    // visible when a golden fails.
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        console.error(`[browser error] ${msg.text()}`);
      }
    });
    page.on('pageerror', (err) => {
      console.error(`[page error] ${err.message}`);
    });
    await installInvokeStub(page);
  });

  for (const spec of [
    { name: 'onboard', label: 'Sources' },
    { name: 'catalog', label: 'Catalog' },
    { name: 'people', label: 'People' },
    { name: 'settings', label: 'Settings' },
  ]) {
    test(`${spec.name} screen renders`, async ({ page }) => {
      await page.goto('/');
      // Wait for the app shell to actually hydrate — the rail is a
      // top-level chrome element, so its appearance is the earliest
      // stable signal that React mounted successfully.
      await page.locator('.rail').waitFor({ timeout: 15_000 });
      await waitForStable(page);
      // Rail buttons carry `aria-label` matching the screen label.
      await page.getByRole('button', { name: spec.label }).first().click();
      await waitForStable(page);
      await expect(page).toHaveScreenshot(`${spec.name}.png`, {
        fullPage: false,
        maxDiffPixelRatio: 0.02,
      });
    });
  }
});
