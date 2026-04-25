import { Icon } from '../../primitives/Icon';
import { useCull } from '../../state/cull';

export interface CullSidePanelProps {
  readonly total: number;
}

const ISSUE_FILTERS: { label: string }[] = [
  { label: 'Near-duplicates' },
  { label: 'Out of focus' },
  { label: 'Eyes closed' },
  { label: 'Over/under exp.' },
  { label: 'Screenshots' },
  { label: 'Low-res / web' },
];

export function CullSidePanel({ total }: CullSidePanelProps) {
  const mode = useCull((s) => s.mode);
  const onModeChange = useCull((s) => s.setMode);
  const idx = useCull((s) => s.idx);
  const kept = useCull((s) => s.kept);
  const rejected = useCull((s) => s.rejected);
  const activeFilters = useCull((s) => s.activeFilters);
  const onFilterToggle = useCull((s) => s.toggleFilter);
  const pct = total > 0 ? (idx / total) * 100 : 0;
  const minutesLeft = Math.max(1, Math.round((total - idx) * 0.4));
  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Cull Queue</h3>
        <span className="count">{Math.max(0, total - idx)} left</span>
      </div>

      <div style={{ padding: '4px 16px 14px' }}>
        <div
          className="mono"
          style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 8, letterSpacing: '0.08em' }}
        >
          SESSION PROGRESS
        </div>
        <div className="cull-progress">
          <div style={{ width: `${pct}%` }} />
        </div>
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-dim)',
            marginTop: 6,
            display: 'flex',
            justifyContent: 'space-between',
          }}
        >
          <span>
            {idx}/{total} reviewed
          </span>
          <span>~{minutesLeft} min left</span>
        </div>
      </div>

      <div className="section-label">
        <span>Mode</span>
      </div>
      <div style={{ padding: '0 12px 12px' }}>
        <div className="cull-mode-seg" role="tablist" aria-label="Cull mode">
          {(
            [
              { value: 'compare', label: 'Compare' },
              { value: 'grid', label: 'Grid' },
              { value: 'swipe', label: 'Swipe' },
            ] as const
          ).map((o) => (
            <button
              key={o.value}
              type="button"
              role="tab"
              aria-selected={mode === o.value}
              className={mode === o.value ? 'on' : ''}
              onClick={() => onModeChange(o.value)}
            >
              {o.label}
            </button>
          ))}
        </div>
      </div>

      <div className="section-label">
        <span>Filter issues</span>
      </div>
      <div className="list">
        {ISSUE_FILTERS.map((f) => (
          <button
            key={f.label}
            type="button"
            className={`item ${activeFilters.has(f.label) ? 'active' : ''}`}
            onClick={() => onFilterToggle(f.label)}
            aria-pressed={activeFilters.has(f.label)}
          >
            <span className="ico">
              <Icon name="flag" size={14} />
            </span>
            <span>{f.label}</span>
            <span className="n">—</span>
          </button>
        ))}
      </div>

      <div
        style={{
          marginTop: 'auto',
          padding: 14,
          borderTop: '1px solid var(--stroke)',
        }}
      >
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            marginBottom: 8,
            letterSpacing: '0.08em',
          }}
        >
          SESSION SUMMARY
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
          <div>
            <div className="display" style={{ fontSize: 28 }}>
              {kept}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
              KEPT
            </div>
          </div>
          <div>
            <div className="display" style={{ fontSize: 28, color: 'var(--warn)' }}>
              {rejected}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
              REJECTED
            </div>
          </div>
        </div>
        <button
          type="button"
          className="btn primary phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 2 · Cull Bin review"
          style={{ width: '100%', marginTop: 12, justifyContent: 'center' }}
        >
          Review rejects before deleting
        </button>
      </div>
    </div>
  );
}
