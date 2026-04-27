/**
 * User settings persisted via `@tauri-apps/plugin-store` (settings.json).
 *
 * Distinct from [`useUi`](./ui.ts) `Tweaks`, which covers cosmetic display
 * preferences. This module holds behavioural settings that affect imports
 * and file operations:
 *
 *   - `catalog_home_path` — destination folder used when mode is
 *     'consolidate'. Falls back to `get_default_catalog_path` (Pictures /
 *     Chronimage) when unset.
 */

import { useEffect, useState } from 'react';
import { loadPersisted, savePersisted } from '../util/store';

const CATALOG_HOME_KEY = 'catalog_home_path';

/**
 * Reads + writes `catalog_home_path`. Returns `[value, setValue, hydrated]`.
 * `value` is `null` when unset (caller falls back to the Rust-provided
 * default via `get_default_catalog_path`).
 */
export function useCatalogHome(): [string | null, (next: string | null) => Promise<void>, boolean] {
  const [path, setPath] = useState<string | null>(null);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const loaded = await loadPersisted<string | null>(CATALOG_HOME_KEY, null);
      if (!cancelled) {
        setPath(loaded);
        setHydrated(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function set(next: string | null) {
    setPath(next);
    await savePersisted<string | null>(CATALOG_HOME_KEY, next);
  }

  return [path, set, hydrated];
}
