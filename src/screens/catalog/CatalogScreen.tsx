import { useVirtualizer } from '@tanstack/react-virtual';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { SEARCH_SUGGESTIONS } from '../../state/fixtures';
import { useAlbums, useOnThisDay, usePhotos, useUnseenPhotos } from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';

export interface CatalogScreenProps {
  albumId: string;
}

const FACETS = ['All', 'People', 'Places', 'Objects', 'Events', 'Colors', 'Cameras'];

const ROW_HEIGHT = 190;
const MIN_CELL_WIDTH = 170;

function useColumnCount(containerRef: React.RefObject<HTMLDivElement | null>) {
  const [cols, setCols] = useState(4);
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      const w = entry.contentRect.width;
      setCols(Math.max(1, Math.floor(w / MIN_CELL_WIDTH)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [containerRef]);
  return cols;
}

interface VirtualGridProps {
  photos: PhotoRow[];
  selected: Set<number>;
  onToggle: (id: number) => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
}

function VirtualGrid({ photos, selected, onToggle, scrollRef }: VirtualGridProps) {
  const gridRef = useRef<HTMLDivElement>(null);
  const cols = useColumnCount(gridRef);

  const rows = useMemo(() => {
    const result: PhotoRow[][] = [];
    for (let i = 0; i < photos.length; i += cols) {
      result.push(photos.slice(i, i + cols));
    }
    return result;
  }, [photos, cols]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 3,
  });

  return (
    <div ref={gridRef} style={{ position: 'relative', height: rowVirtualizer.getTotalSize() }}>
      {rowVirtualizer.getVirtualItems().map((vrow) => {
        const rowPhotos = rows[vrow.index] ?? [];
        return (
          <div
            key={vrow.key}
            data-index={vrow.index}
            ref={rowVirtualizer.measureElement}
            style={{
              position: 'absolute',
              top: vrow.start,
              left: 0,
              right: 0,
              display: 'flex',
              gap: 3,
              padding: '0 3px',
            }}
          >
            {rowPhotos.map((p) => {
              const hue = (p.id * 31) % 360;
              return (
                <button
                  type="button"
                  key={p.id}
                  className="cell"
                  style={{ flex: 1, minWidth: 0 }}
                  onClick={() => onToggle(p.id)}
                  aria-pressed={selected.has(p.id)}
                  aria-label={`Select ${p.filename}`}
                >
                  <Placeholder
                    photo={{ hue, filename: p.filename, id: String(p.id) }}
                    selected={selected.has(p.id)}
                    subtle
                  />
                </button>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

interface RediscoveryRowProps {
  title: string;
  photos: PhotoRow[];
  selected: Set<number>;
  onToggle: (id: number) => void;
}

function RediscoveryRow({ title, photos, selected, onToggle }: RediscoveryRowProps) {
  if (photos.length === 0) return null;
  return (
    <div style={{ marginBottom: 2 }}>
      <div
        className="mono"
        style={{ fontSize: 10, color: 'var(--fg-mute)', letterSpacing: '0.08em', padding: '10px 18px 4px' }}
      >
        {title}
      </div>
      <div
        style={{
          display: 'flex',
          gap: 3,
          padding: '0 18px 10px',
          overflowX: 'auto',
          scrollbarWidth: 'none',
        }}
      >
        {photos.map((p) => {
          const hue = (p.id * 31) % 360;
          return (
            <button
              type="button"
              key={p.id}
              className="cell"
              style={{ flex: '0 0 160px', height: 160 }}
              onClick={() => onToggle(p.id)}
              aria-pressed={selected.has(p.id)}
              aria-label={`Select ${p.filename}`}
            >
              <Placeholder
                photo={{ hue, filename: p.filename, id: String(p.id) }}
                selected={selected.has(p.id)}
                subtle
              />
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function CatalogScreen({ albumId }: CatalogScreenProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const searching = query.trim().length > 0;
  const scrollRef = useRef<HTMLDivElement>(null);

  const { data: albums = [] } = useAlbums();
  const { data: photos = [] } = usePhotos({ limit: 500 });
  const { data: onThisDayPhotos = [] } = useOnThisDay(20);
  const { data: unseenPhotosList = [] } = useUnseenPhotos(20);

  const album = useMemo(() => {
    if (albumId === 'all') return null;
    return albums.find((a) => String(a.id) === albumId) ?? null;
  }, [albums, albumId]);

  const displayAlbum = album ?? {
    name: 'All Photos',
    photo_count: photos.length,
    description: 'Everything, everywhere',
  };

  const toggle = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

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

      <div className="canvas-scroll" ref={scrollRef}>
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
                  {displayAlbum.name}
                  <em>.</em>
                </h1>
                <div className="mono" style={{ fontSize: 12, color: 'var(--fg-dim)', marginTop: 6 }}>
                  {displayAlbum.photo_count.toLocaleString()} photos ·{' '}
                  {displayAlbum.description ?? 'Last updated 2h ago'}
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

            <RediscoveryRow
              title="ON THIS DAY"
              photos={onThisDayPhotos}
              selected={selected}
              onToggle={toggle}
            />
            <RediscoveryRow
              title="UNSEEN · WORTH ANOTHER LOOK"
              photos={unseenPhotosList}
              selected={selected}
              onToggle={toggle}
            />
            <VirtualGrid photos={photos} selected={selected} onToggle={toggle} scrollRef={scrollRef} />
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
            <VirtualGrid
              photos={photos.slice(0, 24)}
              selected={selected}
              onToggle={toggle}
              scrollRef={scrollRef}
            />
          </div>
        )}
      </div>
    </div>
  );
}
