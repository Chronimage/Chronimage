/**
 * 2D color wheel for one Color Grading zone (shadows / midtones / highlights
 * / global). The center represents zero-saturation; dragging the puck out
 * to the rim sets the hue (angle) and saturation (radius).
 *
 * The luminance axis is rendered as a separate slider beneath the wheel so
 * the user can darken or lift the zone independent of its tint, matching
 * Lightroom's wheel + shadows-bar layout.
 */

import { type PointerEvent, useCallback, useRef } from 'react';
import { Slider } from './Slider';

export interface ColorWheelValue {
  /** Hue 0..360 (degrees). */
  hue: number;
  /** Saturation 0..100. */
  saturation: number;
  /** Luminance -100..100. */
  luminance: number;
}

export interface ColorWheelProps {
  label: string;
  value: ColorWheelValue;
  onChange: (next: ColorWheelValue) => void;
  size?: number;
}

const TWO_PI = Math.PI * 2;

export function ColorWheel({ label, value, onChange, size = 120 }: ColorWheelProps) {
  const wheelRef = useRef<HTMLDivElement | null>(null);

  const setFromPointer = useCallback(
    (e: PointerEvent<HTMLDivElement>) => {
      const wheel = wheelRef.current;
      if (!wheel) return;
      const rect = wheel.getBoundingClientRect();
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      const dx = e.clientX - cx;
      const dy = e.clientY - cy;
      const r = Math.min(1, Math.hypot(dx, dy) / (rect.width / 2));
      const angle = Math.atan2(dy, dx);
      // Map atan2 (-PI..PI) → hue 0..360 with red at the right side.
      const hue = ((angle + TWO_PI) % TWO_PI) * (360 / TWO_PI);
      onChange({ ...value, hue, saturation: r * 100 });
    },
    [onChange, value],
  );

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setFromPointer(e);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (e.buttons === 0) return;
    setFromPointer(e);
  };

  const sat = value.saturation / 100;
  const radians = (value.hue * Math.PI) / 180;
  const px = Math.cos(radians) * sat * (size / 2);
  const py = Math.sin(radians) * sat * (size / 2);

  return (
    <div className="color-wheel">
      <div className="color-wheel-label mono">{label}</div>
      <div
        ref={wheelRef}
        className="color-wheel-disc"
        role="slider"
        tabIndex={0}
        aria-label={`${label} hue and saturation`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(value.saturation)}
        style={{ width: size, height: size }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
      >
        <div
          className="color-wheel-puck"
          style={{
            transform: `translate(calc(-50% + ${px}px), calc(-50% + ${py}px))`,
          }}
        />
      </div>
      <Slider
        label="Luminance"
        value={value.luminance}
        onChange={(v) => onChange({ ...value, luminance: v })}
      />
    </div>
  );
}
