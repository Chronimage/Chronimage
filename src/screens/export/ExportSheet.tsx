/**
 * ExportSheet — modal dialog for a new export job. Phase 2 §5.
 *
 * Layout: left pane is the per-photo queue (fills as the job runs);
 * right pane is the preset + start button. Once the job is enqueued the
 * caller should poll `export_run_next(jobId)` until it returns null.
 */

import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { useExportEnqueue, useExportRunNext } from '../../state/queries';
import {
  defaultExportPreset,
  EXPORT_PROGRESS_EVENT,
  type ExportFormat,
  type ExportPreset,
  type ExportProgress,
  gphotosUpload,
  gphotosUploadScopeOk,
  onedriveAuthStatus,
  onedriveUpload,
  type UploadReceipt,
} from '../../tauri/invoke';
import { debug, warn } from '../../util/log';

export interface ExportSheetProps {
  open: boolean;
  photoIds: number[];
  onClose: () => void;
}

interface ItemStatus {
  photoId: number;
  status: 'queued' | 'running' | 'done' | 'error';
  outputPath?: string;
  errorMsg?: string;
}

export function ExportSheet({ open, photoIds, onClose }: ExportSheetProps) {
  const [preset, setPreset] = useState<ExportPreset>(() => defaultExportPreset());
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [, setJobId] = useState<number | null>(null);
  const [items, setItems] = useState<ItemStatus[]>([]);
  const [running, setRunning] = useState(false);
  const [uploadGphotos, setUploadGphotos] = useState(false);
  const [uploadOnedrive, setUploadOnedrive] = useState(false);
  const [uploadStatus, setUploadStatus] = useState<string | null>(null);

  const enqueue = useExportEnqueue();
  const runNext = useExportRunNext();

  useEffect(() => {
    if (!open) return;
    setItems(photoIds.map((id) => ({ photoId: id, status: 'queued' })));
    setJobId(null);
    setRunning(false);
  }, [open, photoIds]);

  useEffect(() => {
    if (!open) return;
    const unlisten = listen<ExportProgress>(EXPORT_PROGRESS_EVENT, (event) => {
      const p = event.payload;
      setItems((prev) =>
        prev.map((it) =>
          it.photoId === p.photo_id
            ? {
                photoId: it.photoId,
                status: p.item_status,
                outputPath: p.output_path ?? undefined,
                errorMsg: p.error_msg ?? undefined,
              }
            : it,
        ),
      );
    });
    return () => {
      void unlisten.then((un) => un());
    };
  }, [open]);

  const pickOutputDir = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: 'Export to folder' });
    if (typeof picked === 'string') setOutputDir(picked);
  }, []);

  const startJob = useCallback(async () => {
    if (!outputDir || photoIds.length === 0) return;
    try {
      const id = await enqueue.mutateAsync({ photoIds, preset, outputDir });
      setJobId(id);
      setRunning(true);
      // Drive the queue until no more items remain.
      let next = await runNext.mutateAsync(id);
      while (next) {
        next = await runNext.mutateAsync(id);
      }
      debug('export: job complete', { jobId: id });

      // Phase 2 §6: after local export completes, fan out to upload
      // targets if the user ticked them. Each call is best-effort — a
      // failure here doesn't roll back the local export.
      if (uploadGphotos) {
        setUploadStatus('Uploading to Google Photos…');
        try {
          const scopeOk = await gphotosUploadScopeOk();
          if (!scopeOk) {
            setUploadStatus(
              'Google Photos: upload scope not granted. Reconnect from Settings → Sources to grant the photoslibrary.appendonly scope.',
            );
          } else {
            const r: UploadReceipt = await gphotosUpload(photoIds);
            setUploadStatus(
              `Google Photos: ${r.uploaded_count} uploaded, ${r.skipped_count} skipped${
                r.errors.length > 0 ? `, ${r.errors.length} errors` : ''
              }`,
            );
          }
        } catch (e) {
          warn('gphotos_upload failed', e);
          setUploadStatus(`Google Photos: upload failed (${String(e)})`);
        }
      }

      if (uploadOnedrive) {
        setUploadStatus((s) => (s ? `${s}\nUploading to OneDrive…` : 'Uploading to OneDrive…'));
        try {
          const authOk = await onedriveAuthStatus();
          if (!authOk) {
            setUploadStatus(
              'OneDrive: not connected. Connect from Settings → Sources first (requires Azure app registration per docs/manual-setup.md §3).',
            );
          } else {
            const r: UploadReceipt = await onedriveUpload(photoIds, 'Chronimage');
            setUploadStatus(
              (s) =>
                `${s ? `${s}\n` : ''}OneDrive: ${r.uploaded_count} uploaded, ${r.skipped_count} skipped${
                  r.errors.length > 0 ? `, ${r.errors.length} errors` : ''
                }`,
            );
          }
        } catch (e) {
          warn('onedrive_upload failed', e);
          setUploadStatus((s) => `${s ? `${s}\n` : ''}OneDrive: upload failed (${String(e)})`);
        }
      }

      setRunning(false);
    } catch (e) {
      warn('export: job failed', e);
      setRunning(false);
    }
  }, [enqueue, runNext, preset, outputDir, photoIds, uploadGphotos, uploadOnedrive]);

  const doneCount = items.filter((i) => i.status === 'done').length;
  const errorCount = items.filter((i) => i.status === 'error').length;
  const pct = items.length > 0 ? Math.round(((doneCount + errorCount) / items.length) * 100) : 0;

  const outputSize = useMemo(() => {
    // Rough estimate: long-edge² × 3 bytes × quality/100 × N.
    const perImage = (preset.long_edge_px * preset.long_edge_px * 3 * preset.quality) / 100 / 5;
    return ((perImage * photoIds.length) / 1024 / 1024).toFixed(0);
  }, [preset, photoIds.length]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !running) onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, running, onClose]);

  if (!open) return null;

  return (
    <button type="button" className="export-backdrop" onClick={onClose} aria-label="Close export sheet">
      <div
        className="export-sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Export photos"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div className="export-head">
          <h2>
            Export {photoIds.length} photo{photoIds.length === 1 ? '' : 's'}
            <em>.</em>
          </h2>
          <button type="button" className="btn" onClick={onClose} aria-label="Close">
            <Icon name="close" size={14} />
          </button>
        </div>

        <div className="export-body">
          {/* Left pane: queue */}
          <div className="export-queue">
            <div className="mono section-label">QUEUE</div>
            <div className="export-progress">
              <div style={{ width: `${pct}%` }} />
            </div>
            <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 6 }}>
              {doneCount}/{items.length} done · {errorCount} errors
            </div>
            <div className="export-items">
              {items.map((item) => (
                <div key={item.photoId} className={`export-item status-${item.status}`}>
                  <span className="mono" style={{ fontSize: 11 }}>
                    #{item.photoId}
                  </span>
                  <span className="export-item-status">
                    {item.status === 'queued' && '…'}
                    {item.status === 'running' && 'running'}
                    {item.status === 'done' && '✓'}
                    {item.status === 'error' && `! ${item.errorMsg ?? 'error'}`}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Right pane: preset */}
          <div className="export-preset">
            <div className="mono section-label">FORMAT</div>
            <div style={{ display: 'flex', gap: 6, marginBottom: 10 }}>
              {(['jpeg', 'tiff', 'heic'] as ExportFormat[]).map((f) => (
                <button
                  key={f}
                  type="button"
                  className={`btn${preset.format === f ? ' on' : ''}`}
                  onClick={() => setPreset({ ...preset, format: f })}
                  disabled={f === 'heic'}
                  title={f === 'heic' ? 'HEIC deferred — pick JPEG or TIFF' : f.toUpperCase()}
                >
                  {f.toUpperCase()}
                </button>
              ))}
            </div>

            <div className="mono section-label">QUALITY</div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <input
                type="range"
                min={1}
                max={100}
                value={preset.quality}
                onChange={(e) => setPreset({ ...preset, quality: Number(e.target.value) })}
                aria-label="JPEG quality"
                style={{ flex: 1 }}
              />
              <span className="mono" style={{ fontSize: 12, width: 30 }}>
                {preset.quality}
              </span>
            </div>

            <div className="mono section-label">LONG EDGE</div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <input
                type="range"
                min={800}
                max={8000}
                step={200}
                value={preset.long_edge_px}
                onChange={(e) => setPreset({ ...preset, long_edge_px: Number(e.target.value) })}
                aria-label="Long edge pixels"
                style={{ flex: 1 }}
              />
              <span className="mono" style={{ fontSize: 12, width: 56 }}>
                {preset.long_edge_px}px
              </span>
            </div>

            <div className="mono section-label">OPTIONS</div>
            <label className="export-toggle">
              <input
                type="checkbox"
                checked={preset.strip_meta.gps}
                onChange={(e) =>
                  setPreset({
                    ...preset,
                    strip_meta: { ...preset.strip_meta, gps: e.target.checked },
                  })
                }
              />{' '}
              Strip GPS
            </label>
            <label className="export-toggle">
              <input
                type="checkbox"
                checked={preset.strip_meta.all_exif}
                onChange={(e) =>
                  setPreset({
                    ...preset,
                    strip_meta: { ...preset.strip_meta, all_exif: e.target.checked },
                  })
                }
              />{' '}
              Strip all EXIF
            </label>
            <label className="export-toggle">
              <input
                type="checkbox"
                checked={preset.archive_originals}
                onChange={(e) => setPreset({ ...preset, archive_originals: e.target.checked })}
              />{' '}
              Archive originals alongside
            </label>

            <div className="mono section-label">UPLOAD TARGETS</div>
            <label className="export-toggle">
              <input
                type="checkbox"
                checked={uploadGphotos}
                onChange={(e) => setUploadGphotos(e.target.checked)}
              />{' '}
              Also upload to Google Photos
            </label>
            <label className="export-toggle">
              <input
                type="checkbox"
                checked={uploadOnedrive}
                onChange={(e) => setUploadOnedrive(e.target.checked)}
              />{' '}
              Also upload to OneDrive
            </label>
            {uploadStatus && (
              <div
                className="mono"
                style={{
                  fontSize: 10.5,
                  color: 'var(--fg-mute)',
                  marginTop: 6,
                  whiteSpace: 'pre-line',
                  padding: '6px 8px',
                  background: 'var(--bg-elev)',
                  borderRadius: 6,
                }}
              >
                {uploadStatus}
              </div>
            )}

            <div className="mono section-label">WATERMARK</div>
            <input
              type="text"
              value={preset.watermark_text ?? ''}
              onChange={(e) =>
                setPreset({
                  ...preset,
                  watermark_text: e.target.value.trim() || null,
                })
              }
              placeholder="© your name"
              className="export-input"
            />

            <div className="mono section-label">OUTPUT</div>
            <button
              type="button"
              className="btn"
              onClick={pickOutputDir}
              style={{ width: '100%', justifyContent: 'space-between' }}
            >
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {outputDir ?? 'Pick output folder…'}
              </span>
              <Icon name="chevR" size={12} />
            </button>

            <div className="export-stats">
              <Chip>Est. {outputSize} MB</Chip>
              <Chip>{photoIds.length} photos</Chip>
            </div>
          </div>
        </div>

        <div className="export-footer">
          <button type="button" className="btn" onClick={onClose} disabled={running}>
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={startJob}
            disabled={!outputDir || running || photoIds.length === 0}
          >
            {running ? 'Running…' : `Start ${photoIds.length} task${photoIds.length === 1 ? '' : 's'}`}
          </button>
        </div>
      </div>
    </button>
  );
}
