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

// Short labels keep the 4-button mode-seg readable inside the 252-px
// sidepanel column. Full names live in `aria-label` and the per-mode
// tooltip via `title`.
export interface MaskModeOption {
  id: MaskMode;
  label: string;
  full: string;
}
export const MASK_MODES: MaskModeOption[] = [
  { id: 'normal', label: 'New', full: 'New mask' },
  { id: 'add', label: 'Add', full: 'Add to mask' },
  { id: 'subtract', label: 'Sub', full: 'Subtract from mask' },
  { id: 'intersect', label: 'Int', full: 'Intersect with mask' },
];
