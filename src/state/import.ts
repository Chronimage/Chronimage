/**
 * Global import-progress store.
 *
 * A single Zustand store holds a `Map<importId, ActiveImport>` populated by a
 * listener on `IMPORT_PROGRESS_EVENT`. The listener is mounted once at the
 * React root via `useImportProgressListener()` so every screen (catalog
 * sidebar, status bar, etc.) sees the same state.
 *
 * `mode` on each import is captured at start time from the user's current
 * `default_import_mode` setting — it determines whether the frontend
 * auto-fires a lift-and-shift after the import finishes.
 */

import { listen } from '@tauri-apps/api/event';
import { useEffect, useMemo } from 'react';
import { create } from 'zustand';
import { IMPORT_PROGRESS_EVENT, type ImportProgressEvent } from '../tauri/invoke';

export type ImportMode = 'index_in_place' | 'consolidate';

export interface ActiveImport {
  importId: number;
  sourceId: number;
  sourceName: string;
  mode: ImportMode;
  total: number;
  done: number;
  currentFile: string;
  etaSeconds: number | null;
  finished: boolean;
}

interface ImportState {
  active: Map<number, ActiveImport>;
  /** Register a newly started import so progress events find a row. */
  register: (row: Omit<ActiveImport, 'total' | 'done' | 'currentFile' | 'etaSeconds' | 'finished'>) => void;
  /** Called by the event listener for every progress tick. */
  applyProgress: (e: ImportProgressEvent) => void;
  /** Remove a finished import from the visible list (e.g. after the user dismisses it). */
  dismiss: (importId: number) => void;
  /** Drop every entry (used by tests). */
  reset: () => void;
}

export const useImportStore = create<ImportState>((set) => ({
  active: new Map(),
  register: (row) =>
    set((s) => {
      const next = new Map(s.active);
      const prev = next.get(row.importId);
      next.set(row.importId, {
        ...row,
        total: prev?.total ?? 0,
        done: prev?.done ?? 0,
        currentFile: prev?.currentFile ?? '',
        etaSeconds: prev?.etaSeconds ?? null,
        finished: prev?.finished ?? false,
      });
      return { active: next };
    }),
  applyProgress: (e) =>
    set((s) => {
      const existing = s.active.get(e.import_id);
      if (!existing) {
        // Progress for an import we never registered (e.g. app restart
        // mid-import) — create a minimal row so the UI still shows it.
        const next = new Map(s.active);
        next.set(e.import_id, {
          importId: e.import_id,
          sourceId: e.source_id,
          sourceName: `Source ${e.source_id}`,
          mode: 'index_in_place',
          total: e.total,
          done: e.done,
          currentFile: e.current_file,
          etaSeconds: e.eta_seconds ?? null,
          finished: e.done >= e.total && e.total > 0,
        });
        return { active: next };
      }
      const next = new Map(s.active);
      next.set(e.import_id, {
        ...existing,
        total: e.total,
        done: e.done,
        currentFile: e.current_file,
        etaSeconds: e.eta_seconds ?? null,
        finished: e.done >= e.total && e.total > 0,
      });
      return { active: next };
    }),
  dismiss: (importId) =>
    set((s) => {
      const next = new Map(s.active);
      next.delete(importId);
      return { active: next };
    }),
  reset: () => set({ active: new Map() }),
}));

/**
 * Convenience selector that returns active imports as a sorted array. Sort is
 * stable by importId ascending.
 *
 * We deliberately subscribe to `s.active` (a reference that only changes when
 * the store mutates) and derive the sorted array in a `useMemo`. Returning a
 * freshly-sorted array from the selector itself would trip Zustand's
 * identity check on every render and infinite-loop.
 */
export function useActiveImports(): ActiveImport[] {
  const active = useImportStore((s) => s.active);
  return useMemo(() => [...active.values()].sort((a, b) => a.importId - b.importId), [active]);
}

/**
 * Mount this hook ONCE at the React root (not per-screen). It installs a
 * Tauri event listener that feeds every progress tick into the store.
 * Returns no value; unlisten happens on unmount.
 */
export function useImportProgressListener() {
  const applyProgress = useImportStore((s) => s.applyProgress);
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      try {
        const u = await listen<ImportProgressEvent>(IMPORT_PROGRESS_EVENT, (evt) => {
          applyProgress(evt.payload);
        });
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      } catch {
        // Tauri bridge unavailable (e.g. vitest env) — silently no-op.
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [applyProgress]);
}
