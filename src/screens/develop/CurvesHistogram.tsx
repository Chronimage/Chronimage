/**
 * Real per-channel histogram rendered behind the curves grid.
 *
 * The previous design overlaid a fake static SVG path on the photo
 * canvas — purely decorative, no pixel data. Lightroom's histogram lives
 * inside the Curves panel and is computed from the rendered preview.
 *
 * We sample the latest preview data URL via OffscreenCanvas (or a regular
 * canvas as fallback) at a coarse stride. 256 bins per channel, drawn as
 * additive translucent layers in the same colours as the channel toggle
 * pills above. Recomputes whenever the preview data URL identity changes.
 */

import { useEffect, useRef, useState } from 'react';
import { useDevelopUi } from '../../state/develop';
import { warn } from '../../util/log';

interface Bins {
  r: Uint32Array;
  g: Uint32Array;
  b: Uint32Array;
  l: Uint32Array;
  max: number;
}

const BINS = 256;
// Sampling stride keeps the histogram cheap on big previews — at stride 3
// we touch ~10% of pixels which is more than enough for a smooth shape.
const SAMPLE_STRIDE = 3;

function computeHistogram(data: ImageData): Bins {
  const r = new Uint32Array(BINS);
  const g = new Uint32Array(BINS);
  const b = new Uint32Array(BINS);
  const l = new Uint32Array(BINS);
  const px = data.data;
  for (let i = 0; i < px.length; i += 4 * SAMPLE_STRIDE) {
    const ri = px[i] ?? 0;
    const gi = px[i + 1] ?? 0;
    const bi = px[i + 2] ?? 0;
    r[ri] = (r[ri] ?? 0) + 1;
    g[gi] = (g[gi] ?? 0) + 1;
    b[bi] = (b[bi] ?? 0) + 1;
    // Rec.709 luma — same as Lightroom's L histogram.
    const li = Math.min(255, Math.max(0, Math.round(0.2126 * ri + 0.7152 * gi + 0.0722 * bi)));
    l[li] = (l[li] ?? 0) + 1;
  }
  // Skip endpoint clipping when computing peak so a hard-clipped highlight
  // doesn't squash the rest of the histogram into a flat line.
  let max = 0;
  for (let i = 1; i < BINS - 1; i++) {
    const rv = r[i] ?? 0;
    const gv = g[i] ?? 0;
    const bv = b[i] ?? 0;
    const lv = l[i] ?? 0;
    if (rv > max) max = rv;
    if (gv > max) max = gv;
    if (bv > max) max = bv;
    if (lv > max) max = lv;
  }
  return { r, g, b, l, max: Math.max(1, max) };
}

function buildPath(bins: Uint32Array, max: number): string {
  let d = `M 0 ${BINS}`;
  for (let i = 0; i < BINS; i++) {
    const h = ((bins[i] ?? 0) / max) * BINS;
    const y = BINS - h;
    d += ` L ${i} ${y.toFixed(2)}`;
  }
  d += ` L ${BINS - 1} ${BINS} Z`;
  return d;
}

export function CurvesHistogram() {
  const preview = useDevelopUi((s) => s.preview);
  const [bins, setBins] = useState<Bins | null>(null);
  const previousUrlRef = useRef<string | null>(null);

  useEffect(() => {
    if (!preview) {
      setBins(null);
      previousUrlRef.current = null;
      return;
    }
    if (preview === previousUrlRef.current) return;
    previousUrlRef.current = preview;

    let cancelled = false;
    const img = new Image();
    img.onload = () => {
      if (cancelled) return;
      try {
        const max = 256;
        const ratio = Math.min(1, max / Math.max(img.width, img.height));
        const w = Math.max(1, Math.round(img.width * ratio));
        const h = Math.max(1, Math.round(img.height * ratio));
        let imageData: ImageData;
        if (typeof OffscreenCanvas !== 'undefined') {
          const canvas = new OffscreenCanvas(w, h);
          const ctx = canvas.getContext('2d');
          if (!ctx) return;
          ctx.drawImage(img, 0, 0, w, h);
          imageData = ctx.getImageData(0, 0, w, h);
        } else {
          const canvas = document.createElement('canvas');
          canvas.width = w;
          canvas.height = h;
          const ctx = canvas.getContext('2d');
          if (!ctx) return;
          ctx.drawImage(img, 0, 0, w, h);
          imageData = ctx.getImageData(0, 0, w, h);
        }
        if (!cancelled) setBins(computeHistogram(imageData));
      } catch (err) {
        warn('Histogram compute failed', err);
      }
    };
    img.onerror = () => warn('Histogram image load failed');
    img.src = preview;

    return () => {
      cancelled = true;
    };
  }, [preview]);

  if (!bins) return null;

  const r = buildPath(bins.r, bins.max);
  const g = buildPath(bins.g, bins.max);
  const b = buildPath(bins.b, bins.max);
  const l = buildPath(bins.l, bins.max);

  return (
    <svg
      className="curves-histogram"
      viewBox={`0 0 ${BINS} ${BINS}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <path d={l} fill="rgba(255,255,255,0.18)" />
      <path d={r} fill="rgba(224,88,88,0.32)" />
      <path d={g} fill="rgba(90,205,124,0.32)" />
      <path d={b} fill="rgba(95,141,232,0.32)" />
    </svg>
  );
}
