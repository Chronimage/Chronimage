/**
 * Vitest global setup — jsdom augmentation + Tauri invoke mocks.
 * Imported from `vitest.config.ts` via `setupFiles`.
 */

import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// Mock Tauri event API — listen/emit need window.__TAURI_INTERNALS__ which
// doesn't exist in jsdom. Return a no-op unlisten function.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));

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
      case 'list_albums':
        return [];
      case 'list_photos':
        return [];
      case 'list_sources':
        return [];
      case 'create_source':
        return { id: 1, name: 'Test', kind: 'local', status: 'idle', last_scan_at: null, photo_count: 0 };
      case 'start_import':
        return { import_id: 1 };
      case 'list_imports':
        return [];
      default:
        throw new Error(`mock invoke: unknown command ${cmd}`);
    }
  }),
}));
