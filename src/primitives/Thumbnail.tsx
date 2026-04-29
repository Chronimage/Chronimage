/**
 * Thumbnail — real photo preview sourced from the Rust `get_thumbnail` command
 * (JPEG bytes → Blob URL). Falls back to `<Placeholder>` while loading or when
 * the thumbnail is unavailable (missing local copy, unsupported format, etc.).
 */

import { useThumbnailUrl } from '../state/queries';
import { Placeholder, type PlaceholderProps } from './Placeholder';

export interface ThumbnailProps extends PlaceholderProps {
  photoId: number;
  sizePx?: number;
  alt?: string;
  fit?: 'cover' | 'contain';
}

const THUMBNAIL_SIZE_BUCKETS = [160, 240, 320, 480, 640, 960, 1280] as const;

export function thumbnailSizeForCssBox(
  widthPx: number,
  heightPx = widthPx,
  options: { maxPx?: number; minPx?: number; dpr?: number } = {},
) {
  const dpr =
    options.dpr ??
    (typeof window === 'undefined' ? 1 : Math.min(2, Math.max(1, window.devicePixelRatio || 1)));
  const target = Math.max(options.minPx ?? 160, Math.ceil(Math.max(widthPx, heightPx) * dpr));
  const maxPx = options.maxPx ?? 960;
  const buckets = THUMBNAIL_SIZE_BUCKETS.filter((bucket) => bucket <= maxPx);
  return buckets.find((bucket) => bucket >= target) ?? buckets.at(-1) ?? 320;
}

export function Thumbnail({
  photoId,
  sizePx = 320,
  alt,
  fit = 'cover',
  ...placeholderProps
}: ThumbnailProps) {
  const { data: url } = useThumbnailUrl(photoId, sizePx);

  if (!url) {
    return <Placeholder {...placeholderProps} />;
  }

  const filename = placeholderProps.photo?.filename;

  // `ph-loaded` carries the loaded-photo styling (solid bg, no padding,
  // no stripe gradient, force-filled img). Keeping `ph` on the wrapper
  // preserves the existing layout selectors that target the cell body
  // (e.g. `.cell-justified > .cell-open > .ph` for sizing). The loaded
  // styles win because `.ph-loaded` is a class-specificity override
  // declared after `.ph` in global.css.
  return (
    <div className={`ph ph-loaded${placeholderProps.selected ? ' selected' : ''}`} data-fit={fit}>
      <img src={url} alt={alt ?? filename ?? 'photo thumbnail'} loading="lazy" decoding="async" />
      {placeholderProps.rejected && <div className="corner-rej">×</div>}
      {placeholderProps.keep && <div className="corner-keep">✓</div>}
    </div>
  );
}
