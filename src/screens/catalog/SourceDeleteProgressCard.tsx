/**
 * SourceDeleteProgressCard — live disconnect progress for the catalog sidebar.
 *
 * Mirrors `ImportProgressCard`. Reads from the global
 * [`useSourceDeleteStore`], renders one row per active disconnect with the
 * current phase + percentage, and collapses to nothing when no disconnects
 * are running.
 */

import { useActiveSourceDeletes } from '../../state/sourceDelete';
import type { SourceDeletePhase } from '../../tauri/invoke';

const PHASE_LABEL: Record<SourceDeletePhase, string> = {
  collecting: 'Collecting',
  deleting: 'Removing',
  committed: 'Cleaning up',
  thumb_cleanup: 'Cleaning thumbnails',
  recycling: 'Recycling files',
  done: 'Done',
};

export function SourceDeleteProgressCard() {
  const deletes = useActiveSourceDeletes();
  if (deletes.length === 0) return null;

  return (
    <div style={{ padding: '0 10px 10px' }}>
      <div
        style={{
          border: '1px solid var(--stroke)',
          borderRadius: 'var(--radius-md)',
          padding: 8,
          background: 'var(--bg-elev)',
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        <div
          className="mono"
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            letterSpacing: '0.06em',
          }}
        >
          <span>DISCONNECTING</span>
          <span>{deletes.length}</span>
        </div>
        {deletes.map((d) => {
          const pct = d.total > 0 ? Math.round((d.done / d.total) * 100) : d.finished ? 100 : 0;
          return (
            <div key={d.sourceId} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  fontSize: 11.5,
                }}
              >
                <span
                  style={{
                    flex: 1,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                  title={d.sourceName}
                >
                  {d.sourceName}
                </span>
                <span className="mono" style={{ color: 'var(--accent)', fontSize: 11, flexShrink: 0 }}>
                  {pct}%
                </span>
              </div>
              <div
                className="progress"
                style={{
                  height: 3,
                  background: 'var(--stroke)',
                  borderRadius: 2,
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    width: `${pct}%`,
                    height: '100%',
                    background: 'var(--accent)',
                    transition: 'width 200ms linear',
                  }}
                />
              </div>
              <div
                className="mono"
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  fontSize: 10,
                  color: 'var(--fg-mute)',
                }}
              >
                <span>{PHASE_LABEL[d.phase]}</span>
                <span>{d.total > 0 ? `${d.done}/${d.total}` : ''}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
