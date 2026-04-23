/**
 * UI-only state for the Cull screen stub. Holds the mode + pair index + kept/
 * rejected tallies + issue-filter toggles. Persists nothing — a real verdict
 * engine (Phase 2 §2) writes to `cull_verdicts` in SQLite; this store is a
 * placeholder for the in-session interaction state.
 */

import { create } from 'zustand';
import type { CullMode, CullVerdict } from '../screens/cull/types';

interface CullState {
  mode: CullMode;
  idx: number;
  kept: number;
  rejected: number;
  activeFilters: Set<string>;
  setMode: (mode: CullMode) => void;
  next: () => void;
  prev: () => void;
  recordVerdict: (verdict: CullVerdict) => void;
  toggleFilter: (filter: string) => void;
  reset: () => void;
}

export const useCull = create<CullState>((set) => ({
  mode: 'compare',
  idx: 0,
  kept: 0,
  rejected: 0,
  activeFilters: new Set<string>(['Near-duplicates', 'Out of focus', 'Eyes closed']),
  setMode: (mode) => set({ mode }),
  next: () => set((s) => ({ idx: s.idx + 1 })),
  prev: () => set((s) => ({ idx: Math.max(0, s.idx - 1) })),
  recordVerdict: (verdict) =>
    set((s) => {
      const delta = verdictDelta(verdict);
      return {
        idx: s.idx + 1,
        kept: s.kept + delta.kept,
        rejected: s.rejected + delta.rejected,
      };
    }),
  toggleFilter: (filter) =>
    set((s) => {
      const next = new Set(s.activeFilters);
      if (next.has(filter)) next.delete(filter);
      else next.add(filter);
      return { activeFilters: next };
    }),
  reset: () =>
    set({
      mode: 'compare',
      idx: 0,
      kept: 0,
      rejected: 0,
      activeFilters: new Set(['Near-duplicates', 'Out of focus', 'Eyes closed']),
    }),
}));

function verdictDelta(verdict: CullVerdict): { kept: number; rejected: number } {
  if (verdict === 'reject_both') return { kept: 0, rejected: 2 };
  if (verdict === 'reject_a' || verdict === 'reject_b') return { kept: 1, rejected: 1 };
  if (verdict === 'accept_ai') return { kept: 1, rejected: 1 };
  return { kept: 1, rejected: 0 };
}
