/**
 * Placeholder — striped-SVG tile that stands in for a real photo thumbnail.
 * Phase 1 thumbnails will replace this with a real `<Thumbnail photoId />`
 * component driven by the Rust backend. Until then, we use the design's
 * deterministic placeholder so layouts render.
 */

export interface PlaceholderPhoto {
  id?: string;
  hue: number;
  filename?: string;
  scene?: string;
  ap?: number;
}

export interface PlaceholderProps {
  photo?: PlaceholderPhoto;
  idx?: number;
  selected?: boolean;
  rejected?: boolean;
  keep?: boolean;
  showLabel?: boolean;
  subtle?: boolean;
  className?: string;
}

export function Placeholder({
  photo,
  idx = 0,
  selected,
  rejected,
  keep,
  showLabel = true,
  subtle = false,
  className = '',
}: PlaceholderProps) {
  const hue = photo?.hue ?? (idx * 31) % 360;
  const p = photo ?? { hue, filename: `IMG_${idx}.ARW`, scene: '', ap: 1.8 };
  const tintA = `oklch(0.58 0.18 ${hue})`;
  const tintB = `oklch(0.22 0.08 ${hue})`;
  const seed = (((p.id ? p.id.charCodeAt(4) : 7) || 7) * 17) % 100;
  const classes = ['ph', selected && 'selected', rejected && 'rejected', className].filter(Boolean).join(' ');

  return (
    <div className={classes}>
      <div
        className="tint"
        style={{
          background: `linear-gradient(${135 + (seed % 60)}deg, ${tintA} 0%, ${tintB} 100%)`,
        }}
      />
      <div
        className="tint"
        style={{
          backgroundImage:
            'repeating-linear-gradient( -45deg, transparent 0 10px, rgba(255,255,255,0.04) 10px 11px )',
        }}
      />
      {rejected && <div className="corner-rej">×</div>}
      {keep && <div className="corner-keep">✓</div>}
      {showLabel && !subtle && (
        <div className="cap">
          <div style={{ fontWeight: 500, color: 'rgba(255,255,255,0.9)' }}>{p.filename}</div>
          <div style={{ opacity: 0.65 }}>{p.scene}</div>
        </div>
      )}
      {showLabel && subtle && (
        <div className="cap" style={{ opacity: 0.75 }}>
          <div>{p.filename}</div>
        </div>
      )}
    </div>
  );
}
