/**
 * JustifiedGrid — Google Photos / Flickr-style justified row layout.
 *
 * Photos are packed into rows of a target height (~200 px). Each photo
 * keeps its natural aspect ratio, and every complete row is scaled
 * horizontally so its total width matches the container (minus gaps).
 * Result: uniform row heights, tight 4 px gaps, no empty gutters, and
 * every thumbnail visible at close to its natural proportions.
 *
 * Virtualised by scroll position — only rows whose vertical range
 * intersects the visible viewport (± a buffer) are rendered.
 *
 * Props mirror the old MasonryGrid so CatalogScreen changes are minimal.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { Thumbnail } from '../../primitives/Thumbnail';
import type { PhotoRow } from '../../tauri/invoke';

const TARGET_ROW_HEIGHT_DEFAULT = 200;
const MAX_ROW_HEIGHT_DEFAULT = 280;
const GAP_DEFAULT = 4;
const VIEWPORT_BUFFER_PX = 800; // render ±800 px above/below viewport

export interface JustifiedGridProps {
  photos: PhotoRow[];
  selected: Set<number>;
  onToggle: (id: number) => void;
  onFocus: (globalIndex: number) => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  onEndReached?: () => void;
  hasMore?: boolean;
  isFetchingMore?: boolean;
  /** Ideal row height before justify-fit scales each row to the container. */
  targetRowHeight?: number;
  /** Upper cap so a short trailing row doesn't blow up to a billboard. */
  maxRowHeight?: number;
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
 * Justified-row packing.
 *
 * Algorithm: greedily append photos to the current row at `targetRowHeight`
 * until the row's natural width (sum of aspect × targetH + gaps) exceeds
 * the container width — at which point we close the row and back-scale
 * every cell's width & height uniformly so the row fills the container.
 *
 * The trailing (incomplete) row is left at `targetRowHeight` rather than
 * stretched out — stretching would balloon one row to huge heights when
 * there's only a couple of photos left. Capped at `maxRowHeight`.
 */
function packJustified(
  photos: PhotoRow[],
  containerWidth: number,
  targetRowHeight: number,
  maxRowHeight: number,
  gap: number,
): { items: PackedItem[]; totalHeight: number } {
  if (containerWidth <= 0 || photos.length === 0) {
    return { items: [], totalHeight: 0 };
  }

  const items: PackedItem[] = [];
  let y = 0;

  let rowStart = 0;
  let rowAspectSum = 0;
  // Clamp bounds stop a single portrait sliver from becoming 30 px wide, and
  // a single pano from becoming a full-height banner.
  const MIN_ASPECT = 0.4;
  const MAX_ASPECT = 3.0;

  const flushRow = (endExclusive: number, scaleToFit: boolean) => {
    const rowLen = endExclusive - rowStart;
    if (rowLen <= 0) return;
    // Available width for cell content only (gaps sit between cells).
    const gapsWidth = gap * (rowLen - 1);
    const contentWidth = containerWidth - gapsWidth;

    // Raw height when the row is at target height. scale = contentWidth /
    // (rowAspectSum * targetRowHeight). If scaling, that's our row height;
    // otherwise use targetRowHeight.
    let rowHeight = targetRowHeight;
    if (scaleToFit && rowAspectSum > 0) {
      rowHeight = Math.min(maxRowHeight, contentWidth / rowAspectSum);
    }

    let x = 0;
    for (let i = rowStart; i < endExclusive; i++) {
      const p = photos[i];
      if (!p) continue;
      const aspect = Math.max(MIN_ASPECT, Math.min(MAX_ASPECT, displayAspectRatio(p)));
      const w = aspect * rowHeight;
      items.push({
        photo: p,
        globalIndex: i,
        x: Math.round(x),
        y: Math.round(y),
        w: Math.round(w),
        h: Math.round(rowHeight),
      });
      x += w + gap;
    }
    y += Math.round(rowHeight) + gap;
    rowStart = endExclusive;
    rowAspectSum = 0;
  };

  for (let i = 0; i < photos.length; i++) {
    const p = photos[i];
    if (!p) continue;
    const aspect = Math.max(MIN_ASPECT, Math.min(MAX_ASPECT, displayAspectRatio(p)));
    rowAspectSum += aspect;

    // Natural row width with this photo added.
    const rowLen = i - rowStart + 1;
    const gapsWidth = gap * (rowLen - 1);
    const naturalWidth = rowAspectSum * targetRowHeight + gapsWidth;

    if (naturalWidth >= containerWidth) {
      // Close the row here — scale to fit.
      flushRow(i + 1, true);
    }
  }

  // Trailing incomplete row — leave at target height (no stretch).
  if (rowStart < photos.length) {
    flushRow(photos.length, false);
  }

  const totalHeight = Math.max(0, y - gap);
  return { items, totalHeight };
}

export function JustifiedGrid({
  photos,
  selected,
  onToggle,
  onFocus,
  scrollRef,
  onEndReached,
  hasMore,
  isFetchingMore,
  targetRowHeight = TARGET_ROW_HEIGHT_DEFAULT,
  maxRowHeight = MAX_ROW_HEIGHT_DEFAULT,
  gap = GAP_DEFAULT,
}: JustifiedGridProps) {
  const gridRef = useRef<HTMLDivElement>(null);
  const containerWidth = useContainerWidth(gridRef);

  const { items, totalHeight } = useMemo(
    () => packJustified(photos, containerWidth, targetRowHeight, maxRowHeight, gap),
    [photos, containerWidth, targetRowHeight, maxRowHeight, gap],
  );

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

  const visibleItems = useMemo(
    () => items.filter((it) => it.y + it.h >= visibleRange.top && it.y <= visibleRange.bottom),
    [items, visibleRange],
  );

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
            className={`cell cell-justified${isSelected ? ' selected' : ''}`}
            style={{
              position: 'absolute',
              left: it.x,
              top: it.y,
              width: it.w,
              height: it.h,
            }}
          >
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
