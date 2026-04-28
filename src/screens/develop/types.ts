export type DevelopTab = 'develop' | 'mask' | 'prompt';

export type PresetCategory = 'face' | 'scene' | 'quality' | 'style' | 'custom';

import {
  type ColorGrading,
  type ColorMixer,
  type Defringe,
  type DevelopCurve,
  type DevelopCurves,
  type DevelopOperations,
  type Grain,
  type HslAdjust,
  type HslWheel,
  identityColorGrading,
  identityColorMixer,
  identityCurve,
  identityCurves,
  identityDefringe,
  identityGrain,
  identityOperations,
  identitySharpening,
  type Sharpening,
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
  texture: number;
  sharpening: Sharpening;
  grain: Grain;
  colorMixer: ColorMixer;
  colorGrading: ColorGrading;
  defringe: Defringe;
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
  texture: 0,
  sharpening: identitySharpening(),
  grain: identityGrain(),
  colorMixer: identityColorMixer(),
  colorGrading: identityColorGrading(),
  defringe: identityDefringe(),
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
    texture: v.texture,
    sharpening: v.sharpening,
    grain: v.grain,
    color_mixer: v.colorMixer,
    color_grading: v.colorGrading,
    defringe: v.defringe,
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
    texture: ops.texture ?? 0,
    sharpening: ops.sharpening ?? identitySharpening(),
    grain: ops.grain ?? identityGrain(),
    colorMixer: ops.color_mixer ?? identityColorMixer(),
    colorGrading: ops.color_grading ?? identityColorGrading(),
    defringe: ops.defringe ?? identityDefringe(),
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
    texture: lerp(base.texture ?? 0, target.texture ?? 0),
    sharpening: blendSharpening(base.sharpening, target.sharpening, t),
    grain: blendGrain(base.grain, target.grain, t),
    color_mixer: blendColorMixer(base.color_mixer, target.color_mixer, t),
    color_grading: blendColorGrading(base.color_grading, target.color_grading, t),
    defringe: blendDefringe(base.defringe, target.defringe, t),
    curves: blendCurves(base.curves, target.curves, t),
  };
}

function lerp01(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function blendSharpening(a: Sharpening | undefined, b: Sharpening | undefined, t: number): Sharpening {
  const aa = a ?? identitySharpening();
  const bb = b ?? identitySharpening();
  return {
    amount: lerp01(aa.amount, bb.amount, t),
    radius: lerp01(aa.radius, bb.radius, t),
    detail: lerp01(aa.detail, bb.detail, t),
    masking: lerp01(aa.masking, bb.masking, t),
  };
}

function blendGrain(a: Grain | undefined, b: Grain | undefined, t: number): Grain {
  const aa = a ?? identityGrain();
  const bb = b ?? identityGrain();
  return {
    amount: lerp01(aa.amount, bb.amount, t),
    size: lerp01(aa.size, bb.size, t),
    roughness: lerp01(aa.roughness, bb.roughness, t),
  };
}

function blendHsl(a: HslAdjust, b: HslAdjust, t: number): HslAdjust {
  return {
    hue: lerp01(a.hue, b.hue, t),
    saturation: lerp01(a.saturation, b.saturation, t),
    luminance: lerp01(a.luminance, b.luminance, t),
  };
}

function blendColorMixer(a: ColorMixer | undefined, b: ColorMixer | undefined, t: number): ColorMixer {
  const aa = a ?? identityColorMixer();
  const bb = b ?? identityColorMixer();
  return {
    red: blendHsl(aa.red, bb.red, t),
    orange: blendHsl(aa.orange, bb.orange, t),
    yellow: blendHsl(aa.yellow, bb.yellow, t),
    green: blendHsl(aa.green, bb.green, t),
    aqua: blendHsl(aa.aqua, bb.aqua, t),
    blue: blendHsl(aa.blue, bb.blue, t),
    purple: blendHsl(aa.purple, bb.purple, t),
    magenta: blendHsl(aa.magenta, bb.magenta, t),
  };
}

function blendWheel(a: HslWheel, b: HslWheel, t: number): HslWheel {
  return {
    hue: lerp01(a.hue, b.hue, t),
    saturation: lerp01(a.saturation, b.saturation, t),
    luminance: lerp01(a.luminance, b.luminance, t),
  };
}

function blendColorGrading(
  a: ColorGrading | undefined,
  b: ColorGrading | undefined,
  t: number,
): ColorGrading {
  const aa = a ?? identityColorGrading();
  const bb = b ?? identityColorGrading();
  return {
    shadows: blendWheel(aa.shadows, bb.shadows, t),
    midtones: blendWheel(aa.midtones, bb.midtones, t),
    highlights: blendWheel(aa.highlights, bb.highlights, t),
    global: blendWheel(aa.global, bb.global, t),
    blending: lerp01(aa.blending, bb.blending, t),
    balance: lerp01(aa.balance, bb.balance, t),
  };
}

function blendDefringe(a: Defringe | undefined, b: Defringe | undefined, t: number): Defringe {
  const aa = a ?? identityDefringe();
  const bb = b ?? identityDefringe();
  return {
    purple_amount: lerp01(aa.purple_amount, bb.purple_amount, t),
    purple_hue_range: lerp01(aa.purple_hue_range, bb.purple_hue_range, t),
    green_amount: lerp01(aa.green_amount, bb.green_amount, t),
    green_hue_range: lerp01(aa.green_hue_range, bb.green_hue_range, t),
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
