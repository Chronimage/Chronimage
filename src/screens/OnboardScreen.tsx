/**
 * OnboardScreen — 4-step stepper onboarding flow.
 *
 * Step 1: Welcome   — catalog home location picker (UI only; move pipeline is Phase 1b)
 * Step 2: Sources   — connect libraries (local, iCloud, iPhone, Google Photos)
 * Step 3: Import    — live import progress wired to IMPORT_PROGRESS_EVENT
 * Step 4: People    — face cluster naming grid
 *
 * Model selection is NOT part of onboarding. Default models ship bundled in
 * the installer; power users swap per-feature from Settings → AI Models
 * (see `docs/adr/0003-bundled-default-models.md`).
 */

import { useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useEffect, useRef, useState } from 'react';
import { Chip } from '../primitives/Chip';
import { Icon } from '../primitives/Icon';
import {
  type CleanupExecuteResult,
  type CleanupPlan,
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  type LiftPlan,
  useCleanupDryRun,
  useCleanupExecute,
  useCreateSource,
  useDeleteSource,
  useDetectIcloudPath,
  useFaceClusterName,
  useFaceClusters,
  useImportGoogleTakeout,
  useIphoneDevices,
  useLiftShiftDryRun,
  useLiftShiftExecute,
  useSources,
  useStartImport,
} from '../state/queries';
import { useUi } from '../state/ui';
import { debug, errorMessage } from '../util/log';

// ── Types ─────────────────────────────────────────────────────────────────────

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

type CatalogMode = 'consolidate' | 'index_in_place';

type StepId = 'welcome' | 'sources' | 'import' | 'people';

interface Step {
  id: StepId;
  t: string;
  s: string;
}

// ── Constants ─────────────────────────────────────────────────────────────────

// Default models ship bundled in the installer (see ADR 0003); there is no
// "Models" step — power users swap models from Settings → AI Models.
const STEPS: Step[] = [
  { id: 'welcome', t: 'Welcome', s: 'Choose your catalog home' },
  { id: 'sources', t: 'Sources', s: 'Connect every library' },
  { id: 'import', t: 'Import', s: 'Indexing in progress' },
  { id: 'people', t: 'Name people', s: 'So faces stick for life' },
];

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

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function getSourceKind(sourceName: string): string {
  const lower = sourceName.toLowerCase();
  if (lower.includes('google')) return 'google_photos';
  if (lower.includes('icloud')) return 'icloud';
  if (lower.includes('iphone')) return 'iphone';
  if (lower.includes('nas')) return 'nas';
  return 'local';
}

// ── Sub-components ────────────────────────────────────────────────────────────

interface CleanupSectionProps {
  plan: CleanupPlan;
}

function CleanupSection({ plan }: CleanupSectionProps) {
  const execute = useCleanupExecute();
  const [confirming, setConfirming] = useState(false);
  const [result, setResult] = useState<CleanupExecuteResult | null>(null);

  const busy = execute.isPending;
  const executed = result !== null;
  const mutationErr = execute.error ? String(execute.error) : null;

  function handlePrimary() {
    if (executed) return;
    if (!confirming) {
      setConfirming(true);
      return;
    }
    execute.mutate(
      { planId: plan.plan_id, confirmToken: plan.confirm_token },
      {
        onSuccess: (data) => {
          setResult(data);
          setConfirming(false);
        },
      },
    );
  }

  let primaryLabel: string;
  if (busy) primaryLabel = 'Deleting…';
  else if (executed) primaryLabel = 'Done';
  else if (confirming) primaryLabel = `Yes — free ${fmtBytes(plan.total_reclaimable_bytes)}`;
  else primaryLabel = `Free up ${fmtBytes(plan.total_reclaimable_bytes)}`;

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
            {fmtBytes(plan.total_reclaimable_bytes)}
          </span>
          <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
            {plan.total_file_count.toLocaleString()} photos across {plan.sources.length}{' '}
            {plan.sources.length === 1 ? 'source' : 'sources'}
          </span>
        </div>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {plan.sources.map((src) => (
          <div
            key={src.source_id}
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
            <Icon name={SOURCE_KIND_ICON[getSourceKind(src.source_name)] ?? 'disk'} size={14} />
            <span style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>{src.source_name}</span>
            <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
              {src.file_count.toLocaleString()} photos
            </span>
            <span className="mono" style={{ fontSize: 12, color: 'var(--accent)', fontWeight: 600 }}>
              {fmtBytes(src.reclaimable_bytes)}
            </span>
          </div>
        ))}
      </div>
      {confirming && !executed && (
        <div
          style={{
            marginTop: 'var(--space-3)',
            padding: 'var(--space-3)',
            border: '1px solid var(--warn)',
            borderRadius: 'var(--radius-md)',
            background: 'color-mix(in oklch, var(--warn) 8%, var(--bg-elev))',
            fontSize: 12,
            color: 'var(--fg)',
          }}
        >
          <strong>This is the second confirmation.</strong> {plan.total_file_count.toLocaleString()} source
          files across {plan.sources.length} {plan.sources.length === 1 ? 'source' : 'sources'} will be
          deleted. Local copies remain; this only removes the originals from Google Photos, iCloud, iPhone
          storage, etc. Continue?
        </div>
      )}
      {executed && result && (
        <div
          style={{
            marginTop: 'var(--space-3)',
            padding: 'var(--space-3)',
            border: '1px solid var(--accent)',
            borderRadius: 'var(--radius-md)',
            background: 'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))',
            fontSize: 12,
            color: 'var(--fg)',
          }}
        >
          Freed <strong>{fmtBytes(result.freed_bytes)}</strong> · deleted{' '}
          {result.deleted_count.toLocaleString()} source {result.deleted_count === 1 ? 'file' : 'files'}
          {result.errors.length > 0 && (
            <span style={{ color: 'var(--warn)' }}>
              {' '}
              · {result.errors.length} {result.errors.length === 1 ? 'error' : 'errors'}
            </span>
          )}
        </div>
      )}
      {mutationErr && !executed && (
        <div
          style={{
            marginTop: 'var(--space-3)',
            padding: 'var(--space-3)',
            border: '1px solid var(--danger)',
            borderRadius: 'var(--radius-md)',
            background: 'color-mix(in oklch, var(--danger) 10%, var(--bg-elev))',
            fontSize: 12,
          }}
        >
          Cleanup failed: {mutationErr}
        </div>
      )}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginTop: 'var(--space-3)',
          gap: 'var(--space-3)',
        }}
      >
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)' }}>
          Local copies verified by SHA256. Source files will not be deleted without a second confirmation.
        </div>
        <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
          {confirming && !executed && (
            <button
              type="button"
              className="btn2 ghost"
              style={{ padding: '6px 12px', fontSize: 11 }}
              onClick={() => setConfirming(false)}
              disabled={busy}
            >
              Cancel
            </button>
          )}
          <button
            type="button"
            className={confirming && !executed ? 'solid btn2' : 'btn2'}
            style={{ padding: '6px 12px', fontSize: 11 }}
            onClick={handlePrimary}
            disabled={busy || executed || plan.total_file_count === 0}
          >
            {primaryLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Step 1: Welcome ───────────────────────────────────────────────────────────

interface OnbWelcomeProps {
  catalogMode: CatalogMode;
  onSelectMode: (mode: CatalogMode) => void;
}

function OnbWelcome({ catalogMode, onSelectMode }: OnbWelcomeProps) {
  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 1 · CATALOG HOME
      </div>
      <h1 className="onb-title">
        Lift &amp; shift
        <br />
        your <em>whole library</em> into one place.
      </h1>
      <p style={{ color: 'var(--fg-dim)', fontSize: 13.5, lineHeight: 1.55, maxWidth: 560 }}>
        Halide can unify fragmented photo libraries — D:/Photos, old exports, Dropbox archives, random SD
        dumps — into a single catalog. Originals stay untouched; a manifest makes them browseable, movable,
        and restorable in one shot.
      </p>

      <div style={{ marginTop: 24, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
        <button
          type="button"
          className={`preset-card${catalogMode === 'consolidate' ? ' on' : ''}`}
          onClick={() => onSelectMode('consolidate')}
        >
          <div
            style={{
              width: 42,
              height: 42,
              borderRadius: 8,
              background: 'color-mix(in oklch, var(--accent) 25%, var(--bg))',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: 'var(--accent)',
            }}
          >
            <Icon name="disk" size={18} />
          </div>
          <div>
            <div className="name">Consolidate into D:/Halide</div>
            <div className="sub">Copy originals · 2.4 TB free of 4 TB</div>
          </div>
          <Chip variant="solid">Recommended</Chip>
        </button>
        <button
          type="button"
          className={`preset-card${catalogMode === 'index_in_place' ? ' on' : ''}`}
          onClick={() => onSelectMode('index_in_place')}
        >
          <div
            style={{
              width: 42,
              height: 42,
              borderRadius: 8,
              background: 'var(--bg)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: 'var(--fg-dim)',
            }}
          >
            <Icon name="layers" size={18} />
          </div>
          <div>
            <div className="name">Index in place</div>
            <div className="sub">Read-only · no files move</div>
          </div>
        </button>
      </div>

      <div
        style={{
          marginTop: 18,
          padding: 14,
          background: 'var(--bg-elev)',
          border: '1px solid var(--stroke)',
          borderRadius: 10,
        }}
      >
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 6 }}>
          CATALOG LOCATION
        </div>
        <div style={{ fontFamily: 'var(--mono-font)', fontSize: 13 }}>D:/Halide/</div>
        <div
          className="mono"
          style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 10, display: 'flex', gap: 24 }}
        >
          <span>Est. move: 1.8 TB · 6,412 folders</span>
          <span>ETA: 4h 12m on this disk</span>
          <span style={{ color: 'var(--accent)' }}>Bit-exact · checksummed</span>
        </div>
      </div>
    </div>
  );
}

// ── Step 2: Sources ───────────────────────────────────────────────────────────

interface OnbSourcesProps {
  activeImports: Map<number, ActiveImport>;
  onAddLocalFolder: () => Promise<void>;
  onConnectGooglePhotos: () => Promise<void>;
  onAddGoogleTakeout: () => Promise<void>;
  onAddIcloud: () => Promise<void>;
  onImportIphone: (deviceId: string, deviceName: string) => Promise<void>;
  busy: boolean;
  error: Error | null;
  gphotosStatus:
    | { kind: 'idle' }
    | { kind: 'signing-in' }
    | { kind: 'picker-open'; sessionId: string }
    | { kind: 'importing'; importId: number }
    | { kind: 'error'; message: string };
}

function OnbSources({
  activeImports,
  onAddLocalFolder,
  onConnectGooglePhotos,
  onAddGoogleTakeout,
  onAddIcloud,
  onImportIphone,
  busy,
  error,
  gphotosStatus,
}: OnbSourcesProps) {
  const { data: sources = [] } = useSources();
  const deleteSource = useDeleteSource();
  const { data: icloudPath } = useDetectIcloudPath();
  const { data: iphoneDevices = [] } = useIphoneDevices();
  const cleanupDryRun = useCleanupDryRun();
  const cleanupPlan = cleanupDryRun.data ?? null;

  const runningImports = [...activeImports.values()].filter((i) => !i.finished);
  const finishedImports = [...activeImports.values()].filter((i) => i.finished);
  const hasAnySources = sources.length > 0;

  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 2 · SOURCES
      </div>
      <h1 className="onb-title">
        Connect <em>everywhere</em> your photos live.
      </h1>

      {/* ── Active imports ── */}
      {runningImports.length > 0 && (
        <div style={{ marginTop: 16, display: 'flex', flexDirection: 'column', gap: 10 }}>
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
                  style={{ fontSize: 10.5, color: 'var(--accent)', marginBottom: 8, letterSpacing: '0.08em' }}
                >
                  <Icon name="usb" size={11} /> IMPORTING · {imp.sourceName || 'local folder'}
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
      <div style={{ marginTop: 20, display: 'flex', gap: 10, flexWrap: 'wrap' }}>
        <button
          type="button"
          className="btn2 primary"
          style={{ padding: '9px 16px', fontSize: 13 }}
          onClick={onAddLocalFolder}
          disabled={busy}
        >
          <Icon name="disk" size={14} />
          {busy ? 'Adding…' : 'Add local folder'}
        </button>
        <button
          type="button"
          className="btn2"
          style={{ padding: '9px 16px', fontSize: 13 }}
          onClick={onConnectGooglePhotos}
          disabled={busy}
          title="Sign in with Google and pick photos via the Photo Picker"
        >
          <Icon name="cloud" size={14} /> Google Photos
        </button>
        <button
          type="button"
          className="btn2"
          style={{ padding: '9px 16px', fontSize: 13, opacity: 0.85 }}
          onClick={onAddGoogleTakeout}
          disabled={busy}
          title="Already downloaded a Google Photos Takeout export? Pick the folder."
        >
          <Icon name="cloud" size={14} /> Takeout folder
        </button>
        <button
          type="button"
          className="btn2"
          style={{ padding: '9px 16px', fontSize: 13 }}
          onClick={onAddIcloud}
          disabled={busy}
          title={icloudPath ? `Detected: ${icloudPath}` : 'Select your iCloud Photos folder'}
        >
          <Icon name="cloud" size={14} />
          {icloudPath ? 'iCloud (detected)' : 'iCloud Photos'}
        </button>
        {iphoneDevices.length > 0 ? (
          iphoneDevices.map((dev) => {
            const label = dev.friendly_name || dev.description || 'iPhone';
            const alreadyAdded = sources.some((s) => s.kind === 'iphone' && s.name.includes(label));
            return (
              <button
                key={dev.device_id}
                type="button"
                className="btn2"
                style={{ padding: '9px 16px', fontSize: 13 }}
                onClick={() => onImportIphone(dev.device_id, label)}
                disabled={busy || alreadyAdded}
                title={alreadyAdded ? `${label} already added` : `Add ${label} as a source`}
              >
                <Icon name="iphone" size={14} /> {label}
              </button>
            );
          })
        ) : (
          <button
            type="button"
            className="btn2"
            style={{ padding: '9px 16px', fontSize: 13 }}
            disabled
            title="Connect an iPhone via USB to enable"
          >
            <Icon name="iphone" size={14} /> iPhone USB
          </button>
        )}
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
        <div className="mono" style={{ fontSize: 11, color: 'var(--danger)', marginTop: 10 }}>
          {error.message}
        </div>
      )}

      {/* ── Google Photos status banner ── */}
      {gphotosStatus.kind !== 'idle' && (
        <div
          style={{
            marginTop: 14,
            padding: '10px 12px',
            border: '1px solid var(--stroke)',
            borderRadius: 6,
            fontSize: 12,
            color: gphotosStatus.kind === 'error' ? 'var(--danger)' : 'var(--fg)',
            background: 'var(--bg-elev)',
          }}
        >
          {gphotosStatus.kind === 'signing-in' && (
            <>
              <strong>Google Photos:</strong> Sign in in your browser. This pane updates automatically when
              you finish.
            </>
          )}
          {gphotosStatus.kind === 'picker-open' && (
            <>
              <strong>Google Photos:</strong> Pick the photos you want to import in the browser tab that just
              opened. We'll start the download once you're done.
            </>
          )}
          {gphotosStatus.kind === 'importing' && (
            <>
              <strong>Google Photos:</strong> Downloading picked photos + running import #
              {gphotosStatus.importId}. Track progress below.
            </>
          )}
          {gphotosStatus.kind === 'error' && (
            <>
              <strong>Google Photos:</strong> {gphotosStatus.message}
            </>
          )}
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
              marginTop: 24,
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
                <button
                  type="button"
                  className="btn2"
                  style={{ padding: '4px 10px', fontSize: 11, color: 'var(--fg-mute)' }}
                  onClick={() => deleteSource.mutate(s.id)}
                  disabled={deleteSource.isPending}
                  title="Remove source"
                  aria-label={`Remove ${s.name}`}
                >
                  <Icon name="close" size={11} />
                </button>
              </div>
            ))}
          </div>
          {cleanupPlan && cleanupPlan.sources.length > 0 && <CleanupSection plan={cleanupPlan} />}
        </>
      )}
    </div>
  );
}

// ── Step 3: Import ────────────────────────────────────────────────────────────

interface OnbImportProps {
  activeImports: Map<number, ActiveImport>;
}

function OnbImport({ activeImports }: OnbImportProps) {
  const importList = [...activeImports.values()];
  const totalDone = importList.reduce((sum, i) => sum + i.done, 0);
  const totalAll = importList.reduce((sum, i) => sum + i.total, 0);
  const finishedCount = importList.filter((i) => i.finished).length;

  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 3 · IMPORT
      </div>
      <h1 className="onb-title">
        Indexing <em>{totalAll > 0 ? totalAll.toLocaleString() : '—'}</em> photos.
      </h1>
      <p style={{ color: 'var(--fg-dim)', fontSize: 13, maxWidth: 560 }}>
        Faces, scenes, OCR, duplicates, and CLIP embeddings. You can keep setting things up — we'll keep
        chewing in the background.
      </p>

      {importList.length > 0 ? (
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10, marginTop: 18 }}>
          {importList.map((imp) => {
            const pct = imp.total > 0 ? Math.round((imp.done / imp.total) * 100) : 0;
            const statusLabel = imp.finished
              ? `Done`
              : imp.total === 0
                ? 'Waiting'
                : `Syncing · ${imp.etaSeconds != null ? `~${imp.etaSeconds}s remaining` : '…'}`;
            return (
              <div key={imp.importId} className="progress-card">
                <div className="pc-head">
                  <span>{imp.sourceName || 'Import'}</span>
                  <span className="mono" style={{ color: pct === 100 ? 'var(--accent)' : 'var(--fg-dim)' }}>
                    {pct}%
                  </span>
                </div>
                <div className="progress">
                  <div
                    style={{
                      width: `${pct}%`,
                      background: pct === 100 ? 'var(--accent)' : 'var(--info)',
                    }}
                  />
                </div>
                <div className="pc-stats">
                  <span>{imp.done.toLocaleString()} photos</span>
                  <span>{statusLabel}</span>
                </div>
              </div>
            );
          })}
        </div>
      ) : (
        <div
          style={{
            marginTop: 18,
            padding: 20,
            border: '1px dashed var(--stroke)',
            borderRadius: 10,
            textAlign: 'center',
            color: 'var(--fg-mute)',
            fontSize: 13,
          }}
        >
          No imports started yet. Go back to Sources to connect a library.
        </div>
      )}

      {importList.length > 0 && (
        <div
          style={{
            marginTop: 18,
            padding: 14,
            background: 'var(--bg-elev)',
            border: '1px solid var(--stroke)',
            borderRadius: 10,
            display: 'grid',
            gridTemplateColumns: '1fr 1fr 1fr 1fr',
            gap: 14,
          }}
        >
          <div>
            <div className="display" style={{ fontSize: 28 }}>
              {totalDone > 0 ? `${(totalDone / 1000).toFixed(0)}K` : '0'}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)' }}>
              INDEXED
            </div>
          </div>
          <div>
            <div className="display" style={{ fontSize: 28 }}>
              {finishedCount}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)' }}>
              SOURCES DONE
            </div>
          </div>
          <div>
            <div className="display" style={{ fontSize: 28 }}>
              {importList.length - finishedCount}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)' }}>
              STILL RUNNING
            </div>
          </div>
          <div>
            <div
              className="display"
              style={{
                fontSize: 28,
                color: importList.every((i) => i.finished) ? 'var(--accent)' : 'var(--fg)',
              }}
            >
              {importList.every((i) => i.finished && i.total > 0) ? 'Done' : '…'}
            </div>
            <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)' }}>
              STATUS
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Step 3b: Lift & Shift panel (shown on import step when mode=consolidate) ──

function OnbLiftPanel() {
  const [targetRoot, setTargetRoot] = useState<string>('');
  const [plan, setPlan] = useState<LiftPlan | null>(null);
  const dryRun = useLiftShiftDryRun();
  const execute = useLiftShiftExecute();

  async function pickTarget(): Promise<void> {
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== 'string') return;
    setTargetRoot(selected);
    setPlan(null);
  }

  async function previewPlan(): Promise<void> {
    if (!targetRoot) return;
    const result = await dryRun.mutateAsync({ targetRoot });
    setPlan(result);
  }

  async function runExecute(): Promise<void> {
    if (!plan) return;
    await execute.mutateAsync({ planId: plan.plan_id, confirmToken: plan.confirm_token });
    setPlan(null);
  }

  const receipt = execute.data;
  const gb = plan ? (plan.total_bytes / 1_073_741_824).toFixed(2) : '0.00';

  return (
    <div
      style={{
        marginTop: 20,
        padding: 14,
        borderRadius: 10,
        border: '1px solid var(--br)',
        background: 'var(--panel)',
      }}
    >
      <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 10 }}>
        CONSOLIDATE INTO ONE LIBRARY
      </div>

      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 10 }}>
        <button type="button" className="btn2 ghost" onClick={pickTarget}>
          <Icon name="disk" size={13} /> {targetRoot ? 'Change target…' : 'Pick target folder…'}
        </button>
        {targetRoot && (
          <code style={{ fontSize: 11, color: 'var(--fg-dim)' }} title={targetRoot}>
            {targetRoot.length > 48 ? `…${targetRoot.slice(-45)}` : targetRoot}
          </code>
        )}
      </div>

      <div style={{ display: 'flex', gap: 8 }}>
        <button
          type="button"
          className="btn2 ghost"
          onClick={previewPlan}
          disabled={!targetRoot || dryRun.isPending}
        >
          {dryRun.isPending ? 'Planning…' : 'Preview consolidation'}
        </button>
        {plan && plan.total_file_count > 0 && (
          <button
            type="button"
            className="btn2 primary"
            onClick={runExecute}
            disabled={execute.isPending || !plan.free_space_ok}
            title={plan.free_space_ok ? '' : 'Target drive free space < 1.5× plan size'}
          >
            {execute.isPending ? 'Consolidating…' : `Consolidate ${plan.total_file_count} files (${gb} GB)`}
          </button>
        )}
      </div>

      {plan && plan.total_file_count === 0 && (
        <div style={{ marginTop: 10, fontSize: 12, color: 'var(--fg-dim)' }}>
          Nothing to move — every photo already lives under this target root.
        </div>
      )}

      {plan && !plan.free_space_ok && (
        <div style={{ marginTop: 10, fontSize: 12, color: 'var(--warn)' }}>
          Target drive does not have 1.5× the plan size free. Pick a different drive.
        </div>
      )}

      {dryRun.error && (
        <div style={{ marginTop: 10, fontSize: 12, color: 'var(--danger, #c33)' }}>
          Plan failed: {String(dryRun.error.message ?? dryRun.error)}
        </div>
      )}

      {receipt && (
        <div style={{ marginTop: 10, fontSize: 12, color: 'var(--fg-dim)' }}>
          Copied {receipt.copied_count} files ({(receipt.bytes_copied / 1_073_741_824).toFixed(2)} GB).
          Manifest: <code>{receipt.manifest_path}</code>
          {receipt.errors.length > 0 && (
            <span style={{ color: 'var(--warn)' }}> · {receipt.errors.length} warnings</span>
          )}
        </div>
      )}
    </div>
  );
}

// ── Step 5: People ────────────────────────────────────────────────────────────

function OnbPeople() {
  const { data: clusters = [], isLoading } = useFaceClusters(12);
  const nameMutation = useFaceClusterName();
  const [drafts, setDrafts] = useState<Record<number, string>>({});

  function handleDraft(id: number, value: string) {
    setDrafts((prev) => ({ ...prev, [id]: value }));
  }

  function handleCommit(id: number, currentName: string | null) {
    const draft = drafts[id];
    if (typeof draft !== 'string') return;
    const trimmed = draft.trim();
    if (trimmed === (currentName ?? '')) return;
    nameMutation.mutate({ clusterId: id, name: trimmed });
  }

  const topClusters = [...clusters].sort((a, b) => b.faceCount - a.faceCount).slice(0, 12);
  const totalFaces = clusters.reduce((sum, c) => sum + c.faceCount, 0);

  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 4 · NAME PEOPLE
      </div>
      <h1 className="onb-title">
        Name them once.
        <br />
        <em>Forever categorised</em> — even for photos you import tomorrow.
      </h1>
      <p style={{ color: 'var(--fg-dim)', fontSize: 13, maxWidth: 560, marginBottom: 'var(--space-5)' }}>
        {isLoading
          ? 'Clustering faces…'
          : clusters.length === 0
            ? 'No face clusters yet — finish your first import and clustering will populate this step. You can always come back from Settings → People.'
            : `Chronimage grouped ${totalFaces.toLocaleString()} faces into ${clusters.length} ${
                clusters.length === 1 ? 'cluster' : 'clusters'
              }. Name the ones you care about — the rest stay unnamed and private. New photos auto-assign as they arrive.`}
      </p>
      {topClusters.length > 0 && (
        <div className="person-grid">
          {topClusters.map((c) => {
            const hueBase = (c.id * 47) % 360;
            const hues = [hueBase, (hueBase + 80) % 360, (hueBase + 160) % 360];
            return (
              <div key={c.id} className="person-card">
                <div className="faces">
                  {hues.map((hue, i) => (
                    <div
                      // biome-ignore lint/suspicious/noArrayIndexKey: stable order within each cluster
                      key={i}
                      className="face"
                      style={{
                        background: `oklch(0.55 0.15 ${hue})`,
                        backgroundImage:
                          'repeating-linear-gradient(-45deg, transparent 0 4px, rgba(255,255,255,0.08) 4px 5px)',
                      }}
                    />
                  ))}
                </div>
                <div className="meta">
                  <input
                    placeholder={`Unnamed · cluster ${c.id}`}
                    value={drafts[c.id] ?? c.name ?? ''}
                    onChange={(e) => handleDraft(c.id, e.target.value)}
                    onBlur={() => handleCommit(c.id, c.name)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        e.currentTarget.blur();
                      }
                    }}
                    aria-label={`Name for cluster ${c.id}`}
                  />
                </div>
                <div
                  className="mono"
                  style={{
                    fontSize: 10.5,
                    color: 'var(--fg-mute)',
                    display: 'flex',
                    justifyContent: 'space-between',
                  }}
                >
                  <span>
                    {c.faceCount.toLocaleString()} {c.faceCount === 1 ? 'face' : 'faces'}
                  </span>
                  {c.isNamed && <span style={{ color: 'var(--accent)' }}>named</span>}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Root component ────────────────────────────────────────────────────────────

export function OnboardScreen() {
  const [stepIdx, setStepIdx] = useState<number>(0);
  const [catalogMode, setCatalogMode] = useState<CatalogMode>('consolidate');
  const queryClient = useQueryClient();

  // Live status for the Google Photos flow. Rendered inline under the
  // Sources step so the user isn't guessing whether the click landed.
  const [gphotosStatus, setGphotosStatus] = useState<
    | { kind: 'idle' }
    | { kind: 'signing-in' }
    | { kind: 'picker-open'; sessionId: string }
    | { kind: 'importing'; importId: number }
    | { kind: 'error'; message: string }
  >({ kind: 'idle' });

  const { data: sources = [] } = useSources();
  const createSource = useCreateSource();
  const startImport = useStartImport();
  const importTakeout = useImportGoogleTakeout();
  const { data: icloudPath } = useDetectIcloudPath();
  const setScreen = useUi((s) => s.setScreen);
  const appName = useUi((s) => s.tweaks.appName);

  const [activeImports, setActiveImports] = useState<Map<number, ActiveImport>>(new Map());
  const unlistenRef = useRef<(() => void) | null>(null);

  // stepIdx is bounded to [0, STEPS.length-1] by goBack/goNext, so this is always defined.
  // biome-ignore lint/style/noNonNullAssertion: stepIdx is always in range
  const step = STEPS[stepIdx]!.id;

  // ── Import progress listener ──────────────────────────────────────────────
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
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlistenRef.current = unlisten;
      })
      .catch((err: unknown) => {
        debug('IMPORT_PROGRESS_EVENT listen failed', err);
      });
    return () => {
      cancelled = true;
      unlistenRef.current?.();
    };
  }, []);

  // ── Source connection handlers ────────────────────────────────────────────

  async function handleAddLocalFolder(): Promise<void> {
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

  async function handleConnectGooglePhotos(): Promise<void> {
    // Full flow:
    //   1. If not signed in → open OAuth in browser, poll flow until
    //      completed.
    //   2. Ensure the `sources` row exists (deterministic via
    //      gphotos_ensure_source_row; the earlier background-task
    //      pattern raced the TanStack cache and left the UI blank).
    //   3. Invalidate useSources() so the row appears in the list.
    //   4. Create a Photo Picker session; open the picker URL in the
    //      browser.
    //   5. Poll the session until mediaItemsSet = true.
    //   6. Kick off import_google_photos with the source_id from step 2.
    //   7. Record in activeImports so the existing progress surface
    //      tracks it.
    const {
      gphotosBeginOauthFlow,
      gphotosPollOauthFlow,
      gphotosAuthStatus,
      gphotosEnsureSourceRow,
      gphotosCreatePickerSession,
      gphotosPollPickerSession,
      gphotosDeletePickerSession,
      importGooglePhotos,
    } = await import('../tauri/invoke');
    const { open: openShell } = await import('@tauri-apps/plugin-shell');

    setGphotosStatus({ kind: 'signing-in' });

    try {
      // Step 1: OAuth if needed.
      let connected = await gphotosAuthStatus();
      if (!connected) {
        const { auth_url, flow_id } = await gphotosBeginOauthFlow();
        await openShell(auth_url);
        const startedAt = Date.now();
        while (Date.now() - startedAt < 305_000) {
          const status = await gphotosPollOauthFlow(flow_id);
          if (status.state === 'completed') {
            connected = true;
            break;
          }
          if (status.state === 'failed') {
            setGphotosStatus({ kind: 'error', message: status.message });
            return;
          }
          if (status.state === 'timed_out') {
            setGphotosStatus({
              kind: 'error',
              message: 'Sign-in timed out. Close the browser tab and try again.',
            });
            return;
          }
          await new Promise((r) => setTimeout(r, 1000));
        }
        if (!connected) {
          setGphotosStatus({ kind: 'error', message: 'Sign-in did not complete.' });
          return;
        }
      }

      // Step 2 + 3: ensure source row + refresh the list.
      const sourceRow = await gphotosEnsureSourceRow();
      await queryClient.invalidateQueries({ queryKey: ['sources'] });

      // Step 4: open the Photo Picker.
      const picker = await gphotosCreatePickerSession();
      if (!picker.pickerUri) {
        setGphotosStatus({
          kind: 'error',
          message:
            'Photo Picker returned no URL — check that the Photo Picker API is enabled on your Google Cloud project.',
        });
        return;
      }
      await openShell(picker.pickerUri);
      setGphotosStatus({ kind: 'picker-open', sessionId: picker.id });

      // Step 5: poll until user finishes picking. 10 min cap.
      const pickerDeadline = Date.now() + 600_000;
      let ready = false;
      while (Date.now() < pickerDeadline) {
        const snap = await gphotosPollPickerSession(picker.id);
        if (snap.mediaItemsSet) {
          ready = true;
          break;
        }
        await new Promise((r) => setTimeout(r, 3000));
      }
      if (!ready) {
        setGphotosStatus({
          kind: 'error',
          message:
            "Didn't detect any photos picked — close the Chronimage picker tab and click Google Photos again to retry.",
        });
        return;
      }

      // Step 6: import.
      const resp = await importGooglePhotos(sourceRow.id, picker.id);
      setGphotosStatus({ kind: 'importing', importId: resp.import_id });
      setActiveImports((prev) => {
        const next = new Map(prev);
        next.set(resp.import_id, {
          importId: resp.import_id,
          sourceId: sourceRow.id,
          sourceName: sourceRow.name,
          total: 0,
          done: 0,
          currentFile: '',
          etaSeconds: null,
          finished: false,
        });
        return next;
      });
      try {
        await gphotosDeletePickerSession(picker.id);
      } catch (err) {
        debug('gphotos onboard: delete picker session failed (non-fatal)', err);
      }
    } catch (err) {
      debug('gphotos onboard: connect flow failed', err);
      setGphotosStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  async function handleAddGoogleTakeout(): Promise<void> {
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== 'string') return;

    const folderName = selected.split(/[\\/]/).pop() ?? selected;
    const source = await createSource.mutateAsync({
      name: `Google Photos · ${folderName}`,
      kind: 'google_photos',
      rootPath: selected,
    });

    const resp = await importTakeout.mutateAsync({ sourceId: source.id, root: selected });
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

  async function handleAddIcloud(): Promise<void> {
    let root = icloudPath ?? null;
    if (!root) {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Select your iCloud Photos folder',
      });
      if (!selected || typeof selected !== 'string') return;
      root = selected;
    }

    const source = await createSource.mutateAsync({
      name: 'iCloud Photos',
      kind: 'icloud',
      rootPath: root,
    });

    const resp = await startImport.mutateAsync({ sourceId: source.id, root });
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

  async function handleImportIphone(deviceId: string, deviceName: string): Promise<void> {
    if (sources.some((s) => s.kind === 'iphone' && s.name.includes(deviceName))) return;
    await createSource.mutateAsync({
      name: `iPhone · ${deviceName}`,
      kind: 'iphone',
      rootPath: deviceId,
    });
  }

  const busy = createSource.isPending || startImport.isPending || importTakeout.isPending;
  const sourceError = createSource.error ?? startImport.error ?? importTakeout.error;
  const sourceErrorNormalized =
    sourceError instanceof Error ? sourceError : sourceError != null ? new Error(String(sourceError)) : null;

  // ── Navigation ────────────────────────────────────────────────────────────

  function goBack() {
    setStepIdx((i) => Math.max(0, i - 1));
  }

  function goNext() {
    setStepIdx((i) => Math.min(STEPS.length - 1, i + 1));
  }

  function openCatalog() {
    setScreen('catalog');
  }

  // ── Render ────────────────────────────────────────────────────────────────

  const appSuffix = appName.slice(-2);
  const appPrefix = appName.slice(0, -2);

  return (
    <div className="canvas" style={{ gridColumn: '2 / -1' }}>
      <div className="onb-wrap">
        {/* ── Left panel: branding + stepper ── */}
        <div className="onb-left">
          <div>
            <div
              className="mono"
              style={{
                fontSize: 10.5,
                color: 'var(--fg-mute)',
                marginBottom: 14,
                letterSpacing: '0.1em',
              }}
            >
              WELCOME TO
            </div>
            <h1>
              {appPrefix}
              <em>{appSuffix}.</em>
            </h1>
            <div className="caption">
              Your photos, <em style={{ fontStyle: 'italic', color: 'var(--fg)' }}>in one light.</em>
              <br />
              On-device AI · no re-uploads · works on RAW.
            </div>
          </div>

          <div className="onb-steps">
            {STEPS.map((s, i) => (
              <div
                key={s.id}
                className={`onb-step${i === stepIdx ? ' on' : ''}${i < stepIdx ? ' done' : ''}`}
              >
                <div className="n">{i < stepIdx ? '✓' : i + 1}</div>
                <div>
                  <div className="t">{s.t}</div>
                  <div className="s">{s.s}</div>
                </div>
              </div>
            ))}
          </div>

          <div
            style={{
              paddingTop: 20,
              fontSize: 11,
              color: 'var(--fg-mute)',
              fontFamily: 'var(--mono-font)',
            }}
          >
            Skip setup — you can reconnect anytime.
          </div>
        </div>

        {/* ── Right panel: step content ── */}
        <div className="onb-right">
          {step === 'welcome' && <OnbWelcome catalogMode={catalogMode} onSelectMode={setCatalogMode} />}
          {step === 'sources' && (
            <OnbSources
              activeImports={activeImports}
              gphotosStatus={gphotosStatus}
              onAddLocalFolder={handleAddLocalFolder}
              onConnectGooglePhotos={handleConnectGooglePhotos}
              onAddGoogleTakeout={handleAddGoogleTakeout}
              onAddIcloud={handleAddIcloud}
              onImportIphone={handleImportIphone}
              busy={busy}
              error={sourceErrorNormalized}
            />
          )}
          {step === 'import' && (
            <>
              <OnbImport activeImports={activeImports} />
              {catalogMode === 'consolidate' && <OnbLiftPanel />}
            </>
          )}
          {step === 'people' && <OnbPeople />}

          {/* ── Navigation actions ── */}
          <div className="onb-actions">
            <button
              type="button"
              className="btn2 ghost"
              onClick={goBack}
              disabled={stepIdx === 0}
              style={{ opacity: stepIdx === 0 ? 0.4 : 1 }}
            >
              <Icon name="chevL" size={13} /> Back
            </button>
            <div style={{ display: 'flex', gap: 10 }}>
              {stepIdx < STEPS.length - 1 ? (
                <button type="button" className="btn2 primary" onClick={goNext}>
                  Continue <Icon name="chevR" size={13} />
                </button>
              ) : (
                <button type="button" className="btn2 primary" onClick={openCatalog}>
                  Open Catalog <Icon name="chevR" size={13} />
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
