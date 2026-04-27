/**
 * DuplicatesPanel — surfaces `find_duplicates` groups over the Catalog.
 *
 * Opens as a modal over the CatalogScreen. Each group lists the member
 * thumbnails (via the real `<Thumbnail>`) alongside a similarity score and
 * Exact/Near badge. In Phase 2 the "Keep one" action will become a cull
 * action; for now the UI is read-only review.
 */

import { useEffect } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import { useDuplicates } from '../../state/queries';

export interface DuplicatesPanelProps {
  onClose: () => void;
}

export function DuplicatesPanel({ onClose }: DuplicatesPanelProps) {
  const { data: groups = [], isLoading, isError } = useDuplicates(0.9);
  const totalDupes = groups.reduce((sum, g) => sum + Math.max(0, g.photo_ids.length - 1), 0);

  // Escape closes the modal — matches ClusterDrillDown + catalog DetailView.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Duplicates"
      style={{
        position: 'absolute',
        inset: 0,
        background: 'var(--bg)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 10,
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--space-3)',
          padding: 'var(--space-3) var(--space-5)',
          borderBottom: '1px solid var(--stroke)',
        }}
      >
        <button
          type="button"
          className="btn2"
          onClick={onClose}
          style={{ padding: '4px 10px', fontSize: 12 }}
        >
          <Icon name="chevL" size={12} /> Back
        </button>
        <h2 style={{ margin: 0, fontSize: 22 }}>
          Duplicates<em>.</em>
        </h2>
        <span className="mono" style={{ fontSize: 11.5, color: 'var(--fg-mute)', letterSpacing: '0.06em' }}>
          {groups.length} {groups.length === 1 ? 'GROUP' : 'GROUPS'} · {totalDupes}{' '}
          {totalDupes === 1 ? 'REDUNDANT PHOTO' : 'REDUNDANT PHOTOS'}
        </span>
      </div>

      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: 'var(--space-5) var(--space-5) var(--space-6)',
        }}
      >
        {isLoading && (
          <div className="mono" style={{ fontSize: 12, color: 'var(--fg-mute)' }}>
            Scanning for duplicates…
          </div>
        )}
        {isError && (
          <div className="mono" style={{ fontSize: 12, color: 'var(--danger)' }}>
            Failed to load duplicates.
          </div>
        )}
        {!isLoading && !isError && groups.length === 0 && (
          <div
            style={{
              padding: '60px var(--space-5)',
              textAlign: 'center',
              border: '1px dashed var(--stroke)',
              borderRadius: 'var(--radius-lg)',
              color: 'var(--fg-mute)',
            }}
          >
            <Icon name="layers" size={36} />
            <div style={{ marginTop: 'var(--space-3)', fontSize: 14 }}>
              No near-duplicates detected. Import more photos or lower the similarity threshold to find more
              candidates.
            </div>
          </div>
        )}

        {groups.length > 0 && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-5)' }}>
            {groups.map((g, idx) => {
              const sim = Math.round(g.max_similarity * 100);
              return (
                <section
                  // biome-ignore lint/suspicious/noArrayIndexKey: groups have no stable id and order is deterministic
                  key={`group-${idx}`}
                  style={{
                    border: '1px solid var(--stroke)',
                    borderRadius: 'var(--radius-md)',
                    background: 'var(--bg-elev)',
                    padding: 'var(--space-3)',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 'var(--space-3)',
                      marginBottom: 'var(--space-3)',
                    }}
                  >
                    <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                      GROUP {idx + 1}
                    </span>
                    <Chip variant={g.kind === 'Exact' ? 'solid' : undefined} tone="info">
                      {g.kind} · {sim}%
                    </Chip>
                    <span className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)' }}>
                      {g.photo_ids.length} photos
                    </span>
                  </div>
                  <div
                    style={{
                      display: 'grid',
                      gridTemplateColumns: `repeat(${Math.min(g.photo_ids.length, 6)}, 1fr)`,
                      gap: 'var(--space-1)',
                    }}
                  >
                    {g.photo_ids.map((pid) => (
                      <div key={pid} className="cell" style={{ aspectRatio: '3/2', position: 'relative' }}>
                        <Thumbnail
                          photoId={pid}
                          sizePx={320}
                          photo={{ hue: (pid * 31) % 360, filename: `#${pid}`, id: String(pid) }}
                          subtle
                        />
                      </div>
                    ))}
                  </div>
                </section>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
