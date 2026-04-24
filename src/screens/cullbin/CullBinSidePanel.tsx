import { useState } from 'react';
import { Icon } from '../../primitives/Icon';

const FILTERS = [
  { label: 'All rejects', count: 312 },
  { label: 'Near-duplicates', count: 184 },
  { label: 'Out of focus', count: 71 },
  { label: 'Eyes closed', count: 34 },
  { label: 'Screenshots', count: 23 },
];

export function CullBinSidePanel() {
  const [activeFilter, setActiveFilter] = useState<string>('All rejects');
  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Cull Bin</h3>
        <span className="count">312 items</span>
      </div>

      <div style={{ padding: '4px 16px 14px' }}>
        <div
          className="mono"
          style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 8, letterSpacing: '0.08em' }}
        >
          RECLAIMABLE
        </div>
        <div className="display" style={{ fontSize: 40, lineHeight: 1 }}>
          8.2<span style={{ fontSize: 16 }}>GB</span>
        </div>
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 4 }}>
          Originals preserved · metadata intact
        </div>
      </div>

      <div className="section-label">
        <span>Filter</span>
      </div>
      <div className="list">
        {FILTERS.map((f) => (
          <button
            key={f.label}
            type="button"
            className={`item ${activeFilter === f.label ? 'active' : ''}`}
            onClick={() => setActiveFilter(f.label)}
            aria-pressed={activeFilter === f.label}
          >
            <span className="ico">
              <Icon name="flag" size={13} />
            </span>
            <span>{f.label}</span>
            <span className="n">{f.count}</span>
          </button>
        ))}
      </div>

      <div className="section-label">
        <span>Retention</span>
      </div>
      <div style={{ padding: '0 16px 14px', fontSize: 12, color: 'var(--fg-dim)', lineHeight: 1.5 }}>
        Auto-empty after{' '}
        <span className="mono" style={{ color: 'var(--accent)' }}>
          30 days
        </span>
        . Nothing leaves your disk without confirmation.
      </div>

      <div
        style={{
          marginTop: 'auto',
          padding: 12,
          borderTop: '1px solid var(--stroke)',
          display: 'flex',
          flexDirection: 'column',
          gap: 6,
        }}
      >
        <button
          type="button"
          className="btn phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 2 · Cull Bin restore"
          style={{ width: '100%', justifyContent: 'center' }}
        >
          Restore all to catalog
        </button>
        <button
          type="button"
          className="btn danger phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 2 · Cull Bin empty"
          style={{ width: '100%', justifyContent: 'center' }}
        >
          <Icon name="reject" size={13} /> Empty bin permanently
        </button>
      </div>
    </div>
  );
}
