import { create } from 'zustand';
import { loadPersisted, savePersisted } from '../util/store';

export type ScreenId = 'catalog' | 'cull' | 'cullbin' | 'develop' | 'people' | 'settings';

export interface Screen {
  id: ScreenId;
  label: string;
}

export const SCREENS: Record<ScreenId, Screen> = {
  catalog: { id: 'catalog', label: 'Catalog' },
  cull: { id: 'cull', label: 'Cull' },
  cullbin: { id: 'cullbin', label: 'Cull Bin' },
  develop: { id: 'develop', label: 'Develop' },
  people: { id: 'people', label: 'People' },
  settings: { id: 'settings', label: 'Settings' },
};

export type PhotoSortBy =
  | 'captured_desc'
  | 'captured_asc'
  | 'imported_desc'
  | 'filename_asc'
  | 'aesthetic_desc'
  | 'random';

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
  /** Absolute path to thumbnail / derivative cache. `null` = backend default (under catalog root). */
  cachePath: string | null;
  /** Release channel the user opts into for auto-updates. */
  preferredChannel: 'stable' | 'beta' | 'nightly' | 'insider';
  /** Catalog grid sort order. Persisted so the user's choice survives reload. */
  sortBy: PhotoSortBy;
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
  cachePath: null,
  preferredChannel: 'stable',
  sortBy: 'captured_desc',
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
  screen: SCREENS.catalog,
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
