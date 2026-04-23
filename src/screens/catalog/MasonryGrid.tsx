/**
 * MasonryGrid — Pinterest/Photos-style variable-height column-packed grid.
 *
 * Replaces the fixed-row VirtualGrid. Each photo's aspect ratio (computed
 * from `width`/`height` + `orientation`) determines its cell height; cells
 * pack into the shortest-column-first. Virtualised by scroll position —
 * only cells whose y-range intersects the visible viewport (± a buffer)
 * are rendered.
 *
 * Props mirror the old VirtualGrid so CatalogScreen only changes a tag.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { Thumbnail } from '../../primitives/Thumbnail';
import type { PhotoRow } from '../../tauri/invoke';

const MIN_COLUMN_WIDTH_DEFAULT = 220;
const GAP_DEFAULT = 8;
const VIEWPORT_BUFFER_PX = 800; // render ±800 px above/below viewport

export interface MasonryGridProps {
  photos: PhotoRow[];
  selected: Set<number>;
  onToggle: (id: number) => void;
  onFocus: (globalIndex: number) => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  onEndReached?: () => void;
  hasMore?: boolean;
  isFetchingMore?: boolean;
  /** Minimum cell width in px. Bigger = fewer, wider columns. */
  minColumnWidth?: number;
  /** Gap between cells in both axes. */
  gap?: number;
}

interface PackedItem {
  photo: PhotoRow;
  globalIndex: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** EXIF orientations 5–8 swap width and height at display time. */
function displayAspectRatio(p: PhotoRow): number {
  const w = p.width > 0 ? p.width : 1;
  const h = p.height > 0 ? p.height : 1;
  // Orientations 5, 6, 7, 8 rotate 90° → swap.
  const swap = p.orientation === 5 || p.orientation === 6 || p.orientation === 7 || p.orientation === 8;
  return swap ? h / w : w / h;
}

function useContainerWidth(ref: React.RefObject<HTMLDivElement | null>) {
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) setWidth(entry.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return width;
}

/**
 * Pack photos into columns using the shortest-column-first algorithm.
 *
 * Returns the placed items (with absolute x/y/w/h) and the total content
 * height (= tallest column). When `containerWidth` is 0 (initial mount
 * before ResizeObserver fires) we return zero-height placeholders so the
 * outer div is valid.
 */
function packMasonry(
  photos: PhotoRow[],
  containerWidth: number,
  minColumnWidth: number,
  gap: number,
): { items: PackedItem[]; totalHeight: number; columnCount: number } {
  if (containerWidth <= 0 || photos.length === 0) {
    return { items: [], totalHeight: 0, columnCount: 1 };
  }

  const columnCount = Math.max(1, Math.floor((containerWidth + gap) / (minColumnWidth + gap)));
  const columnWidth = (containerWidth - gap * (columnCount - 1)) / columnCount;

  const columnHeights = new Array<number>(columnCount).fill(0);
  const items: PackedItem[] = [];

  photos.forEach((p, idx) => {
    const aspect = displayAspectRatio(p);
    // Clamp aspect ratio so nothing degenerates into a 10 px sliver or a
    // 5 000 px monolith — picky phone panoramas still look right at 3:1.
    const clampedAspect = Math.max(0.4, Math.min(3.0, aspect));
    const cellHeight = Math.round(columnWidth / clampedAspect);

    // Place in the shortest column.
    let shortestCol = 0;
    for (let c = 1; c < columnCount; c++) {
      if (columnHeights[c]! < columnHeights[shortestCol]!) {
        shortestCol = c;
      }
    }
    const x = shortestCol * (columnWidth + gap);
    const y = columnHeights[shortestCol]!;
    items.push({
      photo: p,
      globalIndex: idx,
      x,
      y,
      w: columnWidth,
      h: cellHeight,
    });
    columnHeights[shortestCol] = y + cellHeight + gap;
  });

  const totalHeight = Math.max(...columnHeights) - gap;
  return { items, totalHeight, columnCount };
}

export function MasonryGrid({
  photos,
  selected,
  onToggle,
  onFocus,
  scrollRef,
  onEndReached,
  hasMore,
  isFetchingMore,
  minColumnWidth = MIN_COLUMN_WIDTH_DEFAULT,
  gap = GAP_DEFAULT,
}: MasonryGridProps) {
  const gridRef = useRef<HTMLDivElement>(null);
  const containerWidth = useContainerWidth(gridRef);

  const { items, totalHeight } = useMemo(
    () => packMasonry(photos, containerWidth, minColumnWidth, gap),
    [photos, containerWidth, minColumnWidth, gap],
  );

  // Scroll-driven visibility — render items whose vertical range intersects
  // [scrollTop - buffer, scrollTop + viewportHeight + buffer].
  const [visibleRange, setVisibleRange] = useState<{ top: number; bottom: number }>({
    top: 0,
    bottom: Number.POSITIVE_INFINITY,
  });

  useEffect(() => {
    const el = scrollRef.current;
    const grid = gridRef.current;
    if (!el || !grid) return;

    let rafHandle = 0;
    function compute() {
      rafHandle = 0;
      if (!el || !grid) return;
      const gridTop = grid.offsetTop;
      const scrollTop = el.scrollTop;
      const viewportH = el.clientHeight;
      setVisibleRange({
        top: Math.max(0, scrollTop - gridTop - VIEWPORT_BUFFER_PX),
        bottom: scrollTop - gridTop + viewportH + VIEWPORT_BUFFER_PX,
      });
    }
    function onScroll() {
      if (rafHandle === 0) rafHandle = requestAnimationFrame(compute);
    }

    compute();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      el.removeEventListener('scroll', onScroll);
      if (rafHandle !== 0) cancelAnimationFrame(rafHandle);
    };
  }, [scrollRef, totalHeight]);

  const visibleItems = useMemo(() => {
    return items.filter((it) => it.y + it.h >= visibleRange.top && it.y <= visibleRange.bottom);
  }, [items, visibleRange]);

  // Infinite scroll — fire when the last packed item's bottom is within a
  // viewport-height of the current scroll bottom.
  useEffect(() => {
    if (!onEndReached || !hasMore || isFetchingMore || items.length === 0) return;
    const last = items[items.length - 1];
    if (!last) return;
    if (last.y + last.h < visibleRange.bottom + VIEWPORT_BUFFER_PX) {
      onEndReached();
    }
  }, [items, visibleRange, onEndReached, hasMore, isFetchingMore]);

  return (
    <div ref={gridRef} style={{ position: 'relative', width: '100%', height: Math.max(totalHeight, 0) }}>
      {visibleItems.map((it) => {
        const p = it.photo;
        const hue = (p.id * 31) % 360;
        const isSelected = selected.has(p.id);
        return (
          <div
            key={p.id}
            className={`cell cell-masonry${isSelected ? ' selected' : ''}`}
            style={{
              position: 'absolute',
              left: it.x,
              top: it.y,
              width: it.w,
              height: it.h,
            }}
          >
            {/* Click target for opening detail view — sits under the indicator. */}
            <button
              type="button"
              className="cell-open"
              onClick={() => onFocus(it.globalIndex)}
              aria-label={`Open ${p.filename}`}
              style={{
                position: 'absolute',
                inset: 0,
                padding: 0,
                border: 'none',
                background: 'transparent',
                cursor: 'pointer',
              }}
            >
              <Thumbnail
                photoId={p.id}
                photo={{ hue, filename: p.filename, id: String(p.id) }}
                selected={isSelected}
                subtle
              />
            </button>
            {/* Google-Photos-style selection indicator. Hidden by default,
                visible on hover or when any photo is selected, filled when
                this cell is selected. Clicking toggles WITHOUT opening
                detail (stopPropagation). */}
            <button
              type="button"
              className={`sel-indicator${isSelected ? ' on' : ''}`}
              onClick={(e) => {
                e.stopPropagation();
                onToggle(p.id);
              }}
              aria-label={isSelected ? 'Deselect photo' : 'Select photo'}
              aria-pressed={isSelected}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
                <path
                  d="M5 12.5l4.5 4.5L20 7"
                  stroke="currentColor"
                  strokeWidth="2.8"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  fill="none"
                />
              </svg>
            </button>
          </div>
        );
      })}
    </div>
  );
}
