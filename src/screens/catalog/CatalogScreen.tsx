import { useMemo, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { ALBUMS, PHOTOS, SEARCH_SUGGESTIONS } from '../../state/fixtures';

export interface CatalogScreenProps {
  albumId: string;
}

const FACETS = ['All', 'People', 'Places', 'Objects', 'Events', 'Colors', 'Cameras'];

export function CatalogScreen({ albumId }: CatalogScreenProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Set<number>>(new Set([2, 5, 12]));
  const searching = query.trim().length > 0;
  const photos = useMemo(() => PHOTOS.slice(0, 48), []);
  const album = ALBUMS.find((a) => a.id === albumId) ?? {
    id: 'all',
    name: 'All Photos',
    count: 851002,
    desc: 'Everything, everywhere',
    tag: '',
    tint: 0,
    covers: [],
  };

  const toggle = (i: number) => {
    const next = new Set(selected);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    setSelected(next);
  };

  return (
    <div className="canvas">
      <div className="toolbar">
        <div className="search" style={{ flex: 1, maxWidth: 'none' }}>
          <Icon name="search" size={13} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Ask your library — 'Ari laughing outside', 'sunsets on 35mm', 'Milo in snow'…"
            aria-label="Search photos"
          />
          <span className="kbd">⌘K</span>
        </div>
        <div className="divider" />
        <button type="button" className="btn" aria-label="Grid view">
          <Icon name="grid" size={13} />
        </button>
        <button type="button" className="btn" aria-label="Stack view">
          <Icon name="layers" size={13} />
        </button>
        <div className="divider" />
        {selected.size > 0 && (
          <>
            <span className="mono" style={{ fontSize: 11, color: 'var(--accent)' }}>
              {selected.size} selected
            </span>
            <button type="button" className="btn">
              <Icon name="brush" size={13} /> Develop
            </button>
            <button type="button" className="btn">
              <Icon name="cull" size={13} /> Cull
            </button>
            <button type="button" className="btn">
              <Icon name="export" size={13} /> Export
            </button>
            <button type="button" className="btn">
              <Icon name="tag" size={13} /> Tag
            </button>
          </>
        )}
      </div>

      <div className="canvas-scroll">
        {!searching ? (
          <>
            <div className="catalog-hero">
              <div>
                <div
                  className="mono"
                  style={{
                    fontSize: 10.5,
                    color: 'var(--fg-mute)',
                    marginBottom: 4,
                    letterSpacing: '0.08em',
                  }}
                >
                  {albumId === 'all' ? 'SMART ALBUM · ALL PHOTOS' : 'SMART ALBUM · AUTO-CURATED'}
                </div>
                <h1>
                  {album.name}
                  <em>.</em>
                </h1>
                <div className="mono" style={{ fontSize: 12, color: 'var(--fg-dim)', marginTop: 6 }}>
                  {album.count.toLocaleString()} photos · {album.desc ?? 'Last updated 2h ago'}
                </div>
              </div>
              <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                <Chip variant="solid">Ari</Chip>
                <Chip>Backyard</Chip>
                <Chip>Golden hour</Chip>
                <Chip tone="info">CLIP 0.82+</Chip>
              </div>
            </div>

            <div className="facetbar">
              {FACETS.map((f) => (
                <button type="button" key={f} className="btn" style={{ border: '1px solid var(--stroke)' }}>
                  {f}
                </button>
              ))}
            </div>

            <div className="libgrid">
              {photos.map((p, i) => (
                <button
                  type="button"
                  key={p.id}
                  className="cell"
                  onClick={() => toggle(i)}
                  aria-pressed={selected.has(i)}
                  aria-label={`Select ${p.filename}`}
                >
                  <Placeholder photo={p} idx={i} selected={selected.has(i)} subtle />
                </button>
              ))}
            </div>
            <div
              style={{
                padding: '8px 18px 20px',
                color: 'var(--fg-mute)',
                fontSize: 11,
                fontFamily: 'var(--mono-font)',
              }}
            >
              Click to select · Phase 1b: double-click opens detail overlay
            </div>
          </>
        ) : (
          <div>
            <div style={{ padding: '20px 20px 6px' }}>
              <div
                className="mono"
                style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 4, letterSpacing: '0.08em' }}
              >
                SEARCH · gemma4 + CLIP · (stub)
              </div>
              <div className="display" style={{ fontSize: 32 }}>
                "{query}"<em>.</em>
              </div>
              <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)', marginTop: 6 }}>
                (Phase 1: real results come from SigLIP text encoder + sqlite-vec k-NN)
              </div>
            </div>
            <div style={{ padding: '6px 20px 0', display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {SEARCH_SUGGESTIONS.slice(0, 5).map((s) => (
                <Chip key={s} onClick={() => setQuery(s)}>
                  {s}
                </Chip>
              ))}
            </div>
            <div className="libgrid">
              {photos.slice(0, 24).map((p, i) => (
                <button
                  type="button"
                  key={p.id}
                  className="cell"
                  style={{ position: 'relative' }}
                  onClick={() => toggle(i)}
                  aria-label={`Select ${p.filename}`}
                >
                  <Placeholder photo={p} idx={i} selected={selected.has(i)} subtle />
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
