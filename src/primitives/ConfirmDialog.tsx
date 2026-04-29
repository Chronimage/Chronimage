/**
 * ConfirmDialog — destructive-action confirmation built on shadcn
 * `AlertDialog`. Preserves the legacy API: `options` renders a list of
 * checkbox rows, the selected ids are passed to `onConfirm`.
 *
 * Use directly via `<AlertDialog>` for non-destructive confirmations.
 */

import { useEffect, useRef, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

export interface ConfirmDialogOption {
  id: string;
  label: string;
  description?: string;
  defaultChecked?: boolean;
  disabled?: boolean;
}

export interface ConfirmDialogProps {
  readonly open: boolean;
  readonly title: string;
  readonly description?: React.ReactNode;
  readonly confirmLabel: string;
  readonly cancelLabel?: string;
  readonly confirmTone?: 'default' | 'danger';
  readonly options?: ConfirmDialogOption[];
  readonly busy?: boolean;
  readonly onCancel: () => void;
  readonly onConfirm: (selected: Set<string>) => void;
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  cancelLabel = 'Cancel',
  confirmTone = 'default',
  options = [],
  busy = false,
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  const [selected, setSelected] = useState<Set<string>>(() => {
    const init = new Set<string>();
    for (const o of options) if (o.defaultChecked) init.add(o.id);
    return init;
  });

  const prevOpen = useRef(open);
  useEffect(() => {
    if (open && !prevOpen.current) {
      const init = new Set<string>();
      for (const o of options) if (o.defaultChecked) init.add(o.id);
      setSelected(init);
    }
    prevOpen.current = open;
  }, [open, options]);

  function toggleOption(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && !busy && onCancel()}>
      <AlertDialogContent className="max-w-[520px]">
        <AlertDialogHeader>
          <AlertDialogTitle className="font-sans text-[var(--text-xl)] font-semibold tracking-tight text-[color:var(--fg)]">
            {title}
          </AlertDialogTitle>
          {description && (
            <AlertDialogDescription asChild>
              <div className="text-[var(--text-base)] leading-[var(--leading-normal)] text-[color:var(--fg-dim)]">
                {description}
              </div>
            </AlertDialogDescription>
          )}
        </AlertDialogHeader>

        {options.length > 0 && (
          <div className="flex flex-col gap-1.5 pt-1">
            {options.map((opt) => {
              const isSelected = selected.has(opt.id);
              return (
                <Label
                  key={opt.id}
                  htmlFor={`confirm-opt-${opt.id}`}
                  className={cn(
                    'flex cursor-pointer items-start gap-2 rounded-sm border border-[color:var(--stroke)] p-2 transition-colors',
                    'hover:border-[color:var(--stroke-strong)]',
                    opt.disabled && 'cursor-default opacity-60',
                    isSelected && 'border-[color:var(--accent)] bg-[color:var(--accent-soft)]',
                  )}
                >
                  <Checkbox
                    id={`confirm-opt-${opt.id}`}
                    checked={isSelected}
                    disabled={opt.disabled || busy}
                    onCheckedChange={() => toggleOption(opt.id)}
                    className="mt-0.5"
                  />
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[var(--text-base)] text-[color:var(--fg)]">{opt.label}</span>
                    {opt.description && (
                      <span className="text-[var(--text-xs)] text-[color:var(--fg-mute)]">
                        {opt.description}
                      </span>
                    )}
                  </div>
                </Label>
              );
            })}
          </div>
        )}

        <AlertDialogFooter>
          {/* Cancel close path is handled by `onOpenChange` above — no
              explicit onClick here, otherwise Radix's built-in close
              behaviour would double-fire onCancel. */}
          <AlertDialogCancel disabled={busy}>{cancelLabel}</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={() => onConfirm(selected)}
            className={cn(
              confirmTone === 'danger' &&
                'bg-[color:var(--danger)] text-white hover:bg-[color:var(--danger)]/90 focus-visible:ring-[color:var(--danger)]',
            )}
          >
            {busy ? 'Working…' : confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
