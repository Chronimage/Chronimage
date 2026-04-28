export const MASK_PRESETS = [
  { id: 'subject', label: 'Subject', icon: 'faces' as const },
  { id: 'sky', label: 'Sky', icon: 'cloud' as const },
  { id: 'person', label: 'Person', icon: 'faces' as const },
  { id: 'object', label: 'Objects', icon: 'wand' as const },
  { id: 'foreground', label: 'Foreground', icon: 'layers' as const },
  { id: 'background', label: 'Background', icon: 'grid' as const },
];

export type MaskPreset = (typeof MASK_PRESETS)[number];
export type MaskMode = 'normal' | 'add' | 'subtract' | 'intersect';

export const MASK_MODES: { id: MaskMode; label: string }[] = [
  { id: 'normal', label: 'New' },
  { id: 'add', label: 'Add' },
  { id: 'subtract', label: 'Subtract' },
  { id: 'intersect', label: 'Intersect' },
];
