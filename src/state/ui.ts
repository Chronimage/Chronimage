import { create } from 'zustand';
import { loadPersisted, savePersisted } from '../util/store';

export type ScreenId = 'onboard' | 'catalog' | 'cull' | 'cullbin' | 'develop' | 'people' | 'settings';

export interface Screen {
  id: ScreenId;
  label: string;
}

export const SCREENS: Record<ScreenId, Screen> = {
  onboard: { id: 'onboard', label: 'Sources' },
  catalog: { id: 'catalog', label: 'Catalog' },
  cull: { id: 'cull', label: 'Cull' },
  cullbin: { id: 'cullbin', label: 'Cull Bin' },
  develop: { id: 'develop', label: 'Develop' },
  people: { id: 'people', label: 'People' },
  settings: { id: 'settings', label: 'Settings' },
};

export interface Tweaks {
  theme: 'dark' | 'light';
  accent: 'mint' | 'ember' | 'violet' | 'sky' | 'gold';
  displayFont: 'Instrument Serif' | 'Fraunces' | 'Inter Tight';
  gridDensity: 'compact' | 'comfortable' | 'spacious';
  facetPlacement: 'left' | 'bottom';
  cullMode: 'compare' | 'grid' | 'swipe';
  editorLayout: 'right-panel' | 'left-panel';
  appName: string;
  // Persisted Settings screen controls (PRD §14).
  dupeSimilarity: number;
  sharpnessCutoff: number;
  requireReview: boolean;
  nightlyReindex: boolean;
}

export const DEFAULT_TWEAKS: Tweaks = {
  theme: 'dark',
  accent: 'mint',
  displayFont: 'Instrument Serif',
  gridDensity: 'compact',
  facetPlacement: 'left',
  cullMode: 'compare',
  editorLayout: 'right-panel',
  appName: 'Chronimage',
  dupeSimilarity: 85,
  sharpnessCutoff: 32,
  requireReview: true,
  nightlyReindex: true,
};

const TWEAKS_STORE_KEY = 'tweaks';

interface UiState {
  screen: Screen;
  tweaks: Tweaks;
  /** True once `hydrateFromStore()` has completed (even if no persisted value was found). */
  hydrated: boolean;
  setScreen: (id: ScreenId) => void;
  setTweaks: (partial: Partial<Tweaks>) => void;
  /** Read persisted tweaks from `@tauri-apps/plugin-store` and merge. Idempotent. */
  hydrateFromStore: () => Promise<void>;
}

export const useUi = create<UiState>((set, get) => ({
  screen: SCREENS.onboard,
  tweaks: DEFAULT_TWEAKS,
  hydrated: false,
  setScreen: (id) => set({ screen: SCREENS[id] }),
  setTweaks: (partial) => {
    set((s) => {
      const next = { ...s.tweaks, ...partial };
      // Fire-and-forget: errors are logged inside `savePersisted`.
      savePersisted<Tweaks>(TWEAKS_STORE_KEY, next);
      return { tweaks: next };
    });
  },
  hydrateFromStore: async () => {
    if (get().hydrated) return;
    const loaded = await loadPersisted<Partial<Tweaks>>(TWEAKS_STORE_KEY, {});
    set({
      tweaks: { ...DEFAULT_TWEAKS, ...loaded },
      hydrated: true,
    });
  },
}));
