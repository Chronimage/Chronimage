/**
 * UI-only state for the Cull screen. Holds the current mode, workflow view,
 * pair index, and in-session kept/rejected tallies.
 */

import { create } from 'zustand';
import type { CullMode, CullVerdict } from '../screens/cull/types';

export type CullView = 'review' | 'rejected' | 'cleanup';

interface CullState {
  mode: CullMode;
  view: CullView;
  idx: number;
  kept: number;
  rejected: number;
  setMode: (mode: CullMode) => void;
  setView: (view: CullView) => void;
  next: () => void;
  prev: () => void;
  recordVerdict: (verdict: CullVerdict) => void;
  reset: () => void;
}

export const useCull = create<CullState>((set) => ({
  mode: 'compare',
  view: 'review',
  idx: 0,
  kept: 0,
  rejected: 0,
  setMode: (mode) => set({ mode }),
  setView: (view) => set({ view }),
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
  reset: () =>
    set({
      mode: 'compare',
      view: 'review',
      idx: 0,
      kept: 0,
      rejected: 0,
    }),
}));

function verdictDelta(verdict: CullVerdict): { kept: number; rejected: number } {
  if (verdict === 'reject_both') return { kept: 0, rejected: 2 };
  if (verdict === 'reject_a' || verdict === 'reject_b') return { kept: 1, rejected: 1 };
  if (verdict === 'accept_ai') return { kept: 1, rejected: 1 };
  return { kept: 1, rejected: 0 };
}
