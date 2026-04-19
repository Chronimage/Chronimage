/**
 * OnboardScreen — 5-step stepper onboarding flow.
 *
 * Step 1: Welcome   — catalog home location picker (UI only; move pipeline is Phase 1b)
 * Step 2: Sources   — connect libraries (local, iCloud, iPhone, Google Photos)
 * Step 3: Import    — live import progress wired to IMPORT_PROGRESS_EVENT
 * Step 4: Models    — AI model picker with GPU auto-detect + download progress
 * Step 5: People    — face cluster naming grid
 */

import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useEffect, useRef, useState } from 'react';
import { Chip } from '../primitives/Chip';
import { Icon } from '../primitives/Icon';
import {
  type CleanupPlan,
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  type LiftPlan,
  useCleanupDryRun,
  useCreateSource,
  useDeleteSource,
  useDetectIcloudPath,
  useDownloadModels,
  useImportGoogleTakeout,
  useIphoneDevices,
  useLiftShiftDryRun,
  useLiftShiftExecute,
  useSources,
  useStartImport,
} from '../state/queries';
import { useUi } from '../state/ui';
import {
  DOWNLOAD_PROGRESS_EVENT,
  type DownloadProgressEvent,
  detectHardware,
  type HardwareInfo,
} from '../tauri/invoke';
import { debug } from '../util/log';

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

interface ModelDownloadState {
  downloadedBytes: number;
  totalBytes: number;
  done: boolean;
  alreadyInstalled: boolean;
}

type CatalogMode = 'consolidate' | 'index_in_place';

type StepId = 'welcome' | 'sources' | 'import' | 'models' | 'people';

interface Step {
  id: StepId;
  t: string;
  s: string;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const STEPS: Step[] = [
  { id: 'welcome', t: 'Welcome', s: 'Choose your catalog home' },
  { id: 'sources', t: 'Sources', s: 'Connect every library' },
  { id: 'import', t: 'Import', s: 'Indexing in progress' },
  { id: 'models', t: 'Models', s: 'Pick your AI' },
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
  onAddGoogleTakeout: () => Promise<void>;
  onAddIcloud: () => Promise<void>;
  onImportIphone: (deviceId: string, deviceName: string) => Promise<void>;
  busy: boolean;
  error: Error | null;
}

function OnbSources({
  activeImports,
  onAddLocalFolder,
  onAddGoogleTakeout,
  onAddIcloud,
  onImportIphone,
  busy,
  error,
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
          onClick={onAddGoogleTakeout}
          disabled={busy}
          title="Select a Google Photos Takeout export folder"
        >
          <Icon name="cloud" size={14} /> Google Photos
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
        <div style={{ marginTop: 10, fontSize: 12, color: 'var(--warn, #b07a00)' }}>
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
            <span style={{ color: 'var(--warn, #b07a00)' }}> · {receipt.errors.length} warnings</span>
          )}
        </div>
      )}
    </div>
  );
}

// ── Step 4: Models ────────────────────────────────────────────────────────────

interface ModelTier {
  category: string;
  name: string;
  alternates: string;
  sub: string;
  downloadKey: string;
}

const MODEL_TIERS: ModelTier[] = [
  {
    category: 'Cataloging & scenes',
    name: 'gemma4-27b',
    alternates: 'gemma4-9b · LLaVA-1.6',
    sub: '17.3 GB · VRAM 12GB · best quality',
    downloadKey: 'gemma4-27b',
  },
  {
    category: 'Semantic search',
    name: 'CLIP-L14',
    alternates: 'SigLIP',
    sub: '1.4 GB · fastest recall',
    downloadKey: 'clip-l14',
  },
  {
    category: 'Face recognition',
    name: 'ArcFace R100',
    alternates: 'InsightFace',
    sub: 'Local encrypted DB · 58 people',
    downloadKey: 'arcface-r100',
  },
  {
    category: 'Prompt / generative',
    name: 'Flux-dev',
    alternates: 'SDXL inpaint · Cloud',
    sub: '12 GB · on-device inpainting',
    downloadKey: 'flux-dev',
  },
];

function OnbModels() {
  const downloadModels = useDownloadModels();
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [modelProgress, setModelProgress] = useState<Map<string, ModelDownloadState>>(new Map());
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    detectHardware()
      .then((hw) => setHardware(hw))
      .catch((err: unknown) => {
        debug('detectHardware failed', err);
      });
  }, []);

  useEffect(() => {
    let cancelled = false;
    listen<DownloadProgressEvent>(DOWNLOAD_PROGRESS_EVENT, (event) => {
      const p = event.payload;
      setModelProgress((prev) => {
        const next = new Map(prev);
        next.set(p.model_name, {
          downloadedBytes: p.downloaded_bytes,
          totalBytes: p.total_bytes,
          done: p.done,
          alreadyInstalled: p.already_installed,
        });
        return next;
      });
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlistenRef.current = unlisten;
      })
      .catch((err: unknown) => {
        debug('DOWNLOAD_PROGRESS_EVENT listen failed', err);
      });
    return () => {
      cancelled = true;
      unlistenRef.current?.();
    };
  }, []);

  function handleDownloadAll() {
    downloadModels.mutate(undefined);
  }

  function handleDownloadOne(modelKey: string) {
    downloadModels.mutate([modelKey]);
  }

  const hwBadge =
    hardware == null
      ? null
      : hardware.tier === 'CpuOnly'
        ? 'CPU only'
        : `${hardware.adapter_name} · ${(hardware.vram_mb / 1024).toFixed(0)} GB VRAM`;

  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 4 · MODELS
      </div>
      <h1 className="onb-title">
        Pick the models
        <br />
        that <em>read your photos.</em>
      </h1>

      {hwBadge && (
        <div
          className="mono"
          style={{
            fontSize: 11,
            color: 'var(--accent)',
            marginBottom: 14,
            display: 'flex',
            alignItems: 'center',
            gap: 6,
          }}
        >
          <Icon name="ai" size={13} />
          {hwBadge}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, marginTop: 18 }}>
        {MODEL_TIERS.map((r) => {
          const prog = modelProgress.get(r.downloadKey);
          const pct =
            prog && prog.totalBytes > 0 ? Math.round((prog.downloadedBytes / prog.totalBytes) * 100) : 0;
          const isInstalled = prog?.done || prog?.alreadyInstalled;
          const isDownloading = prog != null && !prog.done && !prog.alreadyInstalled;

          return (
            <div
              key={r.category}
              style={{
                padding: 14,
                border: '1px solid var(--stroke)',
                borderRadius: 10,
                background: 'var(--bg-elev)',
              }}
            >
              <div className="mono" style={{ fontSize: 10, color: 'var(--fg-mute)', marginBottom: 6 }}>
                {r.category.toUpperCase()}
              </div>
              <div style={{ fontSize: 16, fontFamily: 'var(--display-font)', marginBottom: 4 }}>{r.name}</div>
              <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 10 }}>
                {r.sub}
              </div>
              <div style={{ fontSize: 11, color: 'var(--fg-dim)', marginBottom: 10 }}>
                Alternates · {r.alternates}
              </div>

              {isDownloading && (
                <div className="progress" style={{ marginBottom: 8 }}>
                  <div style={{ width: `${pct}%`, background: 'var(--info)' }} />
                </div>
              )}

              {isInstalled ? (
                <Chip variant="solid">Installed</Chip>
              ) : (
                <button
                  type="button"
                  className="btn2"
                  style={{ padding: '5px 12px', fontSize: 11 }}
                  disabled={downloadModels.isPending}
                  onClick={() => handleDownloadOne(r.downloadKey)}
                >
                  <Icon name="download" size={12} />
                  {isDownloading ? `${pct}%` : 'Download'}
                </button>
              )}
            </div>
          );
        })}
      </div>

      <div
        style={{
          marginTop: 18,
          padding: '12px 14px',
          background: 'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))',
          border: '1px solid color-mix(in oklch, var(--accent) 30%, var(--stroke))',
          borderRadius: 10,
          display: 'flex',
          alignItems: 'center',
          gap: 12,
        }}
      >
        <Icon name="ai" size={20} />
        <div style={{ flex: 1, fontSize: 12.5, color: 'var(--fg-dim)' }}>
          <strong style={{ color: 'var(--fg)' }}>All on-device.</strong> Nothing leaves your PC unless you
          pick a cloud model. You can swap models later without re-indexing.
        </div>
        <button
          type="button"
          className="btn2 primary"
          style={{ padding: '7px 14px', fontSize: 12, flexShrink: 0 }}
          onClick={handleDownloadAll}
          disabled={downloadModels.isPending}
        >
          <Icon name="download" size={13} />
          {downloadModels.isPending ? 'Downloading…' : 'Download all'}
        </button>
      </div>

      {downloadModels.error && (
        <div className="mono" style={{ fontSize: 11, color: 'var(--danger)', marginTop: 10 }}>
          {downloadModels.error instanceof Error
            ? downloadModels.error.message
            : String(downloadModels.error)}
        </div>
      )}
    </div>
  );
}

// ── Step 5: People ────────────────────────────────────────────────────────────

interface FaceCluster {
  id: number;
  ct: number;
  /** Hue angles for placeholder face visuals */
  hues: number[];
}

// TODO(cc): Replace stub data with a real usePeopleClusters() hook once the
// face clustering pipeline (ArcFace + HDBSCAN) lands in Phase 1c.
const STUB_CLUSTERS: FaceCluster[] = [
  { id: 0, ct: 3240, hues: [210, 140, 300] },
  { id: 1, ct: 1922, hues: [60, 180, 270] },
  { id: 2, ct: 982, hues: [30, 90, 200] },
  { id: 3, ct: 611, hues: [120, 240, 10] },
  { id: 4, ct: 711, hues: [80, 160, 320] },
  { id: 5, ct: 587, hues: [200, 40, 100] },
  { id: 6, ct: 244, hues: [260, 330, 150] },
  { id: 7, ct: 189, hues: [350, 70, 190] },
];

function OnbPeople() {
  const [names, setNames] = useState<Record<number, string>>({});

  function handleNameChange(id: number, value: string) {
    setNames((prev) => ({ ...prev, [id]: value }));
  }

  return (
    <div>
      <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 8 }}>
        STEP 5 · NAME PEOPLE
      </div>
      <h1 className="onb-title">
        Name them once.
        <br />
        <em>Forever categorised</em> — even for photos you import tomorrow.
      </h1>
      <p style={{ color: 'var(--fg-dim)', fontSize: 13, maxWidth: 560, marginBottom: 20 }}>
        Halide clustered {STUB_CLUSTERS.length * 100}+ distinct faces. Name the ones you care about — the rest
        stay unnamed and private. New photos auto-assign as they arrive.
      </p>
      <div className="person-grid">
        {STUB_CLUSTERS.map((p) => (
          <div key={p.id} className="person-card">
            <div className="faces">
              {p.hues.map((hue, i) => (
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
                placeholder={`Unnamed · cluster ${p.id + 1}`}
                value={names[p.id] ?? ''}
                onChange={(e) => handleNameChange(p.id, e.target.value)}
                aria-label={`Name for cluster ${p.id + 1}`}
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
              <span>{p.ct.toLocaleString()} photos</span>
              <button type="button" style={{ color: 'var(--fg-mute)' }}>
                Merge…
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Root component ────────────────────────────────────────────────────────────

export function OnboardScreen() {
  const [stepIdx, setStepIdx] = useState<number>(0);
  const [catalogMode, setCatalogMode] = useState<CatalogMode>('consolidate');

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
              onAddLocalFolder={handleAddLocalFolder}
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
          {step === 'models' && <OnbModels />}
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
