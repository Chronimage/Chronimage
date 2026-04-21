/**
 * Thumbnail — real photo preview sourced from the Rust `get_thumbnail` command
 * (JPEG bytes → Blob URL). Falls back to `<Placeholder>` while loading or when
 * the thumbnail is unavailable (missing local copy, unsupported format, etc.).
 */

import { useEffect } from 'react';
import { useThumbnailUrl } from '../state/queries';
import { Placeholder, type PlaceholderProps } from './Placeholder';

export interface ThumbnailProps extends PlaceholderProps {
  photoId: number;
  sizePx?: number;
  alt?: string;
}

export function Thumbnail({ photoId, sizePx = 320, alt, ...placeholderProps }: ThumbnailProps) {
  const { data: url } = useThumbnailUrl(photoId, sizePx);

  // Revoke the blob URL when the component unmounts or the URL changes so we
  // don't leak object URLs on a long Catalog scroll.
  useEffect(() => {
    if (!url) return undefined;
    return () => URL.revokeObjectURL(url);
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
