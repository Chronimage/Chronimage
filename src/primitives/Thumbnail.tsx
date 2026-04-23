/**
 * Thumbnail — real photo preview sourced from the Rust `get_thumbnail` command
 * (JPEG bytes → Blob URL). Falls back to `<Placeholder>` while loading or when
 * the thumbnail is unavailable (missing local copy, unsupported format, etc.).
 */

import { useEffect, useRef } from 'react';
import { useThumbnailUrl } from '../state/queries';
import { Placeholder, type PlaceholderProps } from './Placeholder';

export interface ThumbnailProps extends PlaceholderProps {
  photoId: number;
  sizePx?: number;
  alt?: string;
}

export function Thumbnail({ photoId, sizePx = 320, alt, ...placeholderProps }: ThumbnailProps) {
  const { data: url } = useThumbnailUrl(photoId, sizePx);

  // Revoke the *previous* blob URL only when the URL changes — not on unmount.
  // Revoking on unmount would invalidate the string still held in React Query's
  // cache; if the component re-mounts before gcTime expires the cached URL is
  // dead and the image silently fails.
  const prevUrlRef = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    const prev = prevUrlRef.current;
    prevUrlRef.current = url;
    if (prev && prev !== url) {
      URL.revokeObjectURL(prev);
    }
  }, [url]);

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
          objectFit: 'cover',
          display: 'block',
        }}
      />
      {placeholderProps.rejected && <div className="corner-rej">×</div>}
      {placeholderProps.keep && <div className="corner-keep">✓</div>}
    </div>
  );
}
