/**
 * DevelopSidePanel — Phase 3 preset library.
 *
 * Presets come from the `presets` table (seeded on boot by
 * `develop::presets::seed_builtins`). The user clicks a card → we call
 * `develop_apply(photo_id, operations)` with locally blended preset
 * operations; we push the preview URL + operations into `useDevelopUi`
 * so the sibling DevelopScreen swaps its stage image and saves the same
 * values the preview shows.
 *
 * Strength is per-preset with a live slider for the most-recently-applied
 * one. The old hardcoded PRESETS list was replaced with live query data.
 */

import { useCallback, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { useDevelopUi } from '../../state/develop';
import { useDevelopAdaptivePresetApply, useDevelopApply, usePresets } from '../../state/queries';
import type { DevelopOperations } from '../../tauri/invoke';
import { warn } from '../../util/log';
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

export function DevelopSidePanel() {
  const [category, setCategory] = useState<PresetCategory>('face');
  const [activePresets, setActivePresets] = useState<Map<number, ActivePreset>>(() => new Map());

  const focusedPhotoId = useDevelopUi((s) => s.focusedPhotoId);
  const setPreview = useDevelopUi((s) => s.setPreview);
  const currentOperations = useDevelopUi((s) => s.operations);
  const setOperations = useDevelopUi((s) => s.setOperations);

  const { data: allPresets = [] } = usePresets();
  const applyPreset = useDevelopApply();
  const applyAdaptivePreset = useDevelopAdaptivePresetApply();

  const filtered = useMemo(() => {
    if (category === 'custom') {
      return allPresets.filter((p) => !p.is_system);
    }
    const g = CAT_TO_GROUP[category];
    return allPresets.filter((p) => p.group_name === g);
  }, [allPresets, category]);

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

  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Presets</h3>
        <span className="count mono">{focusedPhotoId == null ? '—' : `photo ${focusedPhotoId}`}</span>
      </div>

      {primaryEntry && primaryPreset && (
        <div className="preset-active-sticky">
          <div className="lbl mono">Active · {primaryPreset.name}</div>
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
          <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
            {activeEntries.map(([id, activePreset], i) => {
              const meta = allPresets.find((p) => p.id === id);
              if (!meta) return null;
              return (
                <Chip key={id} variant={i === 0 ? 'solid' : undefined} onClose={() => onPresetClick(id)}>
                  {meta.name} · {activePreset.strength}
                </Chip>
              );
            })}
          </div>
        </div>
      )}

      <div className="preset-cats">
        {CATEGORIES.map((c) => (
          <button
            key={c.id}
            type="button"
            className={category === c.id ? 'on' : ''}
            onClick={() => setCategory(c.id)}
            aria-pressed={category === c.id}
          >
            {c.label}
          </button>
        ))}
      </div>

      <div
        style={{
          padding: '10px 12px',
          display: 'flex',
          flexDirection: 'column',
          gap: 6,
          overflow: 'auto',
          flex: 1,
        }}
      >
        {category === 'custom' && filtered.length === 0 ? (
          <div
            style={{
              padding: 24,
              textAlign: 'center',
              color: 'var(--fg-mute)',
              fontSize: 12.5,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              gap: 8,
            }}
          >
            <Icon name="sparkles" size={18} />
            <div>
              No custom presets yet. Save any combination of slider values from the inspector as a preset.
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
                      ? `Adaptive · ${p.mask_source ?? 'mask'}`
                      : (p.description ?? p.group_name)}
                  </div>
                </div>
                <div className="val mono">{active && strength !== undefined ? String(strength) : '—'}</div>
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
