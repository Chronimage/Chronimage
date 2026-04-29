/**
 * DevelopSidePanel - preset library plus mask layer controls.
 *
 * Preset and mask creation live in the left rail. The right inspector owns
 * adjustment values; selecting a mask here switches that inspector to local
 * mask adjustments.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { CollapsibleSection } from '../../primitives/CollapsibleSection';
import { Icon } from '../../primitives/Icon';
import { Seg } from '../../primitives/Seg';
import { Slider } from '../../primitives/Slider';
import { useDevelopUi } from '../../state/develop';
import {
  useDevelopAdaptivePresetApply,
  useDevelopApply,
  useDevelopMaskApplyPreview,
  useDevelopMaskCreate,
  useDevelopMaskDelete,
  useDevelopMaskFaces,
  useDevelopMaskGenerate,
  useDevelopMasks,
  useDevelopMaskUpdate,
  usePresets,
} from '../../state/queries';
import { type DevelopOperations, identityOperations } from '../../tauri/invoke';
import { warn } from '../../util/log';
import { MASK_MODES, MASK_PRESETS, type MaskMode, type MaskPreset } from './masking';
import { blendOperations, normaliseOperations, type PresetCategory, parseOperationsJson } from './types';

const CATEGORIES: { id: PresetCategory; label: string }[] = [
  { id: 'face', label: 'Face' },
  { id: 'scene', label: 'Scene' },
  { id: 'quality', label: 'Quality' },
  { id: 'style', label: 'Style' },
  { id: 'custom', label: 'My presets' },
];

const CAT_TO_GROUP: Record<Exclude<PresetCategory, 'custom'>, string> = {
  face: 'Face',
  scene: 'Scene',
  quality: 'Quality',
  style: 'Style',
};

interface ActivePreset {
  strength: number;
  baseOperations: DevelopOperations;
}

/// Tooltip / aria-label text for a mask preset button. Three branches —
/// disabled-by-feature (e.g. depth model not bundled), disabled-because-
/// no-photo, and the regular "create a local foo mask" message. Pulled
/// out of the JSX so the inline render stays a single early-return-free
/// expression and SonarLint doesn't fire S3358 on a nested ternary.
function maskPresetTooltip(preset: MaskPreset, canMask: boolean, isDisabledKind: boolean): string {
  if (isDisabledKind) {
    // `disabledReason` only exists on the disabled variants of the
    // discriminated union — narrow with `in` so the ready entries don't
    // break the type.
    const reason = 'disabledReason' in preset ? preset.disabledReason : null;
    return reason ?? 'Not available yet';
  }
  if (!canMask) return 'Open a photo first';
  return `Create a local ${preset.label.toLowerCase()} mask`;
}

export function DevelopSidePanel() {
  const [category, setCategory] = useState<PresetCategory>('face');
  const [activePresets, setActivePresets] = useState<Map<number, ActivePreset>>(() => new Map());
  const [maskMode, setMaskMode] = useState<MaskMode>('normal');
  const [masking, setMasking] = useState(false);
  const [maskError, setMaskError] = useState<string | null>(null);
  const [activeMaskPresetId, setActiveMaskPresetId] = useState<string | null>(null);

  const focusedPhotoId = useDevelopUi((s) => s.focusedPhotoId);
  const setPreview = useDevelopUi((s) => s.setPreview);
  // Latest preview data URL — face-thumbnail circles read this through
  // a `--mask-preview` CSS variable on the mask section so the cropped
  // circles always reflect the in-flight global adjustments.
  const sharedPreview = useDevelopUi((s) => s.preview);
  const currentOperations = useDevelopUi((s) => s.operations);
  const setOperations = useDevelopUi((s) => s.setOperations);
  const selectedMaskId = useDevelopUi((s) => s.selectedMaskId);
  const setSelectedMaskId = useDevelopUi((s) => s.setSelectedMaskId);
  const drawMaskKind = useDevelopUi((s) => s.drawMaskKind);
  const setDrawMaskKind = useDevelopUi((s) => s.setDrawMaskKind);
  const eyedropperMode = useDevelopUi((s) => s.eyedropperMode);
  const setEyedropperMode = useDevelopUi((s) => s.setEyedropperMode);
  const maskOverlayVisible = useDevelopUi((s) => s.maskOverlayVisible);
  const setMaskOverlayVisible = useDevelopUi((s) => s.setMaskOverlayVisible);
  const maskOverlayOpacity = useDevelopUi((s) => s.maskOverlayOpacity);
  const setMaskOverlayOpacity = useDevelopUi((s) => s.setMaskOverlayOpacity);

  const { data: allPresets = [] } = usePresets();
  const { data: masks = [] } = useDevelopMasks(focusedPhotoId);
  const { data: faces = [] } = useDevelopMaskFaces(focusedPhotoId);
  const applyPreset = useDevelopApply();
  const applyAdaptivePreset = useDevelopAdaptivePresetApply();
  const createMask = useDevelopMaskCreate();
  const generateMask = useDevelopMaskGenerate();
  const updateMask = useDevelopMaskUpdate();
  const deleteMask = useDevelopMaskDelete();
  const applyMaskPreview = useDevelopMaskApplyPreview();

  const filtered = useMemo(() => {
    if (category === 'custom') {
      return allPresets.filter((p) => !p.is_system);
    }
    const g = CAT_TO_GROUP[category];
    return allPresets.filter((p) => p.group_name === g);
  }, [allPresets, category]);

  const previewOperations = useMemo(() => normaliseOperations(currentOperations), [currentOperations]);

  const pushApply = useCallback(
    (presetId: number, strength: number, baseOperations: DevelopOperations) => {
      if (focusedPhotoId == null) return;
      const preset = allPresets.find((p) => p.id === presetId);
      if (!preset) return;
      if (preset.scope === 'mask') {
        applyAdaptivePreset.mutate(
          { photoId: focusedPhotoId, presetId, strength },
          {
            onSuccess: (r) => {
              setPreview(r.preview_data_url);
            },
            onError: (e) => warn('develop adaptive preset apply failed', e),
          },
        );
        return;
      }
      const presetOperations = parseOperationsJson(preset.operations_json);
      if (!presetOperations) {
        warn('develop preset operations parse failed', preset.operations_json);
        return;
      }
      const operations = blendOperations(baseOperations, presetOperations, strength);
      setOperations(operations, 'sidepanel');
      applyPreset.mutate(
        { photoId: focusedPhotoId, operations },
        {
          onSuccess: (r) => {
            setPreview(r.preview_data_url);
          },
          onError: (e) => warn('develop preset apply failed', e),
        },
      );
    },
    [allPresets, applyAdaptivePreset, applyPreset, focusedPhotoId, setOperations, setPreview],
  );

  const onPresetClick = useCallback(
    (presetId: number) => {
      const existing = activePresets.get(presetId);
      const next = new Map(activePresets);
      if (existing) {
        next.delete(presetId);
        setActivePresets(next);
        pushApply(presetId, 0, existing.baseOperations);
        return;
      }
      const baseOperations = normaliseOperations(currentOperations);
      next.set(presetId, { strength: 75, baseOperations });
      setActivePresets(next);
      pushApply(presetId, 75, baseOperations);
    },
    [activePresets, currentOperations, pushApply],
  );

  const onPresetStrength = useCallback(
    (presetId: number, strength: number) => {
      const existing = activePresets.get(presetId);
      if (!existing) return;
      const next = new Map(activePresets);
      next.set(presetId, { ...existing, strength });
      setActivePresets(next);
      pushApply(presetId, strength, existing.baseOperations);
    },
    [activePresets, pushApply],
  );

  const activeEntries = [...activePresets.entries()];
  const primaryEntry = activeEntries[0];
  const primaryPreset = primaryEntry ? allPresets.find((p) => p.id === primaryEntry[0]) : null;
  const selectedMask =
    selectedMaskId == null ? null : (masks.find((mask) => mask.id === selectedMaskId) ?? null);
  const canMask = focusedPhotoId != null && !masking && !generateMask.isPending;
  const canCreateManualMask = focusedPhotoId != null && !createMask.isPending;

  useEffect(() => {
    if (focusedPhotoId == null) return;
    if (selectedMaskId != null && !masks.some((mask) => mask.id === selectedMaskId)) {
      setSelectedMaskId(null);
    }
  }, [focusedPhotoId, masks, selectedMaskId, setSelectedMaskId]);

  const refreshMaskPreview = useCallback(() => {
    if (focusedPhotoId == null) return;
    applyMaskPreview.mutate(
      { photoId: focusedPhotoId, operations: previewOperations },
      {
        onSuccess: (r) => setPreview(r.preview_data_url),
        onError: (e) => warn('develop mask preview failed', e),
      },
    );
  }, [applyMaskPreview, focusedPhotoId, previewOperations, setPreview]);

  const runMask = useCallback(
    (preset: MaskPreset, faceIndex?: number) => {
      if (!canMask || focusedPhotoId == null) return;
      // Disabled presets (e.g. Depth Range — model not bundled) are
      // surfaced as visible buttons for discoverability but must not
      // start a generation. The button itself is `disabled` in the
      // DOM; this guard is the belt-and-braces version.
      if (preset.availability === 'disabled') return;
      // Pixel-driven kinds need a colour / luma sample from the photo
      // before we can build a mask payload — flip into eyedropper
      // mode and let DevelopScreen handle the next canvas click. The
      // sample handler creates the mask and clears this flag.
      if (preset.id === 'color_range' || preset.id === 'luminance_range') {
        setEyedropperMode(preset.id);
        setDrawMaskKind(null);
        setActiveMaskPresetId(preset.id);
        return;
      }
      setActiveMaskPresetId(preset.id);
      setMasking(true);
      setMaskError(null);
      const exposure =
        preset.id === 'sky' ? 0.35 : preset.id === 'background' || preset.id === 'landscape' ? -0.2 : 0.2;
      const name = faceIndex != null ? `${preset.label} ${faceIndex + 1} mask` : `${preset.label} mask`;
      generateMask.mutate(
        {
          photo_id: focusedPhotoId,
          name,
          source: preset.id,
          mode: maskMode,
          operations: { ...identityOperations(), exposure },
          face_index: faceIndex ?? null,
        },
        {
          onSuccess: (receipt) => {
            setSelectedMaskId(receipt.mask.id);
            setPreview(receipt.preview_data_url);
          },
          onError: (e) => setMaskError(String(e)),
          onSettled: () => setMasking(false),
        },
      );
    },
    [
      canMask,
      focusedPhotoId,
      generateMask,
      maskMode,
      setDrawMaskKind,
      setEyedropperMode,
      setPreview,
      setSelectedMaskId,
    ],
  );

  // Per-face Person N — uses the existing "person" preset but pins
  // the face anchor to a specific row from `useDevelopMaskFaces`.
  const personPreset = useMemo(() => MASK_PRESETS.find((p) => p.id === 'person'), []);
  const runFaceMask = useCallback(
    (faceIndex: number) => {
      if (!personPreset) return;
      runMask(personPreset, faceIndex);
    },
    [personPreset, runMask],
  );

  const createManualMask = useCallback(
    (kind: 'brush' | 'linear_gradient' | 'radial_gradient') => {
      if (!canCreateManualMask || focusedPhotoId == null) return;
      const label =
        kind === 'brush' ? 'Brush mask' : kind === 'linear_gradient' ? 'Linear gradient' : 'Radial gradient';
      createMask.mutate(
        {
          photo_id: focusedPhotoId,
          name: label,
          source: kind,
          mode: maskMode,
          payload_storage: 'inline',
          mask_payload:
            kind === 'linear_gradient'
              ? { kind, top: 0.0, bottom: 0.55 }
              : kind === 'radial_gradient'
                ? { kind, cx: 0.5, cy: 0.5, radius: 0.35, feather: 0.35 }
                : { kind, cx: 0.5, cy: 0.5, radius: 0.18, feather: 0.45, flow: 1.0, density: 1.0 },
          operations: { ...identityOperations(), exposure: kind === 'brush' ? 0.2 : 0.35 },
        },
        {
          onSuccess: (maskId) => {
            setSelectedMaskId(maskId);
            refreshMaskPreview();
          },
          onError: (e) => setMaskError(String(e)),
        },
      );
    },
    [canCreateManualMask, createMask, focusedPhotoId, maskMode, refreshMaskPreview, setSelectedMaskId],
  );

  const updateLayer = useCallback(
    (maskId: number, patch: { mode?: MaskMode; visible?: boolean; exposure?: number }) => {
      const mask = masks.find((m) => m.id === maskId);
      const currentOps = mask
        ? (parseOperationsJson(mask.operations_json) ?? identityOperations())
        : identityOperations();
      const nextOps = patch.exposure == null ? undefined : { ...currentOps, exposure: patch.exposure };
      updateMask.mutate(
        {
          mask_id: maskId,
          ...(patch.mode ? { mode: patch.mode } : {}),
          ...(typeof patch.visible === 'boolean' ? { visible: patch.visible } : {}),
          ...(nextOps ? { operations: nextOps } : {}),
        },
        {
          onSuccess: () => refreshMaskPreview(),
          onError: (e) => setMaskError(String(e)),
        },
      );
    },
    [masks, refreshMaskPreview, updateMask],
  );

  return (
    <div className="sidepanel develop-sidepanel">
      <div className="head">
        <h3>Develop</h3>
        <span className="count mono">{focusedPhotoId == null ? '-' : `photo ${focusedPhotoId}`}</span>
      </div>

      <div className="sidepanel-scroll">
        <CollapsibleSection id="presets" title="Presets" defaultOpen>
          <div className="sidepanel-section-body">
            {primaryEntry && primaryPreset && (
              <div className="preset-active-sticky">
                <div className="lbl mono">Active / {primaryPreset.name}</div>
                <div className="main">
                  <span>Strength</span>
                  <span className="val mono">{primaryEntry[1].strength}</span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="100"
                  value={primaryEntry[1].strength}
                  onChange={(e) => onPresetStrength(primaryEntry[0], Number(e.target.value))}
                  aria-label={`${primaryPreset.name} strength`}
                />
                <div className="preset-chip-row">
                  {activeEntries.map(([id, activePreset], i) => {
                    const meta = allPresets.find((p) => p.id === id);
                    if (!meta) return null;
                    return (
                      <Chip
                        key={id}
                        variant={i === 0 ? 'solid' : undefined}
                        onClose={() => onPresetClick(id)}
                      >
                        {meta.name} / {activePreset.strength}
                      </Chip>
                    );
                  })}
                </div>
              </div>
            )}

            <Seg<PresetCategory>
              value={category}
              onChange={setCategory}
              options={CATEGORIES.map((c) => ({ value: c.id, label: c.label }))}
              className="preset-cats-seg"
            />

            <div className="preset-list">
              {category === 'custom' && filtered.length === 0 ? (
                <div className="sidepanel-empty">
                  <Icon name="sparkles" size={18} />
                  <div>
                    No custom presets yet. Save any combination of slider values from the inspector as a
                    preset.
                  </div>
                </div>
              ) : (
                filtered.map((p, i) => {
                  const active = activePresets.has(p.id);
                  const strength = activePresets.get(p.id)?.strength;
                  return (
                    <button
                      key={p.id}
                      type="button"
                      className={`preset-card ${active ? 'on' : ''}`}
                      onClick={() => onPresetClick(p.id)}
                      disabled={focusedPhotoId == null}
                      aria-pressed={active}
                      title={focusedPhotoId == null ? 'Open a photo first' : (p.description ?? p.name)}
                    >
                      <div
                        className="pv"
                        style={{
                          background: `linear-gradient(135deg, oklch(0.6 0.18 ${(i * 47) % 360}), oklch(0.25 0.08 ${(i * 47) % 360}))`,
                        }}
                      />
                      <div className="preset-meta">
                        <div className="name">{p.name}</div>
                        <div className="sub">
                          {p.scope === 'mask'
                            ? `Adaptive / ${p.mask_source ?? 'mask'}`
                            : (p.description ?? p.group_name)}
                        </div>
                      </div>
                      <div className="val mono">
                        {active && strength !== undefined ? String(strength) : '-'}
                      </div>
                    </button>
                  );
                })
              )}
            </div>
          </div>
        </CollapsibleSection>

        <CollapsibleSection
          id="masks"
          title="Masks"
          defaultOpen
          action={
            <span className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)' }}>
              {masks.length}
            </span>
          }
        >
          <div
            className="sidepanel-section-body mask-sidepanel"
            style={
              sharedPreview
                ? ({ '--mask-preview': `url("${sharedPreview}")` } as Record<string, string>)
                : undefined
            }
          >
            <button
              type="button"
              className={selectedMaskId == null ? 'mask-global-target active' : 'mask-global-target'}
              onClick={() => setSelectedMaskId(null)}
            >
              <span>Global photo adjustments</span>
              <span className="mono">right pane</span>
            </button>

            <div className="mask-create-head">
              <h4>Create New Mask</h4>
              <fieldset className="mask-mode-seg">
                <legend className="mask-mode-legend">Mask combine mode</legend>
                {MASK_MODES.map((mode) => (
                  <button
                    key={mode.id}
                    type="button"
                    className={maskMode === mode.id ? 'on' : ''}
                    onClick={() => setMaskMode(mode.id)}
                    aria-pressed={maskMode === mode.id}
                    aria-label={mode.full}
                    title={mode.full}
                  >
                    {mode.label}
                  </button>
                ))}
              </fieldset>
            </div>

            <div className="mask-preset-grid">
              {MASK_PRESETS.map((preset) => {
                const isDisabledKind = preset.availability === 'disabled';
                const buttonDisabled = !canMask || isDisabledKind;
                const tooltip = maskPresetTooltip(preset, canMask, isDisabledKind);
                return (
                  <button
                    key={preset.id}
                    type="button"
                    className={`${activeMaskPresetId === preset.id ? 'btn primary' : 'btn'}${
                      isDisabledKind ? ' is-coming-soon' : ''
                    }`}
                    disabled={buttonDisabled}
                    aria-disabled={buttonDisabled}
                    onClick={() => runMask(preset)}
                    title={tooltip}
                  >
                    <Icon name={preset.icon} size={12} /> {preset.label}
                  </button>
                );
              })}
            </div>

            {faces.length > 0 && (
              <fieldset className="mask-face-row" aria-label="Mask a specific person">
                {faces.map((face, idx) => {
                  const cx = face.x + face.w * 0.5;
                  const cy = face.y + face.h * 0.5;
                  const sizePct = Math.max(face.w, face.h) * 100;
                  const tooltip = `Mask Person ${idx + 1} (face score ${face.quality.toFixed(2)})`;
                  return (
                    <button
                      key={`${focusedPhotoId}-face-${face.index}`}
                      type="button"
                      className="mask-face-button"
                      disabled={!canMask}
                      onClick={() => runFaceMask(face.index)}
                      title={tooltip}
                      aria-label={tooltip}
                      style={
                        {
                          '--face-cx': `${cx * 100}%`,
                          '--face-cy': `${cy * 100}%`,
                          '--face-zoom': `${Math.max(120, 320 / Math.max(sizePct, 4))}%`,
                        } as Record<string, string>
                      }
                    >
                      <span className="mask-face-thumb" aria-hidden="true" />
                      <span className="mask-face-label">Person {idx + 1}</span>
                    </button>
                  );
                })}
              </fieldset>
            )}

            {eyedropperMode && (
              <div className="mask-draw-hint">
                {eyedropperMode === 'color_range'
                  ? 'Click on the photo to sample a colour for the Color Range mask.'
                  : 'Click on the photo to sample a luma value for the Luminance Range mask.'}
                <button
                  type="button"
                  className="btn"
                  onClick={() => setEyedropperMode(null)}
                  style={{ marginLeft: 'auto' }}
                >
                  Cancel
                </button>
              </div>
            )}

            <div className="mask-preset-grid">
              <button
                type="button"
                className="btn"
                onClick={() => createManualMask('brush')}
                disabled={!canCreateManualMask}
              >
                <Icon name="brush" size={12} /> Brush
              </button>
              <button
                type="button"
                className={`btn${drawMaskKind === 'linear_gradient' ? ' on' : ''}`}
                onClick={() => setDrawMaskKind(drawMaskKind === 'linear_gradient' ? null : 'linear_gradient')}
                disabled={!canCreateManualMask}
                aria-pressed={drawMaskKind === 'linear_gradient'}
                title="Drag a vertical line on the photo to define the gradient"
              >
                Linear gradient
              </button>
              <button
                type="button"
                className={`btn${drawMaskKind === 'radial_gradient' ? ' on' : ''}`}
                onClick={() => setDrawMaskKind(drawMaskKind === 'radial_gradient' ? null : 'radial_gradient')}
                disabled={!canCreateManualMask}
                aria-pressed={drawMaskKind === 'radial_gradient'}
                title="Drag from the center outwards to define a radial mask"
              >
                Radial gradient
              </button>
            </div>
            {drawMaskKind && drawMaskKind !== 'brush' && (
              <div className="mask-draw-hint">
                {drawMaskKind === 'linear_gradient'
                  ? 'Drag vertically on the photo to place the gradient.'
                  : 'Drag outwards on the photo to place the radial mask.'}
                <button
                  type="button"
                  className="btn"
                  onClick={() => setDrawMaskKind(null)}
                  style={{ marginLeft: 'auto' }}
                >
                  Cancel
                </button>
              </div>
            )}

            <label className="mask-overlay-toggle">
              <input
                type="checkbox"
                checked={maskOverlayVisible}
                onChange={(event) => setMaskOverlayVisible(event.currentTarget.checked)}
                disabled={!selectedMask}
              />
              Show selected overlay
            </label>
            <Slider
              label="Overlay opacity"
              value={maskOverlayOpacity}
              onChange={setMaskOverlayOpacity}
              min={10}
              max={100}
              suffix="%"
              disabled={!selectedMask}
            />

            {maskError && <div className="mask-error">{maskError}</div>}

            {masks.length === 0 ? (
              <div className="mask-empty-state">No masks yet</div>
            ) : (
              <div className="mask-layer-list">
                {masks.map((mask) => {
                  const maskOps = parseOperationsJson(mask.operations_json) ?? identityOperations();
                  const exposure = maskOps.exposure;
                  return (
                    <div
                      key={mask.id}
                      className={`mask-layer-row ${selectedMaskId === mask.id ? 'active' : ''}`}
                      data-hidden={!mask.visible}
                    >
                      <button
                        type="button"
                        className="mask-layer-select"
                        onClick={() => setSelectedMaskId(mask.id)}
                      >
                        <strong>{mask.name}</strong>
                        <span className="mono">
                          {mask.source.replaceAll('_', ' ')} / {mask.mode} / {exposure > 0 ? '+' : ''}
                          {exposure.toFixed(2)} EV
                        </span>
                      </button>
                      <fieldset className="mask-layer-mode">
                        <legend className="mask-mode-legend">Combine mode for {mask.name}</legend>
                        {MASK_MODES.map((mode) => (
                          <button
                            key={mode.id}
                            type="button"
                            className={mask.mode === mode.id ? 'on' : ''}
                            onClick={() => updateLayer(mask.id, { mode: mode.id })}
                            aria-label={mode.full}
                            title={mode.full}
                          >
                            {mode.label}
                          </button>
                        ))}
                      </fieldset>
                      <div className="mask-layer-actions">
                        <button type="button" className="btn" onClick={() => setSelectedMaskId(mask.id)}>
                          Select
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={() => updateLayer(mask.id, { visible: !mask.visible })}
                        >
                          {mask.visible ? 'Hide' : 'Show'}
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={() => updateLayer(mask.id, { exposure: 0.35 })}
                        >
                          +Light
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={() => updateLayer(mask.id, { exposure: -0.35 })}
                        >
                          -Dark
                        </button>
                        <button
                          type="button"
                          className="btn danger"
                          onClick={() =>
                            deleteMask.mutate(
                              { maskId: mask.id, photoId: mask.photo_id },
                              { onSuccess: () => refreshMaskPreview() },
                            )
                          }
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </CollapsibleSection>
      </div>
    </div>
  );
}
