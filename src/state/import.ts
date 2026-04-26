/**
 * Global import-progress store.
 *
 * A single Zustand store holds a `Map<importId, ActiveImport>` populated by a
 * listener on `IMPORT_PROGRESS_EVENT`. The listener is mounted once at the
 * React root via `useImportProgressListener()` so every screen (catalog
 * sidebar, status bar, etc.) sees the same state.
 *
 * Every import copies into the catalog (there is no "index in place" mode).
 * The `deleteAfterCopy` flag on each import is captured at start time and
 * read by the post-import auto-chain: once the lift-and-shift finishes
 * verifying + writing the catalog copies, the originals on the source
 * disk are recycled if the flag is set.
 */

import { useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useMemo, useRef } from 'react';
import { create } from 'zustand';
import {
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  liftShiftDryRun,
  liftShiftExecute,
  recycleSourceFilesAfterCopy,
} from '../tauri/invoke';
import { debug, warn } from '../util/log';

/**
 * Legacy shape kept only so older persisted `default_import_mode`
 * values deserialize cleanly — the app ignores the variant and always
 * copies into the catalog now.
 */
export type ImportMode = 'consolidate';

export interface ActiveImport {
  importId: number;
  sourceId: number;
  sourceName: string;
  mode: ImportMode;
  /** When true, the source files are recycled after the post-import
   *  lift-and-shift verifies the catalog copies. */
  deleteAfterCopy: boolean;
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
      const wasFinished = existing?.finished ?? false;
      // The backend tags its final tick with `finished: true`. Don't try
      // to infer "done" from `done == total` — empty imports legitimately
      // end with `done=0, total=0`, which is also the initial state.
      const nowFinished = e.finished;
      if (!wasFinished && nowFinished) {
        // Side-effect: once an import flips to finished, kick off the
        // post-import auto-chain (lift-shift + optional source recycle).
        // Done via setTimeout so we don't fire inside a Zustand set().
        schedulePostImportChain(existing?.sourceId ?? e.source_id, existing?.deleteAfterCopy ?? false);
      }
      if (!existing) {
        // Progress for an import we never registered (e.g. app restart
        // mid-import) — create a minimal row so the UI still shows it.
        const next = new Map(s.active);
        next.set(e.import_id, {
          importId: e.import_id,
          sourceId: e.source_id,
          sourceName: `Source ${e.source_id}`,
          mode: 'consolidate',
          deleteAfterCopy: false,
          total: e.total,
          done: e.done,
          currentFile: e.current_file,
          etaSeconds: e.eta_seconds ?? null,
          finished: nowFinished,
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
        finished: nowFinished,
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
 * Queue the post-import auto-chain: lift-and-shift the just-imported
 * source's photos into the catalog, then (if `deleteAfterCopy`) recycle
 * the original source files that now have a verified catalog copy.
 *
 * Scheduled via `setTimeout` so it fires outside the current Zustand
 * `set()` transaction. Fire-and-forget — errors surface as tracing
 * warnings, never as unhandled rejections.
 */
function schedulePostImportChain(sourceId: number, deleteAfterCopy: boolean): void {
  setTimeout(() => {
    runPostImportChain(sourceId, deleteAfterCopy).catch((err) => warn('post-import chain failed', err));
  }, 0);
}

/**
 * Run the catalog lift-and-shift for every unlifted source copy, then
 * (optionally) recycle the original source files. Designed to be safe
 * to run multiple times: `plan_lift` skips photos that already live
 * under the catalog root, and `recycle_source_files_after_copy`
 * requires a verified surviving copy before it recycles anything.
 *
 * The catalog root comes from the user's `default_catalog_path` via
 * `lift_shift_dry_run` itself — we don't pass it explicitly so existing
 * user-picked overrides in Settings are respected.
 */
async function runPostImportChain(sourceId: number, deleteAfterCopy: boolean): Promise<void> {
  // Resolve the catalog root lazily so tests that mock the import store
  // without booting Tauri don't need to stub this path.
  const { useUi } = await import('./ui');
  const catalogRoot =
    useUi.getState().tweaks.cachePath ??
    (await import('../tauri/invoke').then((m) => m.getDefaultCatalogPath()).catch(() => null));
  if (!catalogRoot) {
    warn('post-import chain: no catalog root resolved, skipping lift-shift');
    return;
  }
  const plan = await liftShiftDryRun(catalogRoot);
  if (plan.total_file_count === 0) {
    debug('post-import chain: lift-shift plan empty (nothing to copy)');
  } else {
    await liftShiftExecute(plan.plan_id, plan.confirm_token);
  }
  if (deleteAfterCopy) {
    const receipt = await recycleSourceFilesAfterCopy(sourceId);
    debug('post-import chain: recycle', receipt);
  }
}

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
 * Tauri event listener that feeds every progress tick into the store and
 * (throttled) invalidates catalog queries so the grid, side panel, and
 * rediscovery rows update live as the import advances.
 *
 * Catalog refresh policy:
 * - Every ~800 ms while an import is in flight, invalidate `photos` +
 *   `albums` + `rediscovery`. That's enough to stream new rows into the
 *   grid without thrashing React Query on every per-photo tick.
 * - On the moment an import crosses done == total, force an immediate
 *   invalidation (+ invalidate `face-clusters`, since reeval runs at
 *   that point too).
 */
export function useImportProgressListener() {
  const applyProgress = useImportStore((s) => s.applyProgress);
  const qc = useQueryClient();
  const lastInvalidateAt = useRef<number>(0);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    function refreshCatalog() {
      qc.invalidateQueries({ queryKey: ['photos'] });
      qc.invalidateQueries({ queryKey: ['albums'] });
      qc.invalidateQueries({ queryKey: ['rediscovery'] });
    }

    (async () => {
      try {
        const u = await listen<ImportProgressEvent>(IMPORT_PROGRESS_EVENT, (evt) => {
          const prev = useImportStore.getState().active.get(evt.payload.import_id);
          const wasFinished = prev?.finished ?? false;
          applyProgress(evt.payload);

          const nowFinished = evt.payload.finished;

          // Throttled live refresh while photos stream in.
          const now = Date.now();
          if (now - lastInvalidateAt.current >= 800) {
            lastInvalidateAt.current = now;
            refreshCatalog();
          }

          // On the finish transition, force an immediate refresh +
          // invalidate cluster/face views (post-import reeval_clusters
          // runs on the backend at this moment).
          if (!wasFinished && nowFinished) {
            lastInvalidateAt.current = now;
            refreshCatalog();
            qc.invalidateQueries({ queryKey: ['face-clusters'] });
            qc.invalidateQueries({ queryKey: ['sources'] });
          }
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
  }, [applyProgress, qc]);
}
