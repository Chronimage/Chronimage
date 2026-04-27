export type DevelopTab = 'develop' | 'mask' | 'prompt';

export type PresetCategory = 'face' | 'scene' | 'quality' | 'style' | 'custom';

export interface Preset {
  id: string;
  name: string;
  sub: string;
  group: 'Face' | 'Scene' | 'Quality' | 'Style';
}

import {
  type DevelopCurve,
  type DevelopCurves,
  type DevelopOperations,
  identityCurve,
  identityCurves,
  identityOperations,
} from '../../tauri/invoke';

export interface DevelopValues {
  exp: number;
  con: number;
  hi: number;
  sh: number;
  whites: number;
  blacks: number;
  temp: number;
  tint: number;
  vib: number;
  sat: number;
  clarity: number;
  dehaze: number;
  curves: DevelopCurves;
}

export const DEFAULT_DEVELOP_VALUES: DevelopValues = {
  exp: 0,
  con: 0,
  hi: 0,
  sh: 0,
  whites: 0,
  blacks: 0,
  temp: 0,
  tint: 0,
  vib: 0,
  sat: 0,
  clarity: 0,
  dehaze: 0,
  curves: identityCurves(),
};

export function defaultDevelopValues(): DevelopValues {
  return operationsToValues(identityOperations());
}

/** Convert UI slider shorthand → backend Operations shape. The UI caps
 * `exposure` at -100..=100; backend expects EV stops (roughly -4..=4).
 * Empirical map: UI slider × 0.04 = EV. Everything else passes through. */
export function valuesToOperations(v: DevelopValues): DevelopOperations {
  return {
    exposure: v.exp * 0.04,
    contrast: v.con,
    highlights: v.hi,
    shadows: v.sh,
    whites: v.whites,
    blacks: v.blacks,
    temp: v.temp,
    tint: v.tint,
    vibrance: v.vib,
    saturation: v.sat,
    clarity: v.clarity,
    dehaze: v.dehaze,
    curves: v.curves,
  };
}

export function operationsToValues(ops: DevelopOperations): DevelopValues {
  return {
    exp: ops.exposure / 0.04,
    con: ops.contrast,
    hi: ops.highlights,
    sh: ops.shadows,
    whites: ops.whites,
    blacks: ops.blacks,
    temp: ops.temp,
    tint: ops.tint,
    vib: ops.vibrance,
    sat: ops.saturation,
    clarity: ops.clarity,
    dehaze: ops.dehaze,
    curves: ops.curves ?? identityCurves(),
  };
}

export function normaliseOperations(ops: Partial<DevelopOperations> | null | undefined): DevelopOperations {
  const base = identityOperations();
  return {
    ...base,
    ...ops,
    curves: {
      ...base.curves,
      ...(ops?.curves ?? {}),
    },
  };
}

export function parseOperationsJson(raw: string): DevelopOperations | null {
  try {
    return normaliseOperations(JSON.parse(raw) as Partial<DevelopOperations>);
  } catch {
    return null;
  }
}

export function blendOperations(
  base: DevelopOperations,
  target: DevelopOperations,
  strength: number,
): DevelopOperations {
  const t = Math.min(100, Math.max(0, strength)) / 100;
  const lerp = (a: number, b: number) => a + (b - a) * t;
  return {
    exposure: lerp(base.exposure, target.exposure),
    contrast: lerp(base.contrast, target.contrast),
    highlights: lerp(base.highlights, target.highlights),
    shadows: lerp(base.shadows, target.shadows),
    whites: lerp(base.whites, target.whites),
    blacks: lerp(base.blacks, target.blacks),
    temp: lerp(base.temp, target.temp),
    tint: lerp(base.tint, target.tint),
    vibrance: lerp(base.vibrance, target.vibrance),
    saturation: lerp(base.saturation, target.saturation),
    clarity: lerp(base.clarity, target.clarity),
    dehaze: lerp(base.dehaze, target.dehaze),
    curves: blendCurves(base.curves, target.curves, t),
  };
}

function blendCurves(base: DevelopCurves, target: DevelopCurves, t: number): DevelopCurves {
  return {
    rgb: blendCurve(base.rgb, target.rgb, t),
    r: blendCurve(base.r, target.r, t),
    g: blendCurve(base.g, target.g, t),
    b: blendCurve(base.b, target.b, t),
    l: blendCurve(base.l, target.l, t),
  };
}

function blendCurve(
  base: DevelopCurve | undefined,
  target: DevelopCurve | undefined,
  t: number,
): DevelopCurve {
  const a = base ?? identityCurve();
  const b = target ?? identityCurve();
  if (a.length !== b.length) return cloneCurve(t < 0.5 ? a : b);
  return a.map((p, i) => {
    const q = b[i] ?? p;
    return [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
  });
}

function cloneCurve(curve: DevelopCurve): DevelopCurve {
  return curve.map((p) => [p[0], p[1]]);
}

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
