/**
 * Interactive tone curves — monotone cubic Hermite (Fritsch-Carlson)
 * spline through 2..=16 draggable control points per channel (RGB / R /
 * G / B / L). State lifted to the parent so the curve is part of the
 * persisted Operations payload.
 *
 * Interaction:
 * - drag any handle to move it
 * - click empty space on the curve box to add a new control point
 * - right-click a handle (or Alt+click) to delete it
 * - endpoints (x=0, x=1) can be moved on y but never added or removed
 *
 * The SVG grid is the full `[0, 1]` domain in both axes, drawn with the
 * y-axis inverted (SVG y grows down, photography curves grow up). The
 * displayed spline is baked with the exact same interpolant the backend
 * uses, so WYSIWYG.
 */

import { useCallback, useMemo, useRef } from 'react';
import type { DevelopCurve, DevelopCurves } from '../../tauri/invoke';
import { identityCurve, MAX_CURVE_POINTS } from '../../tauri/invoke';
import { CurvesHistogram } from './CurvesHistogram';

export type CurveChannel = 'rgb' | 'r' | 'g' | 'b' | 'l';

const CHANNELS: { id: CurveChannel; label: string; stroke: string }[] = [
  { id: 'rgb', label: 'RGB', stroke: 'var(--accent)' },
  { id: 'r', label: 'R', stroke: '#e05858' },
  { id: 'g', label: 'G', stroke: '#5acd7c' },
  { id: 'b', label: 'B', stroke: '#5f8de8' },
  { id: 'l', label: 'L', stroke: '#d1d1d1' },
];

export interface CurvesPanelProps {
  /** Full curves state (master + per-channel). */
  value: DevelopCurves;
  onChange: (next: DevelopCurves) => void;
  /** Currently active channel. */
  channel: CurveChannel;
  setChannel: (c: CurveChannel) => void;
}

const VB = 100; // SVG viewBox size
// Minimum x gap between adjacent control points — keeps the spline
// stable and prevents divide-by-zero in the tangent solver.
const MIN_X_GAP = 0.01;
// When the user clicks the curve box with intent to ADD a point, the
// click must land further than this (in normalised distance) from any
// existing handle. Otherwise it's treated as a missed drag.
const ADD_POINT_MIN_GAP = 0.02;

export function CurvesPanel({ value, onChange, channel, setChannel }: CurvesPanelProps) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const dragIdxRef = useRef<number | null>(null);
  // Tracks whether a pointer-down on the svg background actually moved —
  // if it didn't, pointer-up is treated as a click and inserts a point.
  const pressStartRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  const active: DevelopCurve = value[channel];
  const stroke = CHANNELS.find((c) => c.id === channel)?.stroke ?? 'var(--accent)';

  const pathD = useMemo(() => buildCurvePath(active), [active]);
  const altChannels = useMemo(
    () =>
      CHANNELS.filter((c) => c.id !== channel).map((c) => ({
        ...c,
        d: buildCurvePath(value[c.id]),
      })),
    [value, channel],
  );

  const setChannelValue = useCallback(
    (next: DevelopCurve) => {
      onChange({ ...value, [channel]: next });
    },
    [channel, onChange, value],
  );

  const clientToNormalised = useCallback((clientX: number, clientY: number) => {
    const svg = svgRef.current;
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const nx = (clientX - rect.left) / rect.width;
    // SVG y goes top→down; photography curves go bottom→up. Invert.
    const ny = 1 - (clientY - rect.top) / rect.height;
    return { x: clamp01(nx), y: clamp01(ny) };
  }, []);

  const onHandlePointerDown = useCallback(
    (idx: number) => (e: React.PointerEvent<SVGCircleElement>) => {
      e.preventDefault();
      e.stopPropagation();
      dragIdxRef.current = idx;
      (e.target as Element).setPointerCapture?.(e.pointerId);
    },
    [],
  );

  const onHandleContextMenu = useCallback(
    (idx: number) => (e: React.MouseEvent<SVGCircleElement>) => {
      // Right-click = delete. Endpoints are not deletable (pipeline
      // needs a point at x=0 and x=1 to span the domain).
      e.preventDefault();
      e.stopPropagation();
      const n = active.length;
      if (idx === 0 || idx === n - 1) return;
      if (n <= 2) return;
      const next = active.slice();
      next.splice(idx, 1);
      setChannelValue(next);
    },
    [active, setChannelValue],
  );

  const onSvgPointerDown = useCallback(
    (e: React.PointerEvent<SVGElement>) => {
      // Background press — remember it so we can detect click-to-add
      // on pointerup if the pointer didn't move enough to count as a drag.
      const pt = clientToNormalised(e.clientX, e.clientY);
      if (!pt) return;
      pressStartRef.current = { x: pt.x, y: pt.y, moved: false };
    },
    [clientToNormalised],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<SVGElement>) => {
      const pt = clientToNormalised(e.clientX, e.clientY);
      if (!pt) return;

      const press = pressStartRef.current;
      if (press && !press.moved) {
        const dx = pt.x - press.x;
        const dy = pt.y - press.y;
        if (Math.hypot(dx, dy) > 0.01) {
          press.moved = true;
        }
      }

      const idx = dragIdxRef.current;
      if (idx == null) return;

      // Enforce x-monotonicity: each point stays strictly between its
      // neighbours. The two endpoints keep their x locked at 0 / 1 so
      // the curve always spans the full domain; only y is draggable.
      const n = active.length;
      const next = active.slice();
      const leftX = idx === 0 ? 0 : (active[idx - 1]?.[0] ?? 0) + MIN_X_GAP;
      const rightX = idx === n - 1 ? 1 : (active[idx + 1]?.[0] ?? 1) - MIN_X_GAP;
      const clampedX = idx === 0 ? 0 : idx === n - 1 ? 1 : clamp(pt.x, leftX, rightX);
      next[idx] = [clampedX, pt.y];
      setChannelValue(next);
    },
    [active, clientToNormalised, setChannelValue],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<SVGElement>) => {
      const wasDragging = dragIdxRef.current != null;
      dragIdxRef.current = null;

      const press = pressStartRef.current;
      pressStartRef.current = null;
      if (wasDragging) return; // drag ended, not a click
      if (!press || press.moved) return;
      if (active.length >= MAX_CURVE_POINTS) return;

      const pt = clientToNormalised(e.clientX, e.clientY);
      if (!pt) return;

      // Reject clicks too close to existing points — prevents accidental
      // duplicates when the user meant to grab a handle but missed.
      for (const p of active) {
        const px = p[0];
        const py = p[1];
        if (px === undefined || py === undefined) continue;
        if (Math.hypot(px - pt.x, py - pt.y) < ADD_POINT_MIN_GAP) return;
      }

      // Insert keeping x-ascending order. Lock away from the endpoints.
      const clampedX = clamp(pt.x, MIN_X_GAP, 1 - MIN_X_GAP);
      let insertAt = active.length - 1;
      for (let i = 1; i < active.length; i++) {
        const xi = active[i]?.[0];
        if (xi !== undefined && clampedX < xi) {
          insertAt = i;
          break;
        }
      }
      // Respect neighbour x-gap too.
      const leftNeighbourX = active[insertAt - 1]?.[0] ?? 0;
      const rightNeighbourX = active[insertAt]?.[0] ?? 1;
      if (clampedX - leftNeighbourX < MIN_X_GAP) return;
      if (rightNeighbourX - clampedX < MIN_X_GAP) return;

      const next = active.slice();
      next.splice(insertAt, 0, [clampedX, pt.y]);
      setChannelValue(next);
    },
    [active, clientToNormalised, setChannelValue],
  );

  const resetChannel = useCallback(() => {
    setChannelValue(identityCurve());
  }, [setChannelValue]);

  const isIdentity = useMemo(() => curveIsIdentity(active), [active]);

  return (
    <div>
      <div style={{ display: 'flex', gap: 4, marginBottom: 8, alignItems: 'center' }}>
        {CHANNELS.map((c) => {
          const on = channel === c.id;
          return (
            <button
              key={c.id}
              type="button"
              onClick={() => setChannel(c.id)}
              className="mono"
              style={{
                padding: '3px 8px',
                borderRadius: 5,
                fontSize: 11,
                background: on ? 'var(--bg-elev)' : 'transparent',
                color: on ? c.stroke : 'var(--fg-dim)',
                border: `1px solid ${on ? c.stroke : 'var(--stroke)'}`,
              }}
              aria-pressed={on}
              aria-label={`${c.label} channel`}
            >
              {c.label}
            </button>
          );
        })}
        <div style={{ flex: 1 }} />
        <button
          type="button"
          onClick={resetChannel}
          className="mono"
          disabled={isIdentity}
          style={{
            padding: '3px 8px',
            borderRadius: 5,
            fontSize: 10.5,
            background: 'transparent',
            color: isIdentity ? 'var(--fg-mute)' : 'var(--fg-dim)',
            border: '1px solid var(--stroke)',
            cursor: isIdentity ? 'default' : 'pointer',
          }}
          title="Reset this channel to the diagonal"
          aria-label="Reset curve"
        >
          reset
        </button>
      </div>
      <div className="curves-box">
        <CurvesHistogram />
        <svg
          ref={svgRef}
          viewBox={`0 0 ${VB} ${VB}`}
          preserveAspectRatio="none"
          onPointerDown={onSvgPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
          onPointerCancel={onPointerUp}
          style={{ touchAction: 'none', cursor: 'crosshair', position: 'relative' }}
          aria-label={`Tone curve — ${channel} channel. Click to add a point, right-click a point to remove it.`}
          role="application"
        >
          <defs>
            <pattern id="curves-grid" width="25" height="25" patternUnits="userSpaceOnUse">
              <path d="M 25 0 L 0 0 0 25" fill="none" stroke="var(--stroke)" strokeWidth="0.3" />
            </pattern>
          </defs>
          {/* Pattern strokes only — fill omitted so the histogram behind shows through. */}
          <rect width={VB} height={VB} fill="url(#curves-grid)" fillOpacity={0.6} />

          {/* Inactive channels, rendered faintly so the user can still see them. */}
          {altChannels.map((c) => (
            <path
              key={c.id}
              d={c.d}
              fill="none"
              stroke={c.stroke}
              strokeOpacity={0.2}
              strokeWidth={0.45}
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}

          {/* Reference diagonal (y = x). */}
          <line
            x1="0"
            y1={VB}
            x2={VB}
            y2="0"
            stroke="var(--stroke-strong)"
            strokeWidth="0.4"
            strokeDasharray="1 1"
          />

          {/* Active channel curve. */}
          <path
            d={pathD}
            fill="none"
            stroke={stroke}
            strokeWidth="0.9"
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />

          {/* Control-point handles — drag targets. */}
          {active.map((p, i) => {
            const cx = p[0] ?? 0;
            const cy = p[1] ?? 0;
            return (
              <circle
                // Position-based key so React doesn't confuse neighbouring
                // handles after an insert/delete. Two handles never share
                // an x (MIN_X_GAP is enforced), so this is stable.
                key={`cp-${cx.toFixed(4)}`}
                cx={cx * VB}
                cy={(1 - cy) * VB}
                r={2.2}
                fill={stroke}
                stroke="var(--bg)"
                strokeWidth={0.6}
                style={{ cursor: 'grab' }}
                onPointerDown={onHandlePointerDown(i)}
                onContextMenu={onHandleContextMenu(i)}
                aria-label={`Control point ${i + 1}`}
                role="slider"
                aria-valuemin={0}
                aria-valuemax={1}
                aria-valuenow={cy}
              />
            );
          })}
        </svg>
      </div>
      <div
        className="mono"
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          marginTop: 6,
          fontSize: 10,
          color: 'var(--fg-mute)',
        }}
      >
        <span>Blacks</span>
        <span>Shadows</span>
        <span>Mids</span>
        <span>Highlights</span>
        <span>Whites</span>
      </div>
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

function curveIsIdentity(c: DevelopCurve): boolean {
  const eps = 1e-4;
  if (c.length < 2) return false;
  for (const p of c) {
    const x = p[0];
    const y = p[1];
    if (x === undefined || y === undefined) return false;
    if (Math.abs(x - y) > eps) return false;
  }
  const first = c[0];
  const last = c[c.length - 1];
  if (!first || !last) return false;
  return (
    Math.abs(first[0]) < eps &&
    Math.abs(first[1]) < eps &&
    Math.abs(last[0] - 1) < eps &&
    Math.abs(last[1] - 1) < eps
  );
}

/** Build an SVG path string for a **monotone cubic Hermite** spline
 *  (Fritsch-Carlson) through the control points. This is the standard
 *  for photo tone curves — smooth + no overshoot, so lifting the mids
 *  can't cause tiny dips in the shadows. Sampled at 100 steps for
 *  butter-smooth rendering without visible faceting. */
function buildCurvePath(curve: DevelopCurve): string {
  if (curve.length < 2) return '';
  const xs = curve.map((p) => p[0] ?? 0);
  const ys = curve.map((p) => p[1] ?? 0);
  const tangents = monotoneTangents(xs, ys);

  const SAMPLES = 100;
  let d = '';
  for (let i = 0; i <= SAMPLES; i++) {
    const x = i / SAMPLES;
    // Find segment k where xs[k] <= x <= xs[k+1].
    let k = 0;
    for (let j = 0; j < xs.length - 1; j++) {
      const xj = xs[j];
      const xj1 = xs[j + 1];
      if (xj !== undefined && xj1 !== undefined && x >= xj && x <= xj1) {
        k = j;
        break;
      }
      if (j === xs.length - 2) k = j;
    }
    const y = hermite(xs, ys, tangents, k, x);
    const svgX = x * VB;
    const svgY = (1 - clamp01(y)) * VB;
    d += `${i === 0 ? 'M' : 'L'}${svgX.toFixed(2)},${svgY.toFixed(2)} `;
  }
  return d.trim();
}

/**
 * Fritsch-Carlson monotone cubic Hermite tangents. Given n points sorted
 * by x, returns n tangent slopes that produce a monotone interpolant —
 * between two control points the curve never overshoots beyond their y
 * range. That invariant is what makes this the idiomatic tone-curve
 * spline (same one Lightroom + Capture One use).
 */
function monotoneTangents(xs: number[], ys: number[]): number[] {
  const n = xs.length;
  const m = new Array<number>(n).fill(0);
  if (n < 2) return m;

  const d = new Array<number>(n - 1).fill(0);
  for (let i = 0; i < n - 1; i++) {
    const xi = xs[i];
    const xi1 = xs[i + 1];
    const yi = ys[i];
    const yi1 = ys[i + 1];
    if (xi === undefined || xi1 === undefined || yi === undefined || yi1 === undefined) continue;
    const dx = xi1 - xi;
    d[i] = dx > 1e-9 ? (yi1 - yi) / dx : 0;
  }

  m[0] = d[0] ?? 0;
  m[n - 1] = d[n - 2] ?? 0;
  for (let i = 1; i < n - 1; i++) {
    m[i] = ((d[i - 1] ?? 0) + (d[i] ?? 0)) / 2;
  }

  for (let i = 0; i < n - 1; i++) {
    const di = d[i] ?? 0;
    if (di === 0) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const alpha = (m[i] ?? 0) / di;
    const beta = (m[i + 1] ?? 0) / di;
    const mag = alpha * alpha + beta * beta;
    if (mag > 9) {
      const tau = 3 / Math.sqrt(mag);
      m[i] = tau * alpha * di;
      m[i + 1] = tau * beta * di;
    }
  }
  return m;
}

/** Evaluate the monotone Hermite spline inside segment `k` at query `x`. */
function hermite(xs: number[], ys: number[], m: number[], k: number, x: number): number {
  const x0 = xs[k];
  const x1 = xs[k + 1];
  const y0 = ys[k];
  const y1 = ys[k + 1];
  const m0 = m[k];
  const m1 = m[k + 1];
  if (
    x0 === undefined ||
    x1 === undefined ||
    y0 === undefined ||
    y1 === undefined ||
    m0 === undefined ||
    m1 === undefined
  ) {
    return 0;
  }
  const h = x1 - x0;
  if (h < 1e-9) return y0;
  const t = (x - x0) / h;
  const t2 = t * t;
  const t3 = t2 * t;
  const h00 = 2 * t3 - 3 * t2 + 1;
  const h10 = t3 - 2 * t2 + t;
  const h01 = -2 * t3 + 3 * t2;
  const h11 = t3 - t2;
  return h00 * y0 + h10 * h * m0 + h01 * y1 + h11 * h * m1;
}
