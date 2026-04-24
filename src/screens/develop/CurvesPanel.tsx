/**
 * Interactive tone curves — Catmull-Rom spline through 5 draggable
 * control points per channel (RGB / R / G / B / L). State lifted to the
 * parent so the curve is part of the persisted Operations payload.
 *
 * The SVG grid is the full `[0, 1]` domain in both axes, drawn with the
 * y-axis inverted (SVG y grows down, photography curves grow up). The
 * displayed spline is baked in the same way the backend does it
 * (Catmull-Rom with mirrored virtual endpoints) so what the user sees
 * is what the pipeline applies.
 */

import { useCallback, useMemo, useRef } from 'react';
import type { DevelopCurve, DevelopCurves } from '../../tauri/invoke';
import { identityCurve } from '../../tauri/invoke';

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

export function CurvesPanel({ value, onChange, channel, setChannel }: CurvesPanelProps) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const dragIdxRef = useRef<number | null>(null);

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

  const onPointerDown = useCallback(
    (idx: number) => (e: React.PointerEvent<SVGCircleElement>) => {
      e.preventDefault();
      e.stopPropagation();
      dragIdxRef.current = idx;
      (e.target as Element).setPointerCapture?.(e.pointerId);
    },
    [],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<SVGElement>) => {
      const idx = dragIdxRef.current;
      if (idx == null) return;
      const pt = clientToNormalised(e.clientX, e.clientY);
      if (!pt) return;

      // Enforce x-monotonicity: each point stays strictly between its
      // neighbours. Endpoints (idx=0, 4) lock their x at 0 / 1 so the
      // curve always spans the full domain.
      const next: DevelopCurve = deepCloneCurve(active);
      const atIdx = (i: number): readonly [number, number] => active[i] as readonly [number, number];
      const leftX = idx === 0 ? 0 : atIdx(idx - 1)[0] + 0.01;
      const rightX = idx === 4 ? 1 : atIdx(idx + 1)[0] - 0.01;
      const clampedX = idx === 0 ? 0 : idx === 4 ? 1 : clamp(pt.x, leftX, rightX);
      // Index assignment is always valid since idx ∈ [0, 4] (see onPointerDown).
      (next as unknown as [number, number][])[idx] = [clampedX, pt.y];
      setChannelValue(next);
    },
    [active, clientToNormalised, setChannelValue],
  );

  const onPointerUp = useCallback(() => {
    dragIdxRef.current = null;
  }, []);

  const resetChannel = useCallback(() => {
    setChannelValue(identityCurve() as DevelopCurve);
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
        <svg
          ref={svgRef}
          viewBox={`0 0 ${VB} ${VB}`}
          preserveAspectRatio="none"
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
          onPointerCancel={onPointerUp}
          style={{ touchAction: 'none', cursor: 'crosshair' }}
          aria-label={`Tone curve — ${channel} channel`}
          role="application"
        >
          <defs>
            <pattern id="curves-grid" width="25" height="25" patternUnits="userSpaceOnUse">
              <path d="M 25 0 L 0 0 0 25" fill="none" stroke="var(--stroke)" strokeWidth="0.3" />
            </pattern>
          </defs>
          <rect width={VB} height={VB} fill="url(#curves-grid)" />

          {/* Inactive channels, rendered faintly so the user can still see them. */}
          {altChannels.map((c) => (
            <path key={c.id} d={c.d} fill="none" stroke={c.stroke} strokeOpacity={0.25} strokeWidth={0.8} />
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
          <path d={pathD} fill="none" stroke={stroke} strokeWidth="1.5" />

          {/* Control-point handles — drag targets. */}
          {active.map((p, i) => (
            <circle
              // The 5 control points are a fixed sequence; index is
              // stable across renders, so `cp-${i}` is a legitimate key
              // here despite the generic index-key warning.
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length ordered tuple
              key={`cp-${i}`}
              cx={p[0] * VB}
              cy={(1 - p[1]) * VB}
              r={2.2}
              fill={stroke}
              stroke="var(--bg)"
              strokeWidth={0.6}
              style={{ cursor: 'grab' }}
              onPointerDown={onPointerDown(i)}
              aria-label={`Control point ${i + 1}`}
              role="slider"
              aria-valuemin={0}
              aria-valuemax={1}
              aria-valuenow={p[1]}
            />
          ))}
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

function deepCloneCurve(c: DevelopCurve): DevelopCurve {
  return [
    [c[0][0], c[0][1]],
    [c[1][0], c[1][1]],
    [c[2][0], c[2][1]],
    [c[3][0], c[3][1]],
    [c[4][0], c[4][1]],
  ];
}

function curveIsIdentity(c: DevelopCurve): boolean {
  const ident = identityCurve();
  const eps = 1e-4;
  for (let i = 0; i < 5; i++) {
    const a = c[i] as readonly [number, number];
    const b = ident[i] as readonly [number, number];
    if (Math.abs(a[0] - b[0]) > eps) return false;
    if (Math.abs(a[1] - b[1]) > eps) return false;
  }
  return true;
}

/** Build an SVG path string for the Catmull-Rom spline through the 5
 *  control points. Virtual endpoints mirror the boundary points around
 *  x = 0 and x = 1 to mimic the backend's endpoint behaviour. We sample
 *  ~48 pixels along the curve — enough to look smooth at the inspector
 *  size without visible faceting. */
function buildCurvePath(curve: DevelopCurve): string {
  // Readonly helpers so TS's tuple-element narrowing doesn't fight the
  // indexing we do inside the sampling loop.
  //
  // Endpoint mirror trick: the Catmull-Rom spline wants a "before" and
  // "after" point to shape tangents at the segment boundaries. We
  // reflect the second point through the first (and the second-to-last
  // through the last) so the tangent at the endpoint points straight
  // at its neighbour. Formula: p_virt = 2*p_end - p_neighbour.
  // Reflecting through the origin (old bug) left the virtual point on
  // top of the endpoint for an identity curve, producing a kink.
  const pt = (i: number): readonly [number, number] => {
    if (i === 0) {
      return [2 * curve[0][0] - curve[1][0], 2 * curve[0][1] - curve[1][1]];
    }
    if (i === 6) {
      return [2 * curve[4][0] - curve[3][0], 2 * curve[4][1] - curve[3][1]];
    }
    return curve[i - 1] as readonly [number, number];
  };

  const SAMPLES = 48;
  let d = '';
  for (let i = 0; i <= SAMPLES; i++) {
    const x = i / SAMPLES;
    // Find segment index j so that x sits between pt(j+1).x and pt(j+2).x.
    let j = 0;
    for (let k = 0; k < 5; k++) {
      const a = pt(k + 1)[0];
      const b = pt(k + 2)[0];
      if (x >= a && x <= b) {
        j = k;
        break;
      }
      if (x < a) {
        j = Math.max(0, k - 1);
        break;
      }
      if (k === 4) j = 4;
    }
    const pJ0 = pt(j);
    const pJ1 = pt(j + 1);
    const pJ2 = pt(j + 2);
    const pJ3 = pt(j + 3);
    const span = Math.max(1e-6, pJ2[0] - pJ1[0]);
    const t = clamp01((x - pJ1[0]) / span);
    const y = catmullRom(pJ0[1], pJ1[1], pJ2[1], pJ3[1], t);
    const svgX = x * VB;
    const svgY = (1 - clamp01(y)) * VB;
    d += `${i === 0 ? 'M' : 'L'}${svgX.toFixed(2)},${svgY.toFixed(2)} `;
  }
  return d.trim();
}

function catmullRom(p0: number, p1: number, p2: number, p3: number, t: number): number {
  const t2 = t * t;
  const t3 = t2 * t;
  return (
    0.5 * (2 * p1 + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 + (-p0 + 3 * p1 - 3 * p2 + p3) * t3)
  );
}
