import { Icon, type IconName } from '../../primitives/Icon';
import { PEOPLE } from '../../state/fixtures';
import { useAlbums, useSources } from '../../state/queries';

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

export interface CatalogSidePanelProps {
  albumId: string;
  onAlbumChange: (id: string) => void;
}

function statusDot(status: string): string {
  if (status === 'synced') return 'var(--accent)';
  if (status === 'syncing') return 'var(--info)';
  if (status === 'ready') return 'var(--warn)';
  return 'var(--fg-mute)';
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

      <div style={{ padding: '0 10px 10px' }}>
        <div className="progress-card" style={{ padding: 8 }}>
          <div className="pc-head" style={{ fontSize: 11.5 }}>
            <span>Cataloging</span>
            <span className="mono" style={{ color: 'var(--accent)' }}>
              99.6%
            </span>
          </div>
          <div className="progress" style={{ height: 3 }}>
            <div style={{ width: '99.6%' }} />
          </div>
          <div className="pc-stats">
            <span>moondream2 · scenes</span>
            <span>~2m left</span>
          </div>
        </div>
      </div>

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
        <span className="ai-badge on">{PEOPLE.length}</span>
      </div>
      <div style={{ padding: '0 10px 10px' }}>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(6,1fr)', gap: 4 }}>
          {PEOPLE.map((p) => {
            const hue = (p.face * 31) % 360;
            return (
              <div key={p.name} title={`${p.name} · ${p.count}`} style={{ textAlign: 'center' }}>
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
                  }}
                >
                  {p.name}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="section-label">
        <span>Sources</span>
      </div>
      <div className="list">
        {sources.slice(0, 7).map((s) => (
          <button type="button" key={s.id} className="item">
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
      </div>

      <div style={{ marginTop: 'auto', padding: 10, borderTop: '1px solid var(--stroke)' }}>
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
