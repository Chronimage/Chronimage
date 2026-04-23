/**
 * ImportProgressCard — live import progress for the catalog sidebar.
 *
 * Replaces the hard-coded `99.6% Cataloging` fixture that used to live at
 * the top of `CatalogSidePanel`. Reads the global [`useImportStore`],
 * collapses to nothing when no imports are active, and shows one compact
 * row per active import with an INDEX or CONSOLIDATE pill.
 */

import { type ActiveImport, useActiveImports } from '../../state/import';

function formatEta(seconds: number | null): string {
  if (seconds == null) return '';
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `~${Math.round(seconds / 60)}m`;
  return `~${Math.round(seconds / 3600)}h`;
}

function ModePill({ mode }: { mode: ActiveImport['mode'] }) {
  const label = mode === 'consolidate' ? 'CONSOLIDATE' : 'INDEX';
  const bg =
    mode === 'consolidate'
      ? 'color-mix(in oklch, var(--accent) 18%, transparent)'
      : 'color-mix(in oklch, var(--fg) 10%, transparent)';
  const fg = mode === 'consolidate' ? 'var(--accent)' : 'var(--fg-dim)';
  return (
    <span
      className="mono"
      style={{
        fontSize: 9.5,
        letterSpacing: '0.08em',
        padding: '2px 6px',
        borderRadius: 4,
        background: bg,
        color: fg,
        flexShrink: 0,
      }}
    >
      {label}
    </span>
  );
}

export function ImportProgressCard() {
  const imports = useActiveImports();
  const running = imports.filter((i) => !i.finished);
  if (running.length === 0) return null;

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
          <span>IMPORTING</span>
          <span>{running.length}</span>
        </div>
        {running.map((imp) => {
          const pct = imp.total > 0 ? Math.round((imp.done / imp.total) * 100) : 0;
          return (
            <div key={imp.importId} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  fontSize: 11.5,
                }}
              >
                <ModePill mode={imp.mode} />
                <span
                  style={{
                    flex: 1,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                  title={imp.sourceName}
                >
                  {imp.sourceName}
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
                <span>
                  {imp.done}/{imp.total || '?'}
                </span>
                <span>{formatEta(imp.etaSeconds)}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
