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

import type { KeyboardEvent } from 'react';
import { useEffect, useId, useRef, useState } from 'react';
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

const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));

/// Format the current value for display in the editable read-out: signed
/// integer with no leading zero, suffix appended. Used for both the
/// passive "show me the number" mode and the initial draft when the user
/// clicks into the input.
function formatValue(value: number, step: number, suffix: string): string {
  const decimals = step >= 1 ? 0 : Math.min(3, Math.ceil(-Math.log10(step)));
  const abs = Math.abs(value).toFixed(decimals);
  const sign = value > 0 ? '+' : value < 0 ? '-' : '';
  return `${sign}${abs}${suffix}`;
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
  const formatted = formatValue(value, step, suffix);

  // Editable read-out: click the value or tab to it → swap to text-input
  // mode, type a number, press Enter or blur to commit. Mirrors how
  // Lightroom's adjustment rows behave so users can dial in an exact
  // number without dragging the slider hair-thin distances.
  const [draft, setDraft] = useState(formatted);
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Re-sync the draft whenever the upstream value changes — including
  // after the user commits, which causes the parent to fire `onChange`
  // with a clamped value that may not match what they typed.
  useEffect(() => {
    if (!editing) setDraft(formatted);
  }, [editing, formatted]);

  const commit = () => {
    // Strip suffix + whitespace, then parse. Accepts `+12`, `-5.5`,
    // `25%` (suffix retained for symmetry with the display form), and
    // bare numbers. Anything unparseable reverts to the previous value.
    const cleaned = draft.replace(suffix, '').trim();
    const parsed = Number(cleaned);
    if (Number.isFinite(parsed)) {
      const next = clamp(parsed, min, max);
      // Snap to step grid. Without this, typing `25.7` on an integer
      // slider would round-trip to `25.7` then snap visually to `26`,
      // which feels like the input is lying about what it accepted.
      const snapped = step > 0 ? Math.round(next / step) * step : next;
      onChange(snapped);
    }
    setEditing(false);
  };

  const handleKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      event.currentTarget.blur();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      setDraft(formatted);
      setEditing(false);
      event.currentTarget.blur();
    } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      // Arrow keys nudge in step increments — the same behaviour the
      // range input has, kept consistent so keyboard users get the same
      // muscle memory whether they're focused on the slider or the
      // value field.
      event.preventDefault();
      const delta = (event.key === 'ArrowUp' ? 1 : -1) * (event.shiftKey ? step * 10 : step);
      const next = clamp(value + delta, min, max);
      onChange(next);
    }
  };

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
      <input
        ref={inputRef}
        type="text"
        inputMode="decimal"
        className="val"
        value={editing ? draft : formatted}
        disabled={disabled}
        aria-label={`${label} value`}
        onFocus={() => {
          setDraft(formatted);
          setEditing(true);
          // Select-all on focus so the user can immediately type-replace.
          requestAnimationFrame(() => inputRef.current?.select());
        }}
        onBlur={commit}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={handleKey}
      />
    </div>
  );
}
