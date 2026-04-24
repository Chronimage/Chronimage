/**
 * CullBinScreen — Phase 2 recoverable-rejects list. Full UI stub over live
 * catalog photos pretending to be "recently rejected"; Restore / Delete are
 * phase-gated until the verdict engine lands (Phase 2 §3).
 */

import { useCallback, useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import { usePhotos } from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';

const MOCK_REASONS = [
  'Near-duplicate of previous',
  'Sharpness 0.18',
  'Eyes closed',
  'Burst 4/6',
  'Screenshot',
  'Low-res web export',
  'Near-duplicate — same subject',
];

const MOCK_AGES = ['12m ago', '34m ago', '2h ago', '4h ago', 'yesterday', '2 days ago', '3 days ago'];

interface BinRow {
  photo: PhotoRow;
  reason: string;
  when: string;
  sizeMb: string;
}

function buildBinRows(photos: PhotoRow[]): BinRow[] {
  return photos.slice(0, 14).map((p, i) => ({
    photo: p,
    reason: MOCK_REASONS[i % MOCK_REASONS.length] ?? 'Rejected',
    when: MOCK_AGES[i % MOCK_AGES.length] ?? 'recently',
    sizeMb: p.size_bytes ? (p.size_bytes / 1024 / 1024).toFixed(1) : '—',
  }));
}

export function CullBinScreen() {
  const { data: photos = [], isLoading } = usePhotos();
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const rows = useMemo(() => buildBinRows(photos), [photos]);

  const toggleSelect = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  if (isLoading) {
    return (
      <div className="canvas">
        <div style={{ padding: 40, color: 'var(--fg-mute)', fontSize: 13 }}>Loading cull bin…</div>
      </div>
    );
  }

  if (rows.length === 0) {
    return (
      <div className="canvas">
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
            CULL BIN · EMPTY
          </div>
          <h1 className="page-title">
            Nothing rejected
            <em>.</em>
          </h1>
          <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>
            When you reject a photo from the Cull screen, it lands here for 30 days before permanent deletion.
            No data loss possible until you explicitly empty the bin.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="canvas">
      <div className="toolbar">
        <div>
          <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
            CULL BIN · RECOVERABLE
          </div>
          <div style={{ fontSize: 14, marginTop: 2 }}>
            {rows.length} items · 8.2 GB · kept until you confirm
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
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 2 · Cull Bin restore"
            >
              <Icon name="keep" size={13} /> Restore
            </button>
            <button
              type="button"
              className="btn danger phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 2 · Cull Bin delete"
            >
              <Icon name="reject" size={13} /> Delete forever
            </button>
            <div className="divider" />
          </>
        )}
        <button
          type="button"
          className="btn primary phase-gated"
          disabled
          aria-disabled="true"
          title="Coming in Phase 2 · Cull Bin empty"
        >
          <Icon name="reject" size={13} /> Empty bin
        </button>
      </div>

      <div
        style={{ flex: 1, overflowY: 'auto', padding: 18, display: 'flex', flexDirection: 'column', gap: 8 }}
      >
        {rows.map((row) => {
          const isSel = selected.has(row.photo.id);
          return (
            <button
              key={row.photo.id}
              type="button"
              className="cullbin-row"
              onClick={() => toggleSelect(row.photo.id)}
              aria-pressed={isSel}
              data-selected={isSel}
            >
              <div style={{ width: 80, height: 60, flexShrink: 0, position: 'relative' }}>
                <Thumbnail
                  photoId={row.photo.id}
                  sizePx={320}
                  photo={{
                    hue: (row.photo.id * 31) % 360,
                    filename: row.photo.filename,
                    id: String(row.photo.id),
                  }}
                />
              </div>
              <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 4 }}>
                <div className="mono" style={{ fontSize: 12, color: 'var(--fg)' }}>
                  {row.photo.filename}
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
                  <Chip tone="warn">{row.reason}</Chip>
                  <span className="mono">Rejected {row.when}</span>
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
                {row.sizeMb} MB
              </div>
              <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                <button
                  type="button"
                  className="btn phase-gated"
                  disabled
                  aria-disabled="true"
                  title="Coming in Phase 2 · Cull Bin restore"
                  onClick={(e) => e.stopPropagation()}
                  style={{ fontSize: 11.5 }}
                >
                  Restore
                </button>
                <button
                  type="button"
                  className="btn phase-gated"
                  disabled
                  aria-disabled="true"
                  title="Coming in Phase 2 · Cull Bin delete"
                  onClick={(e) => e.stopPropagation()}
                  style={{ fontSize: 11.5, color: 'var(--warn)' }}
                >
                  Delete
                </button>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
