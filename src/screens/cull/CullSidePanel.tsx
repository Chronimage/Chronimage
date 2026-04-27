import { Icon } from '../../primitives/Icon';
import { useCull } from '../../state/cull';
import { useCullBinSummary, usePhotos } from '../../state/queries';

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function CullSidePanel() {
  const mode = useCull((s) => s.mode);
  const onModeChange = useCull((s) => s.setMode);
  const view = useCull((s) => s.view);
  const onViewChange = useCull((s) => s.setView);
  const idx = useCull((s) => s.idx);
  const kept = useCull((s) => s.kept);
  const rejected = useCull((s) => s.rejected);
  const { data: binSummary } = useCullBinSummary();
  const { data: photos = [] } = usePhotos();
  const total = Math.floor(photos.length / 2);
  const pct = total > 0 ? (idx / total) * 100 : 0;
  const recoverableCount = binSummary?.total_count ?? 0;
  const recoverableBytes = binSummary?.total_bytes ?? 0;
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
          <span>{Math.max(0, total - idx)} left</span>
        </div>
      </div>

      <div className="section-label">
        <span>Workflow</span>
      </div>
      <div className="list">
        {(
          [
            { value: 'review', label: 'Review queue', count: Math.max(0, total - idx), icon: 'compare' },
            { value: 'rejected', label: 'Rejected items', count: recoverableCount, icon: 'flag' },
            {
              value: 'cleanup',
              label: 'Cleanup',
              count: recoverableBytes > 0 ? formatBytes(recoverableBytes) : '—',
              icon: 'cull',
            },
          ] as const
        ).map((item) => (
          <button
            key={item.value}
            type="button"
            className={`item ${view === item.value ? 'active' : ''}`}
            onClick={() => onViewChange(item.value)}
            aria-pressed={view === item.value}
          >
            <span className="ico">
              <Icon name={item.icon} size={14} />
            </span>
            <span>{item.label}</span>
            <span className="n">{item.count}</span>
          </button>
        ))}
      </div>

      <div className="section-label">
        <span>Review mode</span>
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
          className="btn primary"
          onClick={() => onViewChange('rejected')}
          style={{ width: '100%', marginTop: 12, justifyContent: 'center' }}
        >
          Review rejects before deleting
        </button>
      </div>
    </div>
  );
}
