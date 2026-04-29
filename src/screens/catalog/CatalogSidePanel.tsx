import { Badge } from '@/components/ui/badge';
import { Icon } from '../../primitives/Icon';
import { useAlbums, useSources } from '../../state/queries';
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

  const totalPhotos = sources.reduce((sum, s) => sum + s.photo_count, 0);
  const nonCullAlbums = albums.filter((a) => a.tag !== 'cull').slice(0, 9);

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
          <Badge variant="outline" className="ai-badge-shadcn">
            <Icon name="ai" size={10} />
            AI
          </Badge>
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

        <SourcesPanel />
      </div>
    </div>
  );
}
