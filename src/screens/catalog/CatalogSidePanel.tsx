import { Icon } from '../../primitives/Icon';
import { useAlbums, useFaceClusters, useSources } from '../../state/queries';
import { ImportProgressCard } from './ImportProgressCard';
import { SourceDeleteProgressCard } from './SourceDeleteProgressCard';
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
        <SourceDeleteProgressCard />

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
              <span className="label">{a.name}</span>
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
        {sidebarClusters.length === 0 ? (
          <div className="side-empty">No face clusters yet. Import photos to populate.</div>
        ) : (
          <div className="people-grid">
            {sidebarClusters.map((c) => {
              const hue = (c.id * 47) % 360;
              const label = c.name?.trim() || `#${c.id}`;
              return (
                <div key={c.id} className="person" title={`${label} · ${c.faceCount}`}>
                  <div className="avatar" style={{ background: `oklch(0.5 0.15 ${hue})` }} />
                  <div className="name">{label}</div>
                </div>
              );
            })}
          </div>
        )}

        <SourcesPanel />
      </div>

      <div className="sidepanel-footer">
        <button
          type="button"
          className="btn2 ghost"
          style={{
            width: '100%',
            justifyContent: 'center',
            fontSize: 12,
            padding: 7,
            opacity: 0.5,
            cursor: 'not-allowed',
          }}
          disabled
          title="Coming in Phase 2 — user-defined smart albums"
        >
          <Icon name="plus" size={12} /> New Smart Album
        </button>
      </div>
    </div>
  );
}
