/**
 * ImportProgressCard — live import progress for the catalog sidebar.
 *
 * Reads `useImportStore` and renders one compact row per active import
 * with an INDEX or CONSOLIDATE eyebrow, filename, percentage, ETA, and
 * a thin progress bar driven by `var(--accent)`.
 */

import { cn } from '@/lib/utils';
import { type ActiveImport, useActiveImports } from '../../state/import';

function formatEta(seconds: number | null): string {
  if (seconds == null) return '';
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `~${Math.round(seconds / 60)}m`;
  return `~${Math.round(seconds / 3600)}h`;
}

function ModePill({ mode }: { readonly mode: ActiveImport['mode'] }) {
  const label = mode === 'consolidate' ? 'CONSOLIDATE' : 'INDEX';
  return (
    <span className={cn('import-progress-mode', mode === 'consolidate' && 'is-consolidate')}>{label}</span>
  );
}

export function ImportProgressCard() {
  const imports = useActiveImports();
  const running = imports.filter((i) => !i.finished);
  if (running.length === 0) return null;

  return (
    <div className="import-progress-wrap">
      <div className="import-progress-card">
        <div className="import-progress-head eyebrow">
          <span>Importing</span>
          <span className="num">{running.length}</span>
        </div>
        {running.map((imp) => {
          const pct = imp.total > 0 ? Math.round((imp.done / imp.total) * 100) : 0;
          return (
            <div key={imp.importId} className="import-progress-row">
              <div className="import-progress-row-head">
                <ModePill mode={imp.mode} />
                <span className="import-progress-name" title={imp.sourceName}>
                  {imp.sourceName}
                </span>
                <span className="import-progress-pct num">{pct}%</span>
              </div>
              <div className="import-progress-bar">
                <div className="import-progress-bar-fill" style={{ width: `${pct}%` }} />
              </div>
              <div className="import-progress-meta caption num">
                <span>
                  {imp.done.toLocaleString()}/{imp.total ? imp.total.toLocaleString() : '?'}
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
