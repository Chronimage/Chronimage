/**
 * Vitest global setup — jsdom augmentation + Tauri invoke mocks.
 * Imported from `vitest.config.ts` via `setupFiles`.
 */

import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// jsdom doesn't ship ResizeObserver; stub it so components that use it don't throw.
global.ResizeObserver = class ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
};

// Mock Tauri window API — getCurrentWindow().minimize/toggleMaximize/close
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    maximize: vi.fn(() => Promise.resolve()),
    unmaximize: vi.fn(() => Promise.resolve()),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    setSize: vi.fn(() => Promise.resolve()),
    center: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
  })),
  currentMonitor: vi.fn(() => Promise.resolve({ size: { width: 1920, height: 1080 }, scaleFactor: 1 })),
  LogicalSize: class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));

// Mock Tauri event API — listen/emit need window.__TAURI_INTERNALS__ which
// doesn't exist in jsdom. Return a no-op unlisten function.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));

// Mock Tauri dialog plugin — openDialog is invoked by OnboardScreen handlers.
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(() => Promise.resolve(null)),
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
      case 'cleanup_dry_run':
        return [];
      case 'refresh_smart_albums':
        return undefined;
      case 'on_this_day':
        return [];
      case 'unseen_photos':
        return [];
      case 'list_sources':
        return [];
      case 'create_source':
        return { id: 1, name: 'Test', kind: 'local', status: 'idle', last_scan_at: null, photo_count: 0 };
      case 'start_import':
        return { import_id: 1 };
      case 'list_imports':
        return [];
      case 'import_google_takeout':
        return { import_id: 2 };
      case 'detect_icloud_path':
        return null;
      case 'list_iphone_devices':
        return [];
      case 'detect_hardware':
        return { tier: 'CpuOnly', vram_mb: 0, adapter_name: 'stub' };
      case 'embed_image':
        return new Array(768).fill(0);
      case 'score_aesthetic':
        return 5.5;
      default:
        throw new Error(`mock invoke: unknown command ${cmd}`);
    }
  }),
}));
