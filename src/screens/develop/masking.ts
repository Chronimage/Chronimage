/// Lightroom-style mask preset row. The order here drives the order
/// users see in the side panel; AI-generated kinds come first so the
/// most common workflow ("mask the subject", "darken the sky") is one
/// click away. The pixel-driven kinds (color / luminance range) are at
/// the end because they require an extra interaction (eyedropper /
/// slider) before producing a mask.
///
/// `availability` distinguishes:
///  - `ready`     — fully wired, click to generate
///  - `disabled`  — UI surfaces the button but it can't be used yet
///                  (e.g. depth range needs a model that's not bundled).
///                  Kept visible so the existence of the feature is
///                  discoverable rather than secret.
export const MASK_PRESETS = [
  { id: 'subject', label: 'Subject', icon: 'faces' as const, availability: 'ready' as const },
  { id: 'sky', label: 'Sky', icon: 'cloud' as const, availability: 'ready' as const },
  { id: 'background', label: 'Background', icon: 'grid' as const, availability: 'ready' as const },
  { id: 'person', label: 'Person', icon: 'faces' as const, availability: 'ready' as const },
  { id: 'landscape', label: 'Landscape', icon: 'layers' as const, availability: 'ready' as const },
  { id: 'object', label: 'Objects', icon: 'wand' as const, availability: 'ready' as const },
  { id: 'foreground', label: 'Foreground', icon: 'layers' as const, availability: 'ready' as const },
  { id: 'color_range', label: 'Color Range', icon: 'wand' as const, availability: 'ready' as const },
  {
    id: 'luminance_range',
    label: 'Luminance',
    icon: 'wand' as const,
    availability: 'ready' as const,
  },
  {
    id: 'depth_range',
    label: 'Depth',
    icon: 'layers' as const,
    availability: 'disabled' as const,
    disabledReason: 'Depth model is not bundled yet — coming in a follow-up release.',
  },
] as const;

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
