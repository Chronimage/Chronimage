/**
 * CullBinScreen — Phase 2 §3 recoverable-rejects list. Rows come from the real
 * `cull_bin` table (populated by `cull_apply_verdict`). Restore/Delete route
 * through the Rust commands; a daily sweep background task handles the 30-day
 * auto-empty.
 */

import { useCallback, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { ConfirmDialog } from '../../primitives/ConfirmDialog';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import {
  type CullBinFilter,
  type CullBinRow,
  useCullBin,
  useCullBinDeleteForever,
  useCullBinRestore,
  useCullBinSummary,
} from '../../state/queries';

function reasonLabel(reason: string): string {
  switch (reason) {
    case 'near_dup':
      return 'Near-duplicate';
    case 'blur':
      return 'Out of focus';
    case 'eyes_closed':
      return 'Eyes closed';
    case 'exposure':
      return 'Over/under exposed';
    case 'duplicate':
      return 'Duplicate';
    case 'flag':
      return 'Flagged';
    case 'user':
      return 'Rejected by user';
    default:
      return 'Other';
  }
}

function fmtAge(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diff = Math.max(0, now - then);
  const mins = Math.round(diff / 60_000);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.round(hrs / 24);
  return `${days}d ago`;
}

export interface CullBinScreenProps {
  filter?: CullBinFilter;
  embedded?: boolean;
}

export function CullBinScreen({ filter, embedded = false }: CullBinScreenProps = {}) {
  const { data: rows = [], isLoading } = useCullBin(filter);
  const { data: _summary } = useCullBinSummary();
  const restore = useCullBinRestore();
  const deleteForever = useCullBinDeleteForever();

  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirmEmpty, setConfirmEmpty] = useState(false);
  const [confirmDeleteSelected, setConfirmDeleteSelected] = useState(false);

  const toggleSelect = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const selectedIds = useMemo(() => [...selected], [selected]);
  const totalBytes = useMemo(() => rows.reduce((a, r) => a + (r.size_bytes ?? 0), 0), [rows]);
  const totalGb = (totalBytes / 1024 ** 3).toFixed(1);

  const onRestoreSelected = useCallback(() => {
    restore.mutate(selectedIds, {
      onSuccess: () => setSelected(new Set()),
    });
  }, [restore, selectedIds]);

  const onDeleteSelected = useCallback(() => {
    deleteForever.mutate(selectedIds, {
      onSuccess: () => {
        setSelected(new Set());
        setConfirmDeleteSelected(false);
      },
    });
  }, [deleteForever, selectedIds]);

  const onEmptyBin = useCallback(() => {
    const allIds = rows.map((r) => r.photo_id);
    deleteForever.mutate(allIds, {
      onSuccess: () => {
        setSelected(new Set());
        setConfirmEmpty(false);
      },
    });
  }, [rows, deleteForever]);

  if (isLoading) {
    const content = (
      <div style={{ padding: 40, color: 'var(--fg-mute)', fontSize: 13 }}>Loading rejected photos…</div>
    );
    return embedded ? content : <div className="canvas">{content}</div>;
  }

  if (rows.length === 0) {
    const content = (
      <div
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 14,
          padding: 48,
          textAlign: 'center',
        }}
      >
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em' }}>
          REJECTED · EMPTY
        </div>
        <h1 className="page-title">
          Nothing rejected
          <em>.</em>
        </h1>
        <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>
          Rejected photos stay recoverable here before permanent deletion. Review them from this Cull workflow
          before you empty anything.
        </p>
      </div>
    );
    return embedded ? content : <div className="canvas">{content}</div>;
  }

  const content = (
    <>
      <div className="toolbar">
        <div>
          <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
            CULL · REJECTED
          </div>
          <div style={{ fontSize: 14, marginTop: 2 }}>
            {rows.length} recoverable items · {totalGb} GB · kept until you confirm
          </div>
        </div>
        <div style={{ flex: 1 }} />
        {selected.size > 0 && (
          <>
            <span className="mono" style={{ fontSize: 11, color: 'var(--accent)' }}>
              {selected.size} selected
            </span>
            <button
              type="button"
              className="btn"
              onClick={onRestoreSelected}
              disabled={restore.isPending}
              title="Restore selected to catalog"
            >
              <Icon name="keep" size={13} /> Restore
            </button>
            <button
              type="button"
              className="btn danger"
              onClick={() => setConfirmDeleteSelected(true)}
              disabled={deleteForever.isPending}
              title="Permanently delete selected"
            >
              <Icon name="reject" size={13} /> Delete forever
            </button>
            <div className="divider" />
          </>
        )}
        <button
          type="button"
          className="btn primary"
          onClick={() => setConfirmEmpty(true)}
          disabled={deleteForever.isPending}
        >
          <Icon name="reject" size={13} /> Empty bin
        </button>
      </div>

      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: 18,
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        {rows.map((row: CullBinRow) => {
          const isSel = selected.has(row.photo_id);
          const sizeMb = row.size_bytes != null ? (row.size_bytes / 1024 / 1024).toFixed(1) : '—';
          return (
            <div key={row.photo_id} className="cullbin-row" data-selected={isSel}>
              <div style={{ width: 80, height: 60, flexShrink: 0, position: 'relative' }}>
                <Thumbnail
                  photoId={row.photo_id}
                  sizePx={320}
                  photo={{
                    hue: (row.photo_id * 31) % 360,
                    filename: row.filename,
                    id: String(row.photo_id),
                  }}
                />
              </div>
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 4,
                }}
              >
                <div className="mono" style={{ fontSize: 12, color: 'var(--fg)' }}>
                  {row.filename}
                </div>
                <div
                  style={{
                    fontSize: 11,
                    color: 'var(--fg-mute)',
                    display: 'flex',
                    gap: 10,
                    alignItems: 'center',
                  }}
                >
                  <Chip tone="warn">{reasonLabel(row.reason)}</Chip>
                  <span className="mono">Rejected {fmtAge(row.rejected_at)}</span>
                </div>
              </div>
              <div
                className="mono"
                style={{
                  fontSize: 11,
                  color: 'var(--fg-dim)',
                  flexShrink: 0,
                  minWidth: 60,
                  textAlign: 'right',
                }}
              >
                {sizeMb} MB
              </div>
              <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                <button
                  type="button"
                  className="btn"
                  style={{ fontSize: 11.5 }}
                  onClick={() => toggleSelect(row.photo_id)}
                  aria-pressed={isSel}
                >
                  {isSel ? 'Selected' : 'Select'}
                </button>
                <button
                  type="button"
                  className="btn"
                  style={{ fontSize: 11.5 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    restore.mutate([row.photo_id]);
                  }}
                  disabled={restore.isPending}
                >
                  Restore
                </button>
                <button
                  type="button"
                  className="btn danger"
                  style={{ fontSize: 11.5 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    deleteForever.mutate([row.photo_id]);
                  }}
                  disabled={deleteForever.isPending}
                >
                  Delete
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <ConfirmDialog
        open={confirmEmpty}
        title="Empty cull bin permanently?"
        description={`This will permanently delete ${rows.length} photos from your catalog. Originals on disk are not touched (use "Move to Recycle Bin" in Catalog for that).`}
        confirmLabel="Empty bin"
        confirmTone="danger"
        onConfirm={onEmptyBin}
        onCancel={() => setConfirmEmpty(false)}
      />

      <ConfirmDialog
        open={confirmDeleteSelected}
        title={`Delete ${selected.size} photo${selected.size === 1 ? '' : 's'} forever?`}
        description="Permanently removes the selected rows from the catalog. Originals on disk are not touched."
        confirmLabel="Delete forever"
        confirmTone="danger"
        onConfirm={onDeleteSelected}
        onCancel={() => setConfirmDeleteSelected(false)}
      />
    </>
  );

  return embedded ? content : <div className="canvas">{content}</div>;
}
