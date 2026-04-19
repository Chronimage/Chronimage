import { Icon } from '../../primitives/Icon';
import { ALBUMS, PEOPLE, PHOTOS, SOURCES } from '../../state/fixtures';

export interface CatalogSidePanelProps {
  albumId: string;
  onAlbumChange: (id: string) => void;
}

export function CatalogSidePanel({ albumId, onAlbumChange }: CatalogSidePanelProps) {
  const nonCullAlbums = ALBUMS.filter((a) => a.tag !== 'cull').slice(0, 9);

  return (
    <div className="sidepanel">
      <div className="head">
        <h3>Catalog</h3>
        <span className="count">851,002</span>
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
            <span>gemma4 · scenes</span>
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
          <span className="n">851K</span>
        </button>
        {nonCullAlbums.map((a) => (
          <button
            type="button"
            key={a.id}
            className={`item ${albumId === a.id ? 'active' : ''}`}
            onClick={() => onAlbumChange(a.id)}
          >
            <span className="ico">
              <Icon name={a.tag === 'faces' || a.tag === 'people' ? 'faces' : 'tag'} size={13} />
            </span>
            <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {a.name}
            </span>
            <span className="n">{a.count > 999 ? `${(a.count / 1000).toFixed(1)}K` : a.count}</span>
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
            const photo = PHOTOS[p.face];
            const hue = photo?.hue ?? 0;
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
        {SOURCES.slice(0, 7).map((s) => (
          <button type="button" key={s.id} className="item">
            <span className="ico">
              <Icon name={s.kind} size={13} />
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
                background:
                  s.status === 'synced'
                    ? 'var(--accent)'
                    : s.status === 'syncing'
                      ? 'var(--info)'
                      : s.status === 'ready'
                        ? 'var(--warn)'
                        : 'var(--fg-mute)',
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
