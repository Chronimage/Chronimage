import { Icon } from '../../primitives/Icon';
import { useAlbums, useFaceClusters, useSources } from '../../state/queries';
import { ImportProgressCard } from './ImportProgressCard';
import { SourcesPanel } from './SourcesPanel';

export interface CatalogSidePanelProps {
  albumId: string;
  onAlbumChange: (id: string) => void;
}

export function CatalogSidePanel({ albumId, onAlbumChange }: CatalogSidePanelProps) {
  const { data: albums = [] } = useAlbums();
  const { data: sources = [] } = useSources();
  const { data: clusters = [] } = useFaceClusters(60);

  const totalPhotos = sources.reduce((sum, s) => sum + s.photo_count, 0);
  const nonCullAlbums = albums.filter((a) => a.tag !== 'cull').slice(0, 9);
  // Show up to 6 people in the sidebar, preferring named clusters, then largest.
  const sidebarClusters = [...clusters]
    .sort((a, b) => {
      if (a.isNamed !== b.isNamed) return a.isNamed ? -1 : 1;
      return b.faceCount - a.faceCount;
    })
    .slice(0, 6);

  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Catalog</h3>
        <span className="count">{totalPhotos > 0 ? totalPhotos.toLocaleString() : '—'}</span>
      </div>

      <div className="sidepanel-body">
        <ImportProgressCard />

        <div className="section-label">
          <span>Smart Albums</span>
          <span className="ai-badge on">
            <Icon name="ai" size={10} /> AI
          </span>
        </div>
        <div className="list">
          <button
            type="button"
            className={`item ${albumId === 'all' ? 'active' : ''}`}
            onClick={() => onAlbumChange('all')}
          >
            <span className="ico">
              <Icon name="layers" size={14} />
            </span>
            All Photos
            <span className="n">
              {totalPhotos > 999 ? `${(totalPhotos / 1000).toFixed(0)}K` : totalPhotos || '—'}
            </span>
          </button>
          {nonCullAlbums.map((a) => (
            <button
              type="button"
              key={a.id}
              className={`item ${albumId === String(a.id) ? 'active' : ''}`}
              onClick={() => onAlbumChange(String(a.id))}
            >
              <span className="ico">
                <Icon name={a.tag === 'faces' || a.tag === 'people' ? 'faces' : 'tag'} size={13} />
              </span>
              <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {a.name}
              </span>
              <span className="n">
                {a.photo_count > 999 ? `${(a.photo_count / 1000).toFixed(1)}K` : a.photo_count}
              </span>
            </button>
          ))}
        </div>

        <div className="section-label">
          <span>People</span>
          <span className="ai-badge on">{clusters.length}</span>
        </div>
        <div style={{ padding: '0 10px 10px' }}>
          {sidebarClusters.length === 0 ? (
            <div style={{ fontSize: 11, color: 'var(--fg-mute)', padding: '4px 2px' }}>
              No face clusters yet. Import photos to populate.
            </div>
          ) : (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(6,1fr)', gap: 4 }}>
              {sidebarClusters.map((c) => {
                const hue = (c.id * 47) % 360;
                const label = c.name?.trim() || `#${c.id}`;
                return (
                  <div key={c.id} title={`${label} · ${c.faceCount}`} style={{ textAlign: 'center' }}>
                    <div
                      style={{
                        width: 28,
                        height: 28,
                        borderRadius: '50%',
                        background: `oklch(0.5 0.15 ${hue})`,
                        border: '1px solid var(--stroke)',
                      }}
                    />
                    <div
                      style={{
                        fontSize: 9,
                        color: 'var(--fg-mute)',
                        marginTop: 2,
                        fontFamily: 'var(--mono-font)',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {label}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <SourcesPanel />
      </div>

      <div className="sidepanel-footer">
        <button
          type="button"
          className="btn2 ghost"
          style={{ width: '100%', justifyContent: 'center', fontSize: 12, padding: 7 }}
        >
          <Icon name="plus" size={12} /> New Smart Album
        </button>
      </div>
    </div>
  );
}
