import { useVirtualizer } from '@tanstack/react-virtual';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import {
  useAlbums,
  useFirstTimeOnNewCamera,
  useOnThisDay,
  usePhotoLocation,
  usePhotoQuality,
  usePhotos,
  useRecordPhotoView,
  useSearchPhotos,
  useSearchSuggestions,
  useTags,
  useUnflaggedFavorites,
  useUnseenPhotos,
} from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';
import { DuplicatesPanel } from './DuplicatesPanel';

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
  onEndReached?: () => void;
  hasMore?: boolean;
  isFetchingMore?: boolean;
}

function VirtualGrid({
  photos,
  selected,
  onToggle,
  onFocus,
  scrollRef,
  onEndReached,
  hasMore,
  isFetchingMore,
}: VirtualGridProps) {
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

  // Trigger next-page fetch when the last virtualized row is within 5 rows of the end.
  const virtualItems = rowVirtualizer.getVirtualItems();
  const lastVisibleIndex = virtualItems.length > 0 ? (virtualItems[virtualItems.length - 1]?.index ?? 0) : 0;
  useEffect(() => {
    if (!onEndReached || !hasMore || isFetchingMore) return;
    if (rows.length === 0) return;
    if (lastVisibleIndex >= rows.length - 5) {
      onEndReached();
    }
  }, [lastVisibleIndex, rows.length, onEndReached, hasMore, isFetchingMore]);

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
                  <Thumbnail
                    photoId={p.id}
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

// ── Detail inspector ──────────────────────────────────────────────────────────

const TAG_KIND_LABEL: Record<string, string> = {
  people: 'People',
  place: 'Places',
  object: 'Objects',
  event: 'Events',
  color: 'Colors',
  camera: 'Camera',
  auto_scene: 'Scene',
  user: 'User',
};

const TAG_KIND_ORDER = ['people', 'place', 'object', 'event', 'auto_scene', 'color', 'camera', 'user'];

interface DetailInspectorProps {
  photo: PhotoRow;
  metaParts: string;
  exifParts: string;
}

interface QualityBarProps {
  label: string;
  value: number | null | undefined;
  /** When the score is "higher is better" and below this threshold, tint red. */
  warnBelow?: number;
}

function QualityBar({ label, value, warnBelow }: QualityBarProps) {
  const pct = typeof value === 'number' ? Math.round(Math.min(1, Math.max(0, value)) * 100) : null;
  const bad = typeof value === 'number' && typeof warnBelow === 'number' && value < warnBelow;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
      <span style={{ fontSize: 11.5, color: 'var(--fg-dim)', width: 120 }}>{label}</span>
      <div
        style={{
          flex: 1,
          height: 'var(--space-1)',
          background: 'var(--bg-elev)',
          borderRadius: 'var(--radius-sm)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            width: `${pct ?? 0}%`,
            height: '100%',
            background: bad ? 'var(--warn, #c97)' : 'var(--accent)',
          }}
        />
      </div>
      <span
        className="mono"
        style={{ fontSize: 11, color: 'var(--fg-mute)', minWidth: 42, textAlign: 'right' }}
      >
        {pct === null ? '—' : `${pct}%`}
      </span>
    </div>
  );
}

function DetailInspector({ photo, metaParts, exifParts }: DetailInspectorProps) {
  const { data: tags = [], isLoading: tagsLoading } = useTags(photo.id);
  const { data: quality } = usePhotoQuality(photo.id);
  const { data: location } = usePhotoLocation(photo.id);

  // Group tags by kind, preserving confidence ordering inside each group.
  const tagsByKind = useMemo(() => {
    const groups = new Map<string, typeof tags>();
    for (const t of tags) {
      const bucket = groups.get(t.kind) ?? [];
      bucket.push(t);
      groups.set(t.kind, bucket);
    }
    return groups;
  }, [tags]);

  const tagKinds = TAG_KIND_ORDER.filter((k) => tagsByKind.has(k));

  return (
    <div
      style={{
        padding: '0 var(--space-6) var(--space-4)',
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--space-5)',
      }}
    >
      {/* AI TAGS */}
      <section>
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            marginBottom: 'var(--space-2)',
            letterSpacing: '0.08em',
          }}
        >
          AI TAGS
        </div>
        {tagsLoading && <div style={{ fontSize: 12, color: 'var(--fg-mute)' }}>Loading tags…</div>}
        {!tagsLoading && tags.length === 0 && (
          <div style={{ fontSize: 12, color: 'var(--fg-mute)' }}>
            No tags yet. Auto-tagging runs as part of import (SigLIP + face clustering).
          </div>
        )}
        {!tagsLoading && tags.length > 0 && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            {tagKinds.map((kind) => {
              const kindTags = tagsByKind.get(kind) ?? [];
              return (
                <div key={kind}>
                  <div
                    className="mono"
                    style={{
                      fontSize: 10,
                      color: 'var(--fg-dim)',
                      marginBottom: 'var(--space-1)',
                      letterSpacing: '0.05em',
                    }}
                  >
                    {TAG_KIND_LABEL[kind] ?? kind.toUpperCase()}
                  </div>
                  <div style={{ display: 'flex', gap: 'var(--space-2)', flexWrap: 'wrap' }}>
                    {kindTags.map((t) => {
                      const pct = Math.round(t.confidence * 100);
                      return (
                        <span
                          key={t.id}
                          title={`${t.kind} · ${pct}% confidence`}
                          style={{ display: 'inline-flex' }}
                        >
                          <Chip tone={t.confidence >= 0.85 ? 'info' : undefined}>
                            {t.label}
                            <span style={{ opacity: 0.55, marginLeft: 'var(--space-2)', fontSize: 10 }}>
                              {pct}%
                            </span>
                          </Chip>
                        </span>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* QUALITY */}
      <section>
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            marginBottom: 'var(--space-2)',
            letterSpacing: '0.08em',
          }}
        >
          QUALITY
        </div>
        {!quality && (
          <div style={{ fontSize: 12, color: 'var(--fg-mute)' }}>Quality scores unavailable yet.</div>
        )}
        {quality && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            <QualityBar label="Aesthetic" value={quality.aesthetic != null ? quality.aesthetic / 10 : null} />
            <QualityBar label="Sharpness" value={quality.sharpness} warnBelow={0.3} />
            {quality.face_count > 0 && (
              <>
                <QualityBar label="Face clarity" value={quality.best_face_quality} warnBelow={0.5} />
                <QualityBar label="Eyes open" value={quality.min_eyes_open} warnBelow={0.4} />
                <div
                  className="mono"
                  style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 'var(--space-1)' }}
                >
                  {quality.face_count} {quality.face_count === 1 ? 'face' : 'faces'} detected
                </div>
              </>
            )}
          </div>
        )}
      </section>

      {/* EXIF */}
      <section>
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            marginBottom: 'var(--space-2)',
            letterSpacing: '0.08em',
          }}
        >
          EXIF
        </div>
        <div className="mono" style={{ fontSize: 11, color: 'var(--fg-dim)', lineHeight: 1.6 }}>
          <div>File · {metaParts || '—'}</div>
          <div>Camera · {exifParts || '—'}</div>
          <div>Captured · {photo.captured_at ? new Date(photo.captured_at).toLocaleString() : 'Unknown'}</div>
        </div>
      </section>

      {/* LOCATION */}
      <LocationSection lat={location?.lat ?? null} lng={location?.lng ?? null} />
    </div>
  );
}

interface LocationSectionProps {
  lat: number | null;
  lng: number | null;
}

function LocationSection({ lat, lng }: LocationSectionProps) {
  const hasCoords = typeof lat === 'number' && typeof lng === 'number';
  const osmUrl = hasCoords
    ? `https://www.openstreetmap.org/?mlat=${lat}&mlon=${lng}#map=14/${lat}/${lng}`
    : null;

  // Deterministic 12×8 grid marker — avoids an external tile request while
  // still giving the user a sense of "where on a globe" at a glance.
  let markerX = 0;
  let markerY = 0;
  if (hasCoords && lat !== null && lng !== null) {
    markerX = ((lng + 180) / 360) * 100;
    markerY = ((90 - lat) / 180) * 100;
  }

  return (
    <section>
      <div
        className="mono"
        style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 8, letterSpacing: '0.08em' }}
      >
        LOCATION
      </div>
      {!hasCoords && (
        <div style={{ fontSize: 12, color: 'var(--fg-mute)' }}>No GPS data in this photo's EXIF.</div>
      )}
      {hasCoords && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
          <div
            role="img"
            aria-label={`Map marker at ${lat?.toFixed(4)}, ${lng?.toFixed(4)}`}
            style={{
              position: 'relative',
              height: 120,
              border: '1px solid var(--stroke)',
              borderRadius: 'var(--radius-md)',
              background:
                'radial-gradient(circle at 50% 50%, color-mix(in oklch, var(--accent) 8%, var(--bg-elev)) 0%, var(--bg-elev) 70%)',
              backgroundImage:
                'linear-gradient(to right, color-mix(in oklch, var(--fg) 4%, transparent) 1px, transparent 1px), linear-gradient(to bottom, color-mix(in oklch, var(--fg) 4%, transparent) 1px, transparent 1px)',
              backgroundSize: '8.33% 12.5%',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                position: 'absolute',
                left: `${markerX}%`,
                top: `${markerY}%`,
                transform: 'translate(-50%, -100%)',
                width: 10,
                height: 10,
                borderRadius: '50%',
                background: 'var(--accent)',
                boxShadow: '0 0 0 3px color-mix(in oklch, var(--accent) 30%, transparent)',
              }}
            />
          </div>
          <div
            className="mono"
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              fontSize: 11,
              color: 'var(--fg-dim)',
            }}
          >
            <span>
              {lat?.toFixed(5)}, {lng?.toFixed(5)}
            </span>
            {osmUrl && (
              <a
                href={osmUrl}
                target="_blank"
                rel="noreferrer noopener"
                style={{ color: 'var(--accent)', textDecoration: 'none' }}
              >
                Open in OpenStreetMap →
              </a>
            )}
          </div>
        </div>
      )}
    </section>
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
            <Thumbnail
              photoId={photo.id}
              sizePx={1280}
              photo={{ hue, filename: photo.filename, id: String(photo.id) }}
              subtle={false}
            />
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
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 6 }}>
            {photo.is_raw && <Chip variant="solid">RAW</Chip>}
            {photo.aesthetic_score != null && (
              <Chip tone="info">aesthetic {photo.aesthetic_score.toFixed(1)}</Chip>
            )}
            {photo.paired_photo_id != null && <Chip>paired</Chip>}
          </div>
        </div>
        <DetailInspector photo={photo} metaParts={metaParts} exifParts={exifParts} />
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
                <Thumbnail
                  photoId={ph.id}
                  sizePx={160}
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
              <Thumbnail
                photoId={p.id}
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
  const [showDuplicates, setShowDuplicates] = useState(false);
  const searching = query.trim().length > 0;
  const scrollRef = useRef<HTMLDivElement>(null);

  const { data: albums = [] } = useAlbums();
  const numericAlbumId = albumId !== 'all' ? Number(albumId) : null;
  const {
    data: photos = [],
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
  } = usePhotos({ albumId: numericAlbumId });
  const loadMorePhotos = useCallback(() => {
    void fetchNextPage();
  }, [fetchNextPage]);
  let gridFooter: string;
  if (isFetchingNextPage) gridFooter = 'Loading more…';
  else if (hasNextPage) gridFooter = `${photos.length.toLocaleString()} loaded · scroll for more`;
  else gridFooter = `${photos.length.toLocaleString()} · end of catalog`;
  const { data: onThisDayPhotos = [] } = useOnThisDay(20);
  const { data: unseenPhotosList = [] } = useUnseenPhotos(20);
  const { data: newCameraPhotos = [] } = useFirstTimeOnNewCamera(20);
  const { data: unflaggedFavPhotos = [] } = useUnflaggedFavorites(20, 8.0);
  const { data: searchResults, isFetching: searchFetching } = useSearchPhotos(query);
  const { data: suggestions = [] } = useSearchSuggestions();

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

  const recordView = useRecordPhotoView();
  const openDetail = useCallback(
    (index: number) => {
      const clamped = Math.max(0, Math.min(index, photos.length - 1));
      setFocusedIndex(clamped);
      const photo = photos[clamped];
      if (photo) {
        // Fire-and-forget: errors are non-fatal (no UI consequence).
        recordView.mutate(photo.id);
      }
    },
    [photos, recordView],
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
        <button
          type="button"
          className="btn"
          onClick={() => setShowDuplicates(true)}
          title="Review duplicate and near-duplicate photos"
        >
          <Icon name="layers" size={13} /> Duplicates
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
            <RediscoveryRow
              title="FIRST TIME ON A NEW CAMERA"
              photos={newCameraPhotos}
              selected={selected}
              onToggle={toggle}
            />
            <RediscoveryRow
              title="UNFLAGGED FAVORITES"
              photos={unflaggedFavPhotos}
              selected={selected}
              onToggle={toggle}
            />
            <VirtualGrid
              photos={photos}
              selected={selected}
              onToggle={toggle}
              onFocus={openDetail}
              scrollRef={scrollRef}
              onEndReached={loadMorePhotos}
              hasMore={hasNextPage}
              isFetchingMore={isFetchingNextPage}
            />
            <div
              style={{
                padding: '8px 18px 20px',
                color: 'var(--fg-mute)',
                fontSize: 11,
                fontFamily: 'var(--mono-font)',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <span>Click to select · double-click to open detail</span>
              <span>{gridFooter}</span>
            </div>
          </>
        ) : (
          <div>
            <div style={{ padding: '20px 20px 6px' }}>
              <div
                className="mono"
                style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginBottom: 4, letterSpacing: '0.08em' }}
              >
                {searchFetching
                  ? 'SEARCH · encoding…'
                  : searchResults && searchResults.length > 0
                    ? `SEARCH · SigLIP · ${searchResults.length} result${searchResults.length === 1 ? '' : 's'}`
                    : 'SEARCH · SigLIP · no results yet'}
              </div>
              <div className="display" style={{ fontSize: 32 }}>
                "{query}"<em>.</em>
              </div>
              {(!searchResults || searchResults.length === 0) && !searchFetching && (
                <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)', marginTop: 6 }}>
                  No embeddings yet — import photos and run the AI pipeline to enable search.
                </div>
              )}
            </div>
            <div style={{ padding: '6px 20px 0', display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {suggestions.slice(0, 5).map((s) => (
                <Chip key={s} onClick={() => setQuery(s)}>
                  {s}
                </Chip>
              ))}
            </div>
            {searchResults && searchResults.length > 0 ? (
              <div className="libgrid">
                {searchResults.map((p, i) => (
                  <button
                    type="button"
                    key={p.id}
                    className="cell"
                    style={{ position: 'relative' }}
                    onClick={() => toggle(i)}
                    aria-label={`Select ${p.filename}`}
                  >
                    <Thumbnail
                      photoId={p.id}
                      photo={{
                        id: String(p.id),
                        filename: p.filename,
                        hue: (p.id * 37) % 360,
                        scene: p.camera_make
                          ? `${p.camera_make} · ${p.camera_model ?? ''}`.trim()
                          : p.filename,
                      }}
                      idx={i}
                      selected={selected.has(i)}
                      subtle
                    />
                  </button>
                ))}
              </div>
            ) : (
              <VirtualGrid
                photos={photos}
                selected={selected}
                onToggle={toggle}
                onFocus={openDetail}
                scrollRef={scrollRef}
              />
            )}
          </div>
        )}
      </div>
      {showDuplicates && <DuplicatesPanel onClose={() => setShowDuplicates(false)} />}
    </div>
  );
}
