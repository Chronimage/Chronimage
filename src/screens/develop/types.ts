export type DevelopTab = 'develop' | 'mask' | 'prompt';

export type PresetCategory = 'face' | 'scene' | 'quality' | 'style' | 'custom';

export interface Preset {
  id: string;
  name: string;
  sub: string;
  group: 'Face' | 'Scene' | 'Quality' | 'Style';
}

export interface DevelopValues {
  exp: number;
  con: number;
  hi: number;
  sh: number;
  temp: number;
  tint: number;
  vib: number;
  sat: number;
  clarity: number;
  dehaze: number;
}

export const DEFAULT_DEVELOP_VALUES: DevelopValues = {
  exp: 0,
  con: 0,
  hi: 0,
  sh: 0,
  temp: 0,
  tint: 0,
  vib: 0,
  sat: 0,
  clarity: 0,
  dehaze: 0,
};

export const PRESETS: Preset[] = [
  { id: 'p-1', name: 'Clean up face', sub: 'Smooth skin · keep detail', group: 'Face' },
  { id: 'p-2', name: 'Whiten teeth', sub: 'Subtle L channel lift', group: 'Face' },
  { id: 'p-3', name: 'Eye pop', sub: 'Sharpen + saturate iris', group: 'Face' },
  { id: 'p-4', name: 'Enhance sky', sub: 'Deeper blues, crisper clouds', group: 'Scene' },
  { id: 'p-5', name: 'Golden hour', sub: 'Warm highlights, soft shadows', group: 'Scene' },
  { id: 'p-6', name: 'Urban night', sub: 'Cool shadows, neon pop', group: 'Scene' },
  { id: 'p-7', name: 'Denoise low-light', sub: 'ISO-aware smoothing', group: 'Quality' },
  { id: 'p-8', name: 'Recover shadows', sub: 'Lift dark regions', group: 'Quality' },
  { id: 'p-9', name: 'Moody portrait', sub: 'Desaturate + contrast', group: 'Style' },
  { id: 'p-10', name: 'Faded film', sub: 'Lifted blacks, grain', group: 'Style' },
];
