import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useEffect, useRef, useState } from 'react';
import { Icon } from '../primitives/Icon';
import {
  type CleanupPlan,
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  useCleanupDryRun,
  useCreateSource,
  useSources,
  useStartImport,
} from '../state/queries';
import { useUi } from '../state/ui';

interface ActiveImport {
  importId: number;
  sourceId: number;
  sourceName: string;
  total: number;
  done: number;
  currentFile: string;
  etaSeconds: number | null;
  finished: boolean;
}

function fmtBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function CleanupSection({ plans }: { plans: CleanupPlan[] }) {
  const totalBytes = plans.reduce((sum, p) => sum + p.reclaimable_bytes, 0);
  return (
    <div style={{ marginTop: 32 }}>
      <div
        className="mono"
        style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 10, letterSpacing: '0.08em' }}
      >
        STORAGE RECLAIMABLE · SHA256-VERIFIED
      </div>
      <div
        style={{
          padding: '12px 14px',
          border: '1px solid var(--accent)',
          borderRadius: 10,
          background: 'color-mix(in oklch, var(--accent) 6%, var(--bg-elev))',
          marginBottom: 10,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <span className="display" style={{ fontSize: 28 }}>
            {fmtBytes(totalBytes)}
          </span>
          <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
            {plans.reduce((s, p) => s + p.item_count, 0).toLocaleString()} photos across {plans.length}{' '}
            {plans.length === 1 ? 'source' : 'sources'}
          </span>
        </div>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {plans.map((plan) => (
          <div
            key={plan.source_id}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              padding: '8px 12px',
              border: '1px solid var(--stroke)',
              borderRadius: 8,
              background: 'var(--bg-elev)',
            }}
          >
            <Icon name={SOURCE_KIND_ICON[getSourceKind(plan.source_name)] ?? 'disk'} size={14} />
            <span style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>{plan.source_name}</span>
            <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
              {plan.item_count.toLocaleString()} photos
            </span>
            <span className="mono" style={{ fontSize: 12, color: 'var(--accent)', fontWeight: 600 }}>
              {fmtBytes(plan.reclaimable_bytes)}
            </span>
            <button
              type="button"
              className="btn2"
              style={{ padding: '4px 10px', fontSize: 11 }}
              disabled
              title="Source cleanup executes in Phase 1b"
            >
              Clean up
            </button>
          </div>
        ))}
      </div>
      <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 8 }}>
        Local copies verified by SHA256. Source files will not be deleted without a second confirmation.
      </div>
    </div>
  );
}

function getSourceKind(sourceName: string): string {
  const lower = sourceName.toLowerCase();
  if (lower.includes('google')) return 'google_photos';
  if (lower.includes('icloud')) return 'icloud';
  if (lower.includes('iphone')) return 'iphone';
  if (lower.includes('nas')) return 'nas';
  return 'local';
}

export function OnboardScreen() {
  const { data: sources = [] } = useSources();
  const createSource = useCreateSource();
  const startImport = useStartImport();
  const { data: cleanupPlans = [] } = useCleanupDryRun();
  const setScreen = useUi((s) => s.setScreen);

  const [activeImports, setActiveImports] = useState<Map<number, ActiveImport>>(new Map());
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    let cancelled = false;
    listen<ImportProgressEvent>(IMPORT_PROGRESS_EVENT, (event) => {
      const p = event.payload;
      setActiveImports((prev) => {
        const next = new Map(prev);
        next.set(p.import_id, {
          importId: p.import_id,
          sourceId: p.source_id,
          sourceName: prev.get(p.import_id)?.sourceName ?? '',
          total: p.total,
          done: p.done,
          currentFile: p.current_file,
          etaSeconds: p.eta_seconds,
          finished: p.done >= p.total && p.total > 0,
        });
        return next;
      });
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenRef.current = unlisten;
    });
    return () => {
      cancelled = true;
      unlistenRef.current?.();
    };
  }, []);

  async function handleAddLocalFolder() {
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== 'string') return;

    const folderName = selected.split(/[\\/]/).pop() ?? selected;
    const source = await createSource.mutateAsync({
      name: `Local · ${folderName}`,
      kind: 'local',
      rootPath: selected,
    });

    const resp = await startImport.mutateAsync({ sourceId: source.id, root: selected });

    setActiveImports((prev) => {
      const next = new Map(prev);
      next.set(resp.import_id, {
        importId: resp.import_id,
        sourceId: source.id,
        sourceName: source.name,
        total: 0,
        done: 0,
        currentFile: '',
        etaSeconds: null,
        finished: false,
      });
      return next;
    });
  }

  const hasAnySources = sources.length > 0;
  const runningImports = [...activeImports.values()].filter((i) => !i.finished);
  const finishedImports = [...activeImports.values()].filter((i) => i.finished);
  const busy = createSource.isPending || startImport.isPending;
  const error = createSource.error ?? startImport.error;

  return (
    <div className="canvas" style={{ gridColumn: '2 / -1' }}>
      <div style={{ padding: 48, maxWidth: 820, margin: '0 auto' }}>
        <div
          className="mono"
          style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em', marginBottom: 14 }}
        >
          WELCOME
        </div>
        <h1 className="display" style={{ fontSize: 64, margin: 0, lineHeight: 0.96 }}>
          Your photos,
          <br />
          <em>in one light.</em>
        </h1>
        <p style={{ color: 'var(--fg-dim)', fontSize: 14, lineHeight: 1.55, maxWidth: 560, marginTop: 18 }}>
          Chronimage unifies your fragmented library into one owned catalog. On-device AI tags, clusters
          faces, detects duplicates, and safely frees source storage once copies are verified.
        </p>

        {/* ── Active imports ── */}
        {runningImports.length > 0 && (
          <div style={{ marginTop: 24, display: 'flex', flexDirection: 'column', gap: 10 }}>
            {runningImports.map((imp) => {
              const pct = imp.total > 0 ? Math.round((imp.done / imp.total) * 100) : 0;
              return (
                <div
                  key={imp.importId}
                  style={{
                    padding: 14,
                    border: '1px solid var(--accent)',
                    borderRadius: 10,
                    background: 'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))',
                  }}
                >
                  <div
                    className="mono"
                    style={{
                      fontSize: 10.5,
                      color: 'var(--accent)',
                      marginBottom: 8,
                      letterSpacing: '0.08em',
                    }}
                  >
                    IMPORTING · {imp.sourceName || 'local folder'}
                  </div>
                  <div className="progress" style={{ height: 3, marginBottom: 6 }}>
                    <div style={{ width: `${pct}%`, transition: 'width 0.3s ease' }} />
                  </div>
                  <div
                    className="mono"
                    style={{
                      fontSize: 11,
                      color: 'var(--fg-dim)',
                      display: 'flex',
                      justifyContent: 'space-between',
                    }}
                  >
                    <span
                      style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', flex: 1 }}
                    >
                      {imp.currentFile || 'Scanning…'}
                    </span>
                    <span style={{ marginLeft: 12, flexShrink: 0 }}>
                      {imp.done.toLocaleString()} / {imp.total.toLocaleString()}
                      {imp.etaSeconds != null && imp.etaSeconds > 0 ? ` · ~${imp.etaSeconds}s left` : ''}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {/* ── Finished import toasts ── */}
        {finishedImports.map((imp) => (
          <div
            key={imp.importId}
            style={{
              marginTop: 10,
              padding: '10px 14px',
              border: '1px solid var(--stroke)',
              borderRadius: 8,
              background: 'var(--bg-elev)',
              display: 'flex',
              alignItems: 'center',
              gap: 10,
            }}
          >
            <Icon name="keep" size={13} />
            <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
              {imp.sourceName} — {imp.done.toLocaleString()} photos imported
            </span>
          </div>
        ))}

        {/* ── Add source actions ── */}
        <div style={{ marginTop: 28, display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          <button
            type="button"
            className="btn2 primary"
            style={{ padding: '9px 16px', fontSize: 13 }}
            onClick={handleAddLocalFolder}
            disabled={busy}
          >
            <Icon name="disk" size={14} />
            {busy ? 'Adding…' : 'Add local folder'}
          </button>
          <button
            type="button"
            className="btn2"
            style={{ padding: '9px 16px', fontSize: 13 }}
            disabled
            title="Phase 1b"
          >
            <Icon name="cloud" size={14} /> Google Photos
          </button>
          <button
            type="button"
            className="btn2"
            style={{ padding: '9px 16px', fontSize: 13 }}
            disabled
            title="Phase 1b"
          >
            <Icon name="iphone" size={14} /> iPhone USB
          </button>
          <button
            type="button"
            className="btn2"
            style={{ padding: '9px 16px', fontSize: 13 }}
            disabled
            title="Phase 1b"
          >
            <Icon name="nas" size={14} /> NAS
          </button>
        </div>

        {error && (
          <div className="mono" style={{ fontSize: 11, color: 'var(--destructive)', marginTop: 10 }}>
            {String(error)}
          </div>
        )}

        {/* ── Existing sources ── */}
        {hasAnySources && (
          <>
            <div
              className="mono"
              style={{
                fontSize: 10.5,
                color: 'var(--fg-mute)',
                marginTop: 32,
                marginBottom: 10,
                letterSpacing: '0.08em',
              }}
            >
              CONNECTED SOURCES
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {sources.map((s) => (
                <div key={s.id} className="source-row" style={{ padding: '10px 12px', marginBottom: 0 }}>
                  <div className="ico">
                    <Icon name={SOURCE_KIND_ICON[s.kind] ?? 'disk'} size={18} />
                  </div>
                  <div style={{ flex: 1 }}>
                    <div className="name">{s.name}</div>
                    <div className="sub">
                      {s.photo_count > 0 ? `${s.photo_count.toLocaleString()} photos` : 'No photos yet'} ·{' '}
                      {s.status}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {cleanupPlans.length > 0 && <CleanupSection plans={cleanupPlans} />}

            <button
              type="button"
              className="btn2 primary"
              style={{ marginTop: 24, padding: '9px 20px', fontSize: 13 }}
              onClick={() => setScreen('catalog')}
            >
              Open Catalog <Icon name="chevR" size={12} />
            </button>
          </>
        )}
      </div>
    </div>
  );
}

const SOURCE_KIND_ICON: Record<string, 'disk' | 'cloud' | 'nas' | 'card' | 'iphone' | 'android'> = {
  local: 'disk',
  external: 'disk',
  nas: 'nas',
  sd: 'card',
  iphone: 'iphone',
  android: 'android',
  google_photos: 'cloud',
  icloud: 'cloud',
  onedrive: 'cloud',
  dropbox: 'cloud',
};
