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
  onFocus: (globalIndex: number) => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
}

function VirtualGrid({ photos, selected, onToggle, onFocus, scrollRef }: VirtualGridProps) {
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
            {rowPhotos.map((p, cellIdx) => {
              const globalIndex = vrow.index * cols + cellIdx;
              const hue = (p.id * 31) % 360;
              return (
                <button
                  type="button"
                  key={p.id}
                  className="cell"
                  style={{ flex: 1, minWidth: 0 }}
                  onClick={() => onToggle(p.id)}
                  onDoubleClick={() => onFocus(globalIndex)}
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

interface DetailViewProps {
  photo: PhotoRow;
  photoIndex: number;
  totalPhotos: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
  allPhotos: PhotoRow[];
  onJumpTo: (index: number) => void;
}

function DetailView({
  photo,
  photoIndex,
  totalPhotos,
  onClose,
  onPrev,
  onNext,
  allPhotos,
  onJumpTo,
}: DetailViewProps) {
  const hue = (photo.id * 31) % 360;
  const ext = photo.filename.split('.').pop()?.toUpperCase() ?? '';
  const dims = photo.width && photo.height ? `${photo.width}×${photo.height}` : '';
  const sizeKb = photo.size_bytes ? `${(photo.size_bytes / 1024).toFixed(0)} KB` : '';
  const metaParts = [dims, sizeKb, ext].filter(Boolean).join(' · ');
  const exifParts = [
    photo.camera_make && photo.camera_model ? `${photo.camera_make} ${photo.camera_model}` : null,
    photo.aperture ? `ƒ${photo.aperture.toFixed(1)}` : null,
    photo.shutter ?? null,
    photo.iso ? `ISO ${photo.iso}` : null,
    photo.focal_mm ? `${photo.focal_mm}mm` : null,
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className="canvas">
      <div className="toolbar">
        <button type="button" className="btn" onClick={onClose}>
          <Icon name="chevL" size={13} /> Back to grid
        </button>
        <div className="divider" />
        <button type="button" className="btn" onClick={onPrev} title="Previous (←)">
          <Icon name="chevL" size={13} />
        </button>
        <button type="button" className="btn" onClick={onNext} title="Next (→)">
          <Icon name="chevR" size={13} />
        </button>
        <div className="divider" />
        <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
          {photo.filename} · {photoIndex + 1}/{totalPhotos}
        </span>
        <div style={{ flex: 1 }} />
        <button type="button" className="btn">
          <Icon name="star" size={13} /> Rate
        </button>
        <button type="button" className="btn">
          <Icon name="flag" size={13} /> Flag
        </button>
        <div className="divider" />
        <button type="button" className="btn">
          <Icon name="brush" size={13} /> Develop
        </button>
        <button type="button" className="btn">
          <Icon name="export" size={13} /> Export
        </button>
      </div>
      <div className="detail-stage" style={{ overflowY: 'auto' }}>
        <div className="detail-hero">
          <div style={{ width: '100%', maxWidth: 1000, aspectRatio: '3/2', maxHeight: '100%' }}>
            <Placeholder photo={{ hue, filename: photo.filename, id: String(photo.id) }} subtle={false} />
          </div>
        </div>
        <div style={{ padding: '14px 24px 8px' }}>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 16, marginBottom: 6 }}>
            <div className="display" style={{ fontSize: 24 }}>
              {photo.filename}
              <em>.</em>
            </div>
            <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
              {photo.captured_at ? new Date(photo.captured_at).toLocaleDateString() : 'Unknown date'}
            </span>
          </div>
          <div className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)', marginBottom: 8 }}>
            {[metaParts, exifParts].filter(Boolean).join(' — ')}
          </div>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
            {photo.is_raw && <Chip variant="solid">RAW</Chip>}
            {photo.aesthetic_score != null && (
              <Chip tone="info">aesthetic {photo.aesthetic_score.toFixed(1)}</Chip>
            )}
            {photo.paired_photo_id != null && <Chip>paired</Chip>}
          </div>
        </div>
        {/* Filmstrip */}
        <div
          style={{
            display: 'flex',
            gap: 4,
            padding: '10px 24px 20px',
            overflowX: 'auto',
            scrollbarWidth: 'thin',
          }}
        >
          {allPhotos.map((ph, i) => {
            const phHue = (ph.id * 31) % 360;
            const isFocused = ph.id === photo.id;
            return (
              <button
                type="button"
                key={ph.id}
                onClick={() => onJumpTo(i)}
                style={{
                  flex: '0 0 auto',
                  width: 72,
                  aspectRatio: '3/2',
                  padding: 0,
                  border: 'none',
                  cursor: 'pointer',
                  outline: isFocused ? '2px solid var(--accent)' : '1px solid var(--stroke)',
                  outlineOffset: isFocused ? -2 : -1,
                  opacity: isFocused ? 1 : 0.65,
                  borderRadius: 2,
                  overflow: 'hidden',
                }}
                aria-label={ph.filename}
                aria-current={isFocused ? 'true' : undefined}
              >
                <Placeholder
                  photo={{ hue: phHue, filename: ph.filename, id: String(ph.id) }}
                  subtle={false}
                />
              </button>
            );
          })}
        </div>
      </div>
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
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
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

  const openDetail = useCallback(
    (index: number) => setFocusedIndex(Math.max(0, Math.min(index, photos.length - 1))),
    [photos.length],
  );
  const closeDetail = useCallback(() => setFocusedIndex(null), []);

  // Arrow-key navigation while detail is open
  useEffect(() => {
    if (focusedIndex === null) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'ArrowLeft') openDetail(focusedIndex - 1);
      else if (e.key === 'ArrowRight') openDetail(focusedIndex + 1);
      else if (e.key === 'Escape') closeDetail();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [focusedIndex, openDetail, closeDetail]);

  if (focusedIndex !== null && photos[focusedIndex]) {
    return (
      <DetailView
        photo={photos[focusedIndex]}
        photoIndex={focusedIndex}
        totalPhotos={photos.length}
        onClose={closeDetail}
        onPrev={() => openDetail(focusedIndex - 1)}
        onNext={() => openDetail(focusedIndex + 1)}
        allPhotos={photos}
        onJumpTo={openDetail}
      />
    );
  }

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
            <VirtualGrid
              photos={photos}
              selected={selected}
              onToggle={toggle}
              onFocus={openDetail}
              scrollRef={scrollRef}
            />
            <div
              style={{
                padding: '8px 18px 20px',
                color: 'var(--fg-mute)',
                fontSize: 11,
                fontFamily: 'var(--mono-font)',
              }}
            >
              Click to select · double-click to open detail
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
              onFocus={openDetail}
              scrollRef={scrollRef}
            />
          </div>
        )}
      </div>
    </div>
  );
}
