/**
 * Global source-disconnect progress store.
 *
 * Mirrors `state/import.ts` but for the `delete_source` command. The backend
 * emits `SOURCE_DELETE_PROGRESS_EVENT` ticks at phase boundaries (collecting,
 * deleting, committed, thumb_cleanup, recycling, done). The listener feeds
 * them into a Zustand store keyed by source_id, and on the `committed` phase
 * it invalidates `photos`-keyed queries so the catalog grid empties the
 * moment orphan rows leave the DB — even if recycle-bin work below takes
 * longer.
 */

import { useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useMemo } from 'react';
import { create } from 'zustand';
import {
  SOURCE_DELETE_PROGRESS_EVENT,
  type SourceDeletePhase,
  type SourceDeleteProgressEvent,
} from '../tauri/invoke';
import { resetCatalogContentQueries } from './queryInvalidation';

export interface ActiveSourceDelete {
  sourceId: number;
  sourceName: string;
  phase: SourceDeletePhase;
  total: number;
  done: number;
  finished: boolean;
}

interface SourceDeleteState {
  active: Map<number, ActiveSourceDelete>;
  /** Register a disconnect right after the user confirms, so the card shows
   *  even before the first backend event arrives. */
  register: (sourceId: number, sourceName: string) => void;
  applyProgress: (e: SourceDeleteProgressEvent) => void;
  /** Drop a finished entry (auto-called shortly after `done`). */
  dismiss: (sourceId: number) => void;
  reset: () => void;
}

export const useSourceDeleteStore = create<SourceDeleteState>((set) => ({
  active: new Map(),
  register: (sourceId, sourceName) =>
    set((s) => {
      const next = new Map(s.active);
      const prev = next.get(sourceId);
      next.set(sourceId, {
        sourceId,
        sourceName,
        phase: prev?.phase ?? 'collecting',
        total: prev?.total ?? 0,
        done: prev?.done ?? 0,
        finished: false,
      });
      return { active: next };
    }),
  applyProgress: (e) =>
    set((s) => {
      const existing = s.active.get(e.source_id);
      const next = new Map(s.active);
      next.set(e.source_id, {
        sourceId: e.source_id,
        sourceName: existing?.sourceName ?? `Source ${e.source_id}`,
        phase: e.phase,
        total: e.total,
        done: e.done,
        finished: e.phase === 'done',
      });
      return { active: next };
    }),
  dismiss: (sourceId) =>
    set((s) => {
      const next = new Map(s.active);
      next.delete(sourceId);
      return { active: next };
    }),
  reset: () => set({ active: new Map() }),
}));

/** Sorted-by-sourceId selector for stable rendering. */
export function useActiveSourceDeletes(): ActiveSourceDelete[] {
  const active = useSourceDeleteStore((s) => s.active);
  return useMemo(() => [...active.values()].sort((a, b) => a.sourceId - b.sourceId), [active]);
}

/**
 * Mount once at the React root inside the QueryClient provider. Listens for
 * backend disconnect-progress events, feeds them into the store, and
 * invalidates catalog queries at the right moments:
 *
 *   - `committed`: orphan photo rows are gone — invalidate `photos` /
 *     `albums` / `rediscovery` so the grid empties immediately.
 *   - `done`: invalidate `sources` / `imports` / `cleanup` / `face-clusters`
 *     to refresh the sidebar counts and dependent views, then schedule the
 *     entry for dismissal.
 */
export function useSourceDeleteProgressListener() {
  const applyProgress = useSourceDeleteStore((s) => s.applyProgress);
  const dismiss = useSourceDeleteStore((s) => s.dismiss);
  const qc = useQueryClient();

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    (async () => {
      try {
        const u = await listen<SourceDeleteProgressEvent>(SOURCE_DELETE_PROGRESS_EVENT, (evt) => {
          applyProgress(evt.payload);
          if (evt.payload.phase === 'committed') {
            resetCatalogContentQueries(qc);
          }
          if (evt.payload.phase === 'done') {
            resetCatalogContentQueries(qc);
            qc.invalidateQueries({ queryKey: ['cleanup'] });
            // Let the user see the "done" tick briefly, then clear it.
            const sourceId = evt.payload.source_id;
            setTimeout(() => dismiss(sourceId), 2000);
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
  }, [applyProgress, dismiss, qc]);
}
