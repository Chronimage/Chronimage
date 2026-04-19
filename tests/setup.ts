/**
 * Vitest global setup — jsdom augmentation + Tauri invoke mocks.
 * Imported from `vitest.config.ts` via `setupFiles`.
 */

import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// Mock Tauri's invoke so React tests run without a live bridge.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    switch (cmd) {
      case 'ping':
        return 'pong';
      case 'app_version':
        return '0.0.0-test';
      case 'current_channel':
        return { channel: 'dev' };
      case 'import_dry_run':
        return { root: '/mock', total_files: 0, raw_jpg_pairs: 0, unpaired: 0, by_extension: [] };
      default:
        throw new Error(`mock invoke: unknown command ${cmd}`);
    }
  }),
}));
