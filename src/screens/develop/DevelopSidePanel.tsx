/**
 * DevelopSidePanel — Phase 3 preset library.
 *
 * Presets come from the `presets` table (seeded on boot by
 * `develop::presets::seed_builtins`). The user clicks a card → we call
 * `develop_preset_apply(photo_id, preset_id, strength)` which returns a
 * RenderReceipt; we push the preview URL into `useDevelopUi` so the
 * sibling DevelopScreen swaps its stage image.
 *
 * Strength is per-preset (Map<presetId, strength>) with a live slider for
 * the most-recently-applied one. The old hardcoded PRESETS list was
 * replaced with live query data.
 */

import { useCallback, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { useDevelopUi } from '../../state/develop';
import { useDevelopPresetApply, usePresets } from '../../state/queries';
import { warn } from '../../util/log';
import type { PresetCategory } from './types';

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

export function DevelopSidePanel() {
  const [category, setCategory] = useState<PresetCategory>('face');
  const [activePresets, setActivePresets] = useState<Map<number, number>>(() => new Map());

  const focusedPhotoId = useDevelopUi((s) => s.focusedPhotoId);
  const setPreview = useDevelopUi((s) => s.setPreview);

  const { data: allPresets = [] } = usePresets();
  const applyPreset = useDevelopPresetApply();

  const filtered = useMemo(() => {
    if (category === 'custom') {
      return allPresets.filter((p) => !p.is_system);
    }
    const g = CAT_TO_GROUP[category];
    return allPresets.filter((p) => p.group_name === g);
  }, [allPresets, category]);

  const pushApply = useCallback(
    (presetId: number, strength: number) => {
      if (focusedPhotoId == null) return;
      applyPreset.mutate(
        { photoId: focusedPhotoId, presetId, strength },
        {
          onSuccess: (r) => setPreview(r.preview_data_url),
          onError: (e) => warn('develop_preset_apply failed', e),
        },
      );
    },
    [applyPreset, focusedPhotoId, setPreview],
  );

  const onPresetClick = useCallback(
    (presetId: number) => {
      setActivePresets((prev) => {
        const next = new Map(prev);
        if (next.has(presetId)) {
          next.delete(presetId);
          // Revert to identity/current-saved when un-applying.
          pushApply(presetId, 0);
        } else {
          next.set(presetId, 75);
          pushApply(presetId, 75);
        }
        return next;
      });
    },
    [pushApply],
  );

  const onPresetStrength = useCallback(
    (presetId: number, strength: number) => {
      setActivePresets((prev) => {
        if (!prev.has(presetId)) return prev;
        const next = new Map(prev);
        next.set(presetId, strength);
        return next;
      });
      pushApply(presetId, strength);
    },
    [pushApply],
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
            <span className="val mono">{primaryEntry[1]}</span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            value={primaryEntry[1]}
            onChange={(e) => onPresetStrength(primaryEntry[0], Number(e.target.value))}
            aria-label={`${primaryPreset.name} strength`}
          />
          <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
            {activeEntries.map(([id, strength], i) => {
              const meta = allPresets.find((p) => p.id === id);
              if (!meta) return null;
              return (
                <Chip key={id} variant={i === 0 ? 'solid' : undefined} onClose={() => onPresetClick(id)}>
                  {meta.name} · {strength}
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
            const strength = activePresets.get(p.id);
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
                  <div className="sub">{p.description ?? p.group_name}</div>
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
