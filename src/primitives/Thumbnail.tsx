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

  return (
    <div className={placeholderProps.selected ? 'selected ph' : 'ph'}>
      <img
        src={url}
        alt={alt ?? filename ?? 'photo thumbnail'}
        loading="lazy"
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          objectFit: fit,
          display: 'block',
        }}
      />
      {placeholderProps.rejected && <div className="corner-rej">×</div>}
      {placeholderProps.keep && <div className="corner-keep">✓</div>}
    </div>
  );
}
