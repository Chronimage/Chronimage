/**
 * SourcesPanel — catalog-sidebar sources list.
 *
 * Replaces the inert sources list that used to live in `CatalogSidePanel`.
 *
 * - Lists up to 7 sources (plus a "more" hint if there are more).
 * - A `+` button opens the `AddSourcePopover` inline.
 * - Clicking a source row opens an inline action menu with "Disconnect" —
 *   which triggers `useSourceDeletionPreview` and then a `ConfirmDialog`
 *   that lets the user choose whether to also remove orphan photos from
 *   the catalog and whether to recycle orphan files on disk.
 */

import { useState } from 'react';
import { ConfirmDialog } from '../../primitives/ConfirmDialog';
import { Icon, type IconName } from '../../primitives/Icon';
import type { SourceRow } from '../../state/queries';
import { useDeleteSource, useSourceDeletionPreview, useSources } from '../../state/queries';
import { AddSourcePopover } from './AddSourcePopover';

const KIND_ICON: Record<string, IconName> = {
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

function kindIcon(kind: string): IconName {
  return KIND_ICON[kind] ?? 'disk';
}

function statusDot(status: string): string {
  if (status === 'synced') return 'var(--accent)';
  if (status === 'syncing') return 'var(--info)';
  if (status === 'ready') return 'var(--warn)';
  return 'var(--fg-mute)';
}

function formatBytes(bytes: number): string {
  if (bytes >= 1e12) return `${(bytes / 1e12).toFixed(1)} TB`;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(0)} MB`;
  return `${(bytes / 1e3).toFixed(0)} KB`;
}

export function SourcesPanel() {
  const { data: sources = [] } = useSources();
  const [adding, setAdding] = useState(false);
  const [disconnectTarget, setDisconnectTarget] = useState<SourceRow | null>(null);

  return (
    <>
      <div
        className="section-label"
        style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
      >
        <span>Sources</span>
        <button
          type="button"
          className="btn2 ghost"
          onClick={() => setAdding((v) => !v)}
          aria-label={adding ? 'Close add-source panel' : 'Add a source'}
          style={{
            fontSize: 10,
            padding: '2px 6px',
            minWidth: 22,
            height: 18,
            lineHeight: 1,
          }}
          title="Add a source"
        >
          {adding ? '×' : '+'}
        </button>
      </div>

      {adding && (
        <div style={{ padding: '0 10px 10px' }}>
          <AddSourcePopover layout="inline" />
        </div>
      )}

      <div className="list">
        {sources.length === 0 && !adding && (
          <div style={{ fontSize: 11, color: 'var(--fg-mute)', padding: '4px 12px 10px' }}>
            No sources yet. Click <strong>+</strong> to add one.
          </div>
        )}
        {sources.slice(0, 7).map((s) => (
          <button
            key={s.id}
            type="button"
            className="item"
            onClick={() => setDisconnectTarget(s)}
            title={`${s.name} — click to disconnect`}
          >
            <span className="ico">
              <Icon name={kindIcon(s.kind)} size={13} />
            </span>
            <span
              style={{
                flex: 1,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                fontSize: 12,
              }}
            >
              {s.name.replace(/^.+· /, '')}
            </span>
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: statusDot(s.status),
              }}
            />
          </button>
        ))}
        {sources.length > 7 && (
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', padding: '4px 12px' }}>
            +{sources.length - 7} more
          </div>
        )}
      </div>

      {disconnectTarget && (
        <DisconnectSourceModal source={disconnectTarget} onClose={() => setDisconnectTarget(null)} />
      )}
    </>
  );
}

interface DisconnectSourceModalProps {
  source: SourceRow;
  onClose: () => void;
}

function DisconnectSourceModal({ source, onClose }: DisconnectSourceModalProps) {
  const preview = useSourceDeletionPreview(source.id);
  const deleteMut = useDeleteSource();

  function onConfirm(selected: Set<string>) {
    deleteMut.mutate(
      {
        sourceId: source.id,
        removeOrphanPhotos: selected.has('orphans'),
        recycleFiles: selected.has('recycle'),
      },
      {
        onSettled: () => onClose(),
      },
    );
  }

  const p = preview.data;
  const description = preview.isLoading ? (
    <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
      Calculating impact…
    </div>
  ) : p ? (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <div>
        <strong>{p.photos_total}</strong> photos are linked to this source.
      </div>
      {p.orphan_photos > 0 ? (
        <div>
          <strong>{p.orphan_photos}</strong> would become orphans (they don&rsquo;t exist in any other
          source).
        </div>
      ) : (
        <div>No photos would be orphaned — each photo exists in another source too.</div>
      )}
      {p.local_files > 0 && (
        <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-mute)' }}>
          {p.local_files} local files · {formatBytes(p.total_bytes)}
          {p.cloud_only > 0 && ` · ${p.cloud_only} cloud-only`}
        </div>
      )}
    </div>
  ) : null;

  const options = p
    ? [
        ...(p.orphan_photos > 0
          ? [
              {
                id: 'orphans',
                label: `Also remove ${p.orphan_photos} orphan photos from catalog`,
                description: 'Photos that only exist in this source will be deleted from Chronimage.',
                defaultChecked: true,
              },
            ]
          : []),
        ...(p.local_files > 0
          ? [
              {
                id: 'recycle',
                label: `Also move ${p.local_files} local files to Recycle Bin (${formatBytes(p.total_bytes)})`,
                description: 'Files can be restored from the Recycle Bin if you change your mind.',
                defaultChecked: false,
              },
            ]
          : []),
      ]
    : [];

  return (
    <ConfirmDialog
      open
      title={`Disconnect "${source.name}"?`}
      description={description}
      confirmLabel="Disconnect source"
      confirmTone="danger"
      options={options}
      busy={deleteMut.isPending || preview.isLoading}
      onCancel={onClose}
      onConfirm={onConfirm}
    />
  );
}
