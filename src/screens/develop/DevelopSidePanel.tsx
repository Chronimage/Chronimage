import { useCallback, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { PRESETS, type PresetCategory } from './types';

const CATEGORIES: { id: PresetCategory; label: string }[] = [
  { id: 'face', label: 'Face' },
  { id: 'scene', label: 'Scene' },
  { id: 'quality', label: 'Quality' },
  { id: 'style', label: 'Style' },
  { id: 'custom', label: 'My presets' },
];

const CAT_TO_GROUP: Record<Exclude<PresetCategory, 'custom'>, 'Face' | 'Scene' | 'Quality' | 'Style'> = {
  face: 'Face',
  scene: 'Scene',
  quality: 'Quality',
  style: 'Style',
};

export function DevelopSidePanel() {
  const [category, setCategory] = useState<PresetCategory>('face');
  const [activePresets, setActivePresets] = useState<Map<string, number>>(() => new Map([['p-1', 55]]));

  const onPresetToggle = useCallback((id: string) => {
    setActivePresets((prev) => {
      const next = new Map(prev);
      if (next.has(id)) next.delete(id);
      else next.set(id, 50);
      return next;
    });
  }, []);

  const onPresetStrength = useCallback((id: string, strength: number) => {
    setActivePresets((prev) => {
      if (!prev.has(id)) return prev;
      const next = new Map(prev);
      next.set(id, strength);
      return next;
    });
  }, []);

  const filtered =
    category === 'custom'
      ? []
      : PRESETS.filter((p) => p.group === CAT_TO_GROUP[category as Exclude<PresetCategory, 'custom'>]);

  const activeEntries = [...activePresets.entries()];
  const primaryPreset = activeEntries[0];
  const primaryPresetMeta = primaryPreset ? PRESETS.find((p) => p.id === primaryPreset[0]) : null;

  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Presets</h3>
        <span className="count mono">gemma4</span>
      </div>

      {primaryPreset && primaryPresetMeta && (
        <div className="preset-active-sticky">
          <div className="lbl mono">Active · {primaryPresetMeta.name}</div>
          <div className="main">
            <span>Strength</span>
            <span className="val mono">{primaryPreset[1]}</span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            value={primaryPreset[1]}
            onChange={(e) => onPresetStrength(primaryPreset[0], Number(e.target.value))}
            aria-label={`${primaryPresetMeta.name} strength`}
          />
          <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
            {activeEntries.map(([id, strength], i) => {
              const meta = PRESETS.find((p) => p.id === id);
              if (!meta) return null;
              return (
                <Chip key={id} variant={i === 0 ? 'solid' : undefined} onClose={() => onPresetToggle(id)}>
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
        {category === 'custom' ? (
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
            <div>Save any combination as a preset.</div>
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 3 · preset library"
              style={{ marginTop: 12, justifyContent: 'center', width: '100%' }}
            >
              <Icon name="plus" size={13} /> Save current edits
            </button>
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
                onClick={() => onPresetToggle(p.id)}
                aria-pressed={active}
              >
                <div
                  className="pv"
                  style={{
                    background: `linear-gradient(135deg, oklch(0.6 0.18 ${(i * 47) % 360}), oklch(0.25 0.08 ${(i * 47) % 360}))`,
                  }}
                />
                <div className="preset-meta">
                  <div className="name">{p.name}</div>
                  <div className="sub">{p.sub}</div>
                </div>
                <div className="val mono">{active && strength !== undefined ? String(strength) : '—'}</div>
              </button>
            );
          })
        )}
      </div>

      <div
        style={{
          marginTop: 'auto',
          padding: 10,
          borderTop: '1px solid var(--stroke)',
          display: 'flex',
          gap: 6,
        }}
      >
        <button
          type="button"
          className="btn phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 3 · preset library"
          style={{ flex: 1, justifyContent: 'center', fontSize: 12, padding: '7px' }}
        >
          <Icon name="plus" size={12} /> Save as preset
        </button>
        <button
          type="button"
          className="btn phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 3 · import .xmp presets"
          style={{ justifyContent: 'center', fontSize: 12, padding: '7px' }}
        >
          <Icon name="download" size={12} />
        </button>
      </div>
    </div>
  );
}
