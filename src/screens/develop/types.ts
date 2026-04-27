export type DevelopTab = 'develop' | 'mask' | 'prompt';

export type PresetCategory = 'face' | 'scene' | 'quality' | 'style' | 'custom';

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
  cropX: number;
  cropY: number;
  cropW: number;
  cropH: number;
  rotation: number;
  straighten: number;
  transformH: number;
  transformV: number;
  lensDistortion: number;
  lensVignette: number;
  chromaticAberration: number;
  spotHealCount: number;
  lensBlurAmount: number;
  lensBlurFocusNear: number;
  lensBlurFocusFar: number;
  lensBlurBokehBoost: number;
  lensBlurCatEye: number;
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
  cropX: 0,
  cropY: 0,
  cropW: 100,
  cropH: 100,
  rotation: 0,
  straighten: 0,
  transformH: 0,
  transformV: 0,
  lensDistortion: 0,
  lensVignette: 0,
  chromaticAberration: 0,
  spotHealCount: 0,
  lensBlurAmount: 0,
  lensBlurFocusNear: 0,
  lensBlurFocusFar: 100,
  lensBlurBokehBoost: 0,
  lensBlurCatEye: 0,
  curves: identityCurves(),
};

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function defaultDevelopValues(): DevelopValues {
  return operationsToValues(identityOperations());
}

/** Convert UI slider shorthand → backend Operations shape. The UI caps
 * `exposure` at -100..=100; backend expects EV stops (roughly -4..=4).
 * Empirical map: UI slider × 0.04 = EV. Everything else passes through. */
export function valuesToOperations(v: DevelopValues): DevelopOperations {
  const cropW = clamp(v.cropW / 100, 0.05, 1);
  const cropH = clamp(v.cropH / 100, 0.05, 1);
  const cropX = clamp(v.cropX / 100, 0, 1 - cropW);
  const cropY = clamp(v.cropY / 100, 0, 1 - cropH);
  const focusNear = clamp(v.lensBlurFocusNear / 100, 0, 1);
  const focusFar = clamp(v.lensBlurFocusFar / 100, 0, 1);
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
    crop_x: cropX,
    crop_y: cropY,
    crop_w: cropW,
    crop_h: cropH,
    rotation: v.rotation,
    straighten: v.straighten,
    transform_h: v.transformH,
    transform_v: v.transformV,
    lens_distortion: v.lensDistortion,
    lens_vignette: v.lensVignette,
    chromatic_aberration: v.chromaticAberration,
    spot_heal_count: v.spotHealCount,
    lens_blur_amount: clamp(v.lensBlurAmount, 0, 100),
    lens_blur_focus_near: Math.min(focusNear, focusFar),
    lens_blur_focus_far: Math.max(focusNear, focusFar),
    lens_blur_bokeh_boost: clamp(v.lensBlurBokehBoost, 0, 100),
    lens_blur_cat_eye: clamp(v.lensBlurCatEye, 0, 100),
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
    cropX: (ops.crop_x ?? 0) * 100,
    cropY: (ops.crop_y ?? 0) * 100,
    cropW: (ops.crop_w ?? 1) * 100,
    cropH: (ops.crop_h ?? 1) * 100,
    rotation: ops.rotation ?? 0,
    straighten: ops.straighten ?? 0,
    transformH: ops.transform_h ?? 0,
    transformV: ops.transform_v ?? 0,
    lensDistortion: ops.lens_distortion ?? 0,
    lensVignette: ops.lens_vignette ?? 0,
    chromaticAberration: ops.chromatic_aberration ?? 0,
    spotHealCount: ops.spot_heal_count ?? 0,
    lensBlurAmount: ops.lens_blur_amount ?? 0,
    lensBlurFocusNear: (ops.lens_blur_focus_near ?? 0) * 100,
    lensBlurFocusFar: (ops.lens_blur_focus_far ?? 1) * 100,
    lensBlurBokehBoost: ops.lens_blur_bokeh_boost ?? 0,
    lensBlurCatEye: ops.lens_blur_cat_eye ?? 0,
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
    crop_x: lerp(base.crop_x ?? 0, target.crop_x ?? 0),
    crop_y: lerp(base.crop_y ?? 0, target.crop_y ?? 0),
    crop_w: lerp(base.crop_w ?? 1, target.crop_w ?? 1),
    crop_h: lerp(base.crop_h ?? 1, target.crop_h ?? 1),
    rotation: lerp(base.rotation ?? 0, target.rotation ?? 0),
    straighten: lerp(base.straighten ?? 0, target.straighten ?? 0),
    transform_h: lerp(base.transform_h ?? 0, target.transform_h ?? 0),
    transform_v: lerp(base.transform_v ?? 0, target.transform_v ?? 0),
    lens_distortion: lerp(base.lens_distortion ?? 0, target.lens_distortion ?? 0),
    lens_vignette: lerp(base.lens_vignette ?? 0, target.lens_vignette ?? 0),
    chromatic_aberration: lerp(base.chromatic_aberration ?? 0, target.chromatic_aberration ?? 0),
    spot_heal_count: lerp(base.spot_heal_count ?? 0, target.spot_heal_count ?? 0),
    lens_blur_amount: lerp(base.lens_blur_amount ?? 0, target.lens_blur_amount ?? 0),
    lens_blur_focus_near: lerp(base.lens_blur_focus_near ?? 0, target.lens_blur_focus_near ?? 0),
    lens_blur_focus_far: lerp(base.lens_blur_focus_far ?? 1, target.lens_blur_focus_far ?? 1),
    lens_blur_bokeh_boost: lerp(base.lens_blur_bokeh_boost ?? 0, target.lens_blur_bokeh_boost ?? 0),
    lens_blur_cat_eye: lerp(base.lens_blur_cat_eye ?? 0, target.lens_blur_cat_eye ?? 0),
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
