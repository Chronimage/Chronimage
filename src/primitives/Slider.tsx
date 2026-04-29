/**
 * Slider — Lightroom-style label / track / value triplet.
 *
 * Implementation note: backed by a native `<input type="range">` rather
 * than the Radix-based shadcn `Slider`. Reasons: (1) develop-screen
 * tests use `fireEvent.change` on the input, which native ranges
 * support but Radix's custom-element slider does not; (2) for develop
 * adjustment rows the native control already meets the accessibility
 * + keyboard requirements; (3) it lets us style track + thumb with the
 * new bolder/compact tokens from a single `.slider-row` rule in
 * `global.css`.
 *
 * For headless use (curves, mask range) import the shadcn `Slider`
 * directly from `@/components/ui/slider`.
 */

import { useId } from 'react';
import { cn } from '@/lib/utils';

export interface SliderProps {
  readonly label: string;
  readonly value: number;
  readonly onChange: (value: number) => void;
  readonly min?: number;
  readonly max?: number;
  readonly step?: number;
  readonly suffix?: string;
  readonly disabled?: boolean;
  readonly className?: string;
}

export function Slider({
  label,
  value,
  onChange,
  min = -100,
  max = 100,
  step = 1,
  suffix = '',
  disabled = false,
  className,
}: SliderProps) {
  const id = useId();
  const formatted = value > 0 ? `+${value}` : String(value);

  return (
    <div className={cn('slider-row', disabled && 'opacity-60', className)}>
      <label htmlFor={id} className="lbl truncate">
        {label}
      </label>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-label={label}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="val">
        {formatted}
        {suffix}
      </span>
    </div>
  );
}
