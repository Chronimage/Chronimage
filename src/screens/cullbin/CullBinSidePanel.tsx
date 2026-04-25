import { useState } from 'react';
import { Icon } from '../../primitives/Icon';
import { useCullBinSummary } from '../../state/queries';

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

const REASON_LABELS: Record<string, string> = {
  near_dup: 'Near-duplicates',
  blur: 'Out of focus',
  eyes_closed: 'Eyes closed',
  exposure: 'Over/under exp.',
  user: 'User rejected',
  flag: 'Flagged',
  duplicate: 'Duplicates',
  other: 'Other',
};

export function CullBinSidePanel() {
  const { data: summary } = useCullBinSummary();
  const [activeFilter, setActiveFilter] = useState<string>('All rejects');

  const totalCount = summary?.total_count ?? 0;
  const totalBytes = summary?.total_bytes ?? 0;
  const byReason: [string, number][] = summary?.by_reason ?? [];

  const filters = [
    { label: 'All rejects', count: totalCount },
    ...byReason.map(([reason, count]) => ({
      label: REASON_LABELS[reason] ?? reason,
      count,
    })),
  ];

  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Cull Bin</h3>
        <span className="count">{totalCount} items</span>
      </div>

      <div style={{ padding: '4px 16px 14px' }}>
        <div
          className="mono"
          style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 8, letterSpacing: '0.08em' }}
        >
          RECLAIMABLE
        </div>
        <div className="display" style={{ fontSize: 40, lineHeight: 1 }}>
          {totalBytes > 0 ? formatBytes(totalBytes) : '—'}
        </div>
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 4 }}>
          Originals preserved · metadata intact
        </div>
      </div>

      <div className="section-label">
        <span>Filter</span>
      </div>
      <div className="list">
        {filters.map((f) => (
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
