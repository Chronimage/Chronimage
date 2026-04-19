import { create } from 'zustand';

export type ScreenId = 'onboard' | 'catalog' | 'cull' | 'cullbin' | 'develop' | 'settings';

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
};

interface UiState {
  screen: Screen;
  tweaks: Tweaks;
  setScreen: (id: ScreenId) => void;
  setTweaks: (partial: Partial<Tweaks>) => void;
}

export const useUi = create<UiState>((set) => ({
  screen: SCREENS.onboard,
  tweaks: DEFAULT_TWEAKS,
  setScreen: (id) => set({ screen: SCREENS[id] }),
  setTweaks: (partial) => set((s) => ({ tweaks: { ...s.tweaks, ...partial } })),
}));
