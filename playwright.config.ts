import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: './tests',
  testMatch: ['e2e/**/*.spec.ts', 'visual-goldens/**/*.spec.ts'],
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: isCI ? 1 : undefined,
  reporter: [
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
    ['junit', { outputFile: 'playwright-report/results.xml' }],
    ['list'],
  ],
  use: {
    baseURL: 'http://127.0.0.1:1420',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: isCI ? 'retain-on-failure' : 'off',
  },
  projects: [
    {
      name: 'chromium-webview',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  // In real E2E we drive the packaged Tauri binary via tauri-driver instead of vite dev.
  // For now (Phase 0), tests target the vite dev server.
  webServer: isCI
    ? undefined
    : {
        command: 'pnpm dev',
        url: 'http://127.0.0.1:1420',
        reuseExistingServer: true,
        timeout: 120_000,
      },
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.02,
      animations: 'disabled',
    },
  },
});
