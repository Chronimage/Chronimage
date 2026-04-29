/**
 * Seg — segmented control. Single-select.
 *
 * Built on shadcn `ToggleGroup` for keyboard navigation + ARIA, but the
 * visual treatment is owned by the `.seg-group` / `.seg-item` rules in
 * global.css so we don't fight Tailwind's `tw-merge` resolution against
 * the `toggleVariants` cva.
 */

import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { cn } from '@/lib/utils';

export interface SegOption<T extends string> {
  value: T;
  label: string;
}

export interface SegProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  options: SegOption<T>[];
  className?: string;
}

export function Seg<T extends string>({ value, onChange, options, className }: SegProps<T>) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={(next) => {
        // Single-select: ignore deselection (legacy Seg always had a value).
        if (next) onChange(next as T);
      }}
      className={cn('seg-group', className)}
    >
      {options.map((o) => (
        <ToggleGroupItem key={o.value} value={o.value} className="seg-item" aria-label={o.label}>
          {o.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}
