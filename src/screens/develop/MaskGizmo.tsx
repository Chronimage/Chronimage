import type { CSSProperties, PointerEvent as ReactPointerEvent, RefObject } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';

import type { DevelopMask } from '../../tauri/invoke';

// Local clamp — DevelopScreen.tsx defines its own private copy. Kept
// in sync with that one (any sane min/max → bound `value`). Worth
// extracting to a shared util the next time three modules need it.
const clampNumber = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));

/**
 * Interactive on-canvas gizmos for editable mask kinds.
 *
 * Two flavours:
 * - `RadialMaskGizmo` — Lightroom-style two-circle radial gradient with
 *   four cardinal resize handles, a centre drag handle, and an inner
 *   feather handle that expands / contracts the soft falloff zone.
 * - `BrushMaskGizmo` — single-circle brush stamp with a centre drag and
 *   a cardinal handle for radius. Sits on top of the brush bitmap that
 *   the rasteriser actually paints from.
 *
 * **Pointer-event contract:** the wrapper container is `pointer-events:
 * none` so the wheel-zoom handler on the editor frame keeps receiving
 * scroll events. Only the handles and the centre dot get
 * `pointer-events: auto`. Without this, the gizmo would swallow wheel
 * events the moment a mask is selected and the user couldn't zoom in
 * to fine-tune handle placement.
 *
 * **Coordinate space:** payload values (`cx`, `cy`, `radius`, `feather`)
 * are normalised to `[0, 1]` and live alongside the existing rasteriser
 * contract in `develop::masks::rasterize_payload`. The gizmo translates
 * pointer events through the canvas bounding box back to that space, so
 * a mask drawn on a 4:3 photo looks identical when re-rendered on a
 * 16:9 zoom level.
 */

interface RadialPayload {
  kind: 'radial_gradient';
  cx: number;
  cy: number;
  radius: number;
  feather: number;
}

interface BrushPayload {
  kind: 'brush';
  cx: number;
  cy: number;
  radius: number;
  feather: number;
  density?: number;
}

const DEFAULT_RADIAL: Omit<RadialPayload, 'kind'> = {
  cx: 0.5,
  cy: 0.5,
  radius: 0.32,
  feather: 0.45,
};

const DEFAULT_BRUSH: Omit<BrushPayload, 'kind'> = {
  cx: 0.5,
  cy: 0.5,
  radius: 0.18,
  feather: 0.55,
  density: 1,
};

function parsePayload<T>(mask: DevelopMask): Partial<T> {
  try {
    return JSON.parse(mask.mask_payload) as Partial<T>;
  } catch {
    return {};
  }
}

interface DragState {
  kind: 'centre' | 'resize-n' | 'resize-e' | 'resize-s' | 'resize-w' | 'feather';
  pointerId: number;
}

interface GizmoCommonProps {
  mask: DevelopMask;
  containerRef: RefObject<HTMLDivElement | null>;
  onPayloadChange: (payload: Record<string, unknown>) => void;
}

/**
 * Lightroom-style two-circle radial gradient gizmo. Outer ring is the
 * full-falloff radius; the inner ring sits at `radius * (1 - feather)`
 * and shows where the mask is at full strength. Drag the inner ring to
 * change feather without resizing the outer extent.
 */
export function RadialMaskGizmo({ mask, containerRef, onPayloadChange }: GizmoCommonProps) {
  const initial = parsePayload<RadialPayload>(mask);
  const [cx, setCx] = useState(clampUnit(initial.cx, DEFAULT_RADIAL.cx));
  const [cy, setCy] = useState(clampUnit(initial.cy, DEFAULT_RADIAL.cy));
  const [radius, setRadius] = useState(clampNumber(initial.radius ?? DEFAULT_RADIAL.radius, 0.02, 0.95));
  const [feather, setFeather] = useState(clampNumber(initial.feather ?? DEFAULT_RADIAL.feather, 0, 0.95));
  // When the mask row updates from outside (e.g. after the backend
  // round-trips and the same payload comes back through TanStack Query),
  // sync the local interactive state. We compare on the persisted JSON
  // string so a re-render with identical content is a no-op.
  const lastSeenPayload = useRef(mask.mask_payload);
  useEffect(() => {
    if (lastSeenPayload.current === mask.mask_payload) return;
    lastSeenPayload.current = mask.mask_payload;
    const next = parsePayload<RadialPayload>(mask);
    setCx(clampUnit(next.cx, DEFAULT_RADIAL.cx));
    setCy(clampUnit(next.cy, DEFAULT_RADIAL.cy));
    setRadius(clampNumber(next.radius ?? DEFAULT_RADIAL.radius, 0.02, 0.95));
    setFeather(clampNumber(next.feather ?? DEFAULT_RADIAL.feather, 0, 0.95));
  }, [mask]);

  const drag = useRef<DragState | null>(null);
  const dragStart = useRef<{
    x: number;
    y: number;
    cx: number;
    cy: number;
    radius: number;
    feather: number;
  } | null>(null);

  const commit = useCallback(
    (next: { cx: number; cy: number; radius: number; feather: number }) => {
      onPayloadChange({
        kind: 'radial_gradient',
        cx: next.cx,
        cy: next.cy,
        radius: next.radius,
        feather: next.feather,
      });
    },
    [onPayloadChange],
  );

  const onPointerDown = (kind: DragState['kind']) => (event: ReactPointerEvent<SVGElement>) => {
    event.stopPropagation();
    event.preventDefault();
    const target = event.currentTarget;
    target.setPointerCapture?.(event.pointerId);
    drag.current = { kind, pointerId: event.pointerId };
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    dragStart.current = {
      x: (event.clientX - rect.left) / rect.width,
      y: (event.clientY - rect.top) / rect.height,
      cx,
      cy,
      radius,
      feather,
    };
  };

  const onPointerMove = (event: ReactPointerEvent<SVGElement>) => {
    if (!drag.current || !dragStart.current) return;
    if (event.pointerId !== drag.current.pointerId) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const px = (event.clientX - rect.left) / rect.width;
    const py = (event.clientY - rect.top) / rect.height;
    const start = dragStart.current;
    const dx = px - start.x;
    const dy = py - start.y;
    if (drag.current.kind === 'centre') {
      setCx(clampUnit(start.cx + dx, 0.5));
      setCy(clampUnit(start.cy + dy, 0.5));
    } else if (drag.current.kind === 'feather') {
      // Inner-ring drag: change feather while keeping outer radius
      // pinned, so the user gets independent control over the soft
      // edge without accidentally resizing the whole mask.
      const dist = Math.hypot(px - start.cx, py - start.cy);
      const innerFraction = clampNumber(dist / Math.max(start.radius, 1e-4), 0, 1);
      setFeather(clampNumber(1 - innerFraction, 0, 0.95));
    } else {
      // Cardinal handles: project the pointer offset along the axis
      // whose handle is being dragged. Always scale uniformly (radius
      // is a single scalar in the payload), matching Lightroom's
      // default Shift-modified behaviour. We pick the larger of the
      // two axis projections so a diagonal drag still resizes
      // smoothly.
      const sign =
        drag.current.kind === 'resize-e'
          ? 1
          : drag.current.kind === 'resize-w'
            ? -1
            : drag.current.kind === 'resize-s'
              ? 1
              : -1;
      const axisDelta = drag.current.kind === 'resize-e' || drag.current.kind === 'resize-w' ? dx : dy;
      const next = clampNumber(start.radius + sign * axisDelta, 0.02, 0.95);
      setRadius(next);
    }
  };

  const onPointerUp = (event: ReactPointerEvent<SVGElement>) => {
    if (!drag.current || event.pointerId !== drag.current.pointerId) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    drag.current = null;
    dragStart.current = null;
    commit({ cx, cy, radius, feather });
  };

  // Convert normalised radius to the SVG's percentage units. Single
  // value because the rasteriser uses one scalar (rasterizes as an
  // ellipse against the image's normalised coords); the gizmo follows
  // the same shape so what you see is what gets baked.
  const innerRadius = radius * (1 - feather);
  const cxPct = cx * 100;
  const cyPct = cy * 100;
  const rPct = radius * 100;
  const irPct = innerRadius * 100;

  return (
    <svg
      className="mask-gizmo radial"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      style={GIZMO_SVG_STYLE}
    >
      {/* Body hit-target — invisible ellipse covering the full
          gradient extent that captures pointer-down for centre drag.
          Without this the user can only move the mask by grabbing the
          tiny centre dot, which is far harder than dragging the body.
          Lightroom lets you drag anywhere inside the radial; this
          mirrors that behaviour. Sits above the gizmo-fill in paint
          order but below the rings + handles so the more-specific
          interactive elements still take priority. */}
      <ellipse
        cx={cxPct}
        cy={cyPct}
        rx={rPct}
        ry={rPct}
        fill="transparent"
        onPointerDown={onPointerDown('centre')}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ pointerEvents: 'fill', cursor: 'move' }}
      />
      {/* Pink fill inside the inner radius — the "fully selected"
          region. Lightroom shows pink at 35% opacity here. */}
      <ellipse cx={cxPct} cy={cyPct} rx={irPct} ry={irPct} className="mask-gizmo-fill" pointerEvents="none" />
      {/* Outer ring — the falloff edge. */}
      <ellipse cx={cxPct} cy={cyPct} rx={rPct} ry={rPct} className="mask-gizmo-ring" pointerEvents="none" />
      {/* Inner ring — the feather control surface. Pointer-events
          enabled so the user can drag it to change feather without
          touching the outer cardinal handles. */}
      <ellipse
        cx={cxPct}
        cy={cyPct}
        rx={irPct}
        ry={irPct}
        className="mask-gizmo-ring inner"
        onPointerDown={onPointerDown('feather')}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ pointerEvents: 'stroke', cursor: 'col-resize' }}
      />
      {/* Cardinal resize handles — N, E, S, W on the outer ring. */}
      {(['n', 'e', 's', 'w'] as const).map((dir) => {
        const hx = cxPct + (dir === 'e' ? rPct : dir === 'w' ? -rPct : 0);
        const hy = cyPct + (dir === 's' ? rPct : dir === 'n' ? -rPct : 0);
        return (
          <circle
            key={dir}
            cx={hx}
            cy={hy}
            r={1.4}
            className="mask-gizmo-handle"
            onPointerDown={onPointerDown(`resize-${dir}` as DragState['kind'])}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
            style={{
              pointerEvents: 'auto',
              cursor: dir === 'n' || dir === 's' ? 'ns-resize' : 'ew-resize',
            }}
          />
        );
      })}
      {/* Centre drag dot — keep this last so it paints over the rings. */}
      <circle
        cx={cxPct}
        cy={cyPct}
        r={1.6}
        className="mask-gizmo-handle centre"
        onPointerDown={onPointerDown('centre')}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ pointerEvents: 'auto', cursor: 'move' }}
      />
    </svg>
  );
}

/**
 * Single-circle brush stamp gizmo. Like the radial but without the
 * separate feather ring — feather is a percentage of the radius,
 * adjusted via the brush settings panel rather than on-canvas.
 */
export function BrushMaskGizmo({ mask, containerRef, onPayloadChange }: GizmoCommonProps) {
  const initial = parsePayload<BrushPayload>(mask);
  const [cx, setCx] = useState(clampUnit(initial.cx, DEFAULT_BRUSH.cx));
  const [cy, setCy] = useState(clampUnit(initial.cy, DEFAULT_BRUSH.cy));
  const [radius, setRadius] = useState(clampNumber(initial.radius ?? DEFAULT_BRUSH.radius, 0.02, 0.6));
  const featherInitial = clampNumber(initial.feather ?? DEFAULT_BRUSH.feather, 0, 0.95);
  const densityInitial = clampNumber(initial.density ?? DEFAULT_BRUSH.density ?? 1, 0, 1);

  const lastSeenPayload = useRef(mask.mask_payload);
  useEffect(() => {
    if (lastSeenPayload.current === mask.mask_payload) return;
    lastSeenPayload.current = mask.mask_payload;
    const next = parsePayload<BrushPayload>(mask);
    setCx(clampUnit(next.cx, DEFAULT_BRUSH.cx));
    setCy(clampUnit(next.cy, DEFAULT_BRUSH.cy));
    setRadius(clampNumber(next.radius ?? DEFAULT_BRUSH.radius, 0.02, 0.6));
  }, [mask]);

  const drag = useRef<DragState | null>(null);
  const dragStart = useRef<{ x: number; y: number; cx: number; cy: number; radius: number } | null>(null);

  const onPointerDown = (kind: DragState['kind']) => (event: ReactPointerEvent<SVGElement>) => {
    event.stopPropagation();
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { kind, pointerId: event.pointerId };
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    dragStart.current = {
      x: (event.clientX - rect.left) / rect.width,
      y: (event.clientY - rect.top) / rect.height,
      cx,
      cy,
      radius,
    };
  };

  const onPointerMove = (event: ReactPointerEvent<SVGElement>) => {
    if (!drag.current || !dragStart.current) return;
    if (event.pointerId !== drag.current.pointerId) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const px = (event.clientX - rect.left) / rect.width;
    const py = (event.clientY - rect.top) / rect.height;
    const start = dragStart.current;
    if (drag.current.kind === 'centre') {
      setCx(clampUnit(start.cx + (px - start.x), 0.5));
      setCy(clampUnit(start.cy + (py - start.y), 0.5));
    } else {
      const sign = drag.current.kind === 'resize-e' || drag.current.kind === 'resize-s' ? 1 : -1;
      const axisDelta =
        drag.current.kind === 'resize-e' || drag.current.kind === 'resize-w' ? px - start.x : py - start.y;
      setRadius(clampNumber(start.radius + sign * axisDelta, 0.02, 0.6));
    }
  };

  const onPointerUp = (event: ReactPointerEvent<SVGElement>) => {
    if (!drag.current || event.pointerId !== drag.current.pointerId) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    drag.current = null;
    dragStart.current = null;
    onPayloadChange({
      kind: 'brush',
      cx,
      cy,
      radius,
      feather: featherInitial,
      density: densityInitial,
    });
  };

  const cxPct = cx * 100;
  const cyPct = cy * 100;
  const rPct = radius * 100;

  return (
    <svg
      className="mask-gizmo brush"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      style={GIZMO_SVG_STYLE}
    >
      {/* Body hit-target — see RadialMaskGizmo for rationale. Lets
          users drag anywhere inside the brush stamp to reposition it
          without having to grab the centre dot. */}
      <ellipse
        cx={cxPct}
        cy={cyPct}
        rx={rPct}
        ry={rPct}
        fill="transparent"
        onPointerDown={onPointerDown('centre')}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ pointerEvents: 'fill', cursor: 'move' }}
      />
      <ellipse cx={cxPct} cy={cyPct} rx={rPct} ry={rPct} className="mask-gizmo-fill" pointerEvents="none" />
      <ellipse cx={cxPct} cy={cyPct} rx={rPct} ry={rPct} className="mask-gizmo-ring" pointerEvents="none" />
      {(['n', 'e', 's', 'w'] as const).map((dir) => {
        const hx = cxPct + (dir === 'e' ? rPct : dir === 'w' ? -rPct : 0);
        const hy = cyPct + (dir === 's' ? rPct : dir === 'n' ? -rPct : 0);
        return (
          <circle
            key={dir}
            cx={hx}
            cy={hy}
            r={1.4}
            className="mask-gizmo-handle"
            onPointerDown={onPointerDown(`resize-${dir}` as DragState['kind'])}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
            style={{
              pointerEvents: 'auto',
              cursor: dir === 'n' || dir === 's' ? 'ns-resize' : 'ew-resize',
            }}
          />
        );
      })}
      <circle
        cx={cxPct}
        cy={cyPct}
        r={1.6}
        className="mask-gizmo-handle centre"
        onPointerDown={onPointerDown('centre')}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ pointerEvents: 'auto', cursor: 'move' }}
      />
    </svg>
  );
}

const GIZMO_SVG_STYLE: CSSProperties = {
  position: 'absolute',
  inset: 0,
  width: '100%',
  height: '100%',
  // Parent SVG is non-interactive — only handles get pointer-events.
  // Wheel events bubble through to the editor frame so cursor-anchored
  // zoom keeps working while a mask is selected.
  pointerEvents: 'none',
  zIndex: 5,
  overflow: 'visible',
};

function clampUnit(value: number | undefined, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return clampNumber(value, 0, 1);
}
