/**
 * Typed wrapper around `@tauri-apps/plugin-store` for persisting small
 * key/value UI state (Settings tweaks) across app restarts.
 *
 * Store file: `<AppLocalData>/settings.json`. Uses `autoSave` so writes
 * flush to disk without an explicit `.save()` call. Silently falls back to
 * in-memory state when the plugin isn't available (e.g. vitest).
 */

import { load, type Store } from '@tauri-apps/plugin-store';
import { debug, error as logError } from './log';

const STORE_FILE = 'settings.json';

let storePromise: Promise<Store | null> | null = null;

async function getStore(): Promise<Store | null> {
  if (storePromise !== null) return storePromise;
  storePromise = (async () => {
    try {
      return await load(STORE_FILE, { autoSave: true, defaults: {} });
    } catch (e) {
      // In vitest the tauri plugin isn't mocked; return null so callers
      // silently fall back to in-memory defaults.
      debug('plugin-store unavailable, using in-memory only', e);
      return null;
    }
  })();
  return storePromise;
}

export async function loadPersisted<T>(key: string, fallback: T): Promise<T> {
  const store = await getStore();
  if (!store) return fallback;
  try {
    const v = await store.get<T>(key);
    return v ?? fallback;
  } catch (e) {
    logError('store.get failed', key, e);
    return fallback;
  }
}

export async function savePersisted<T>(key: string, value: T): Promise<void> {
  const store = await getStore();
  if (!store) return;
  try {
    await store.set(key, value);
  } catch (e) {
    logError('store.set failed', key, e);
  }
}
