/**
 * Chip — thin wrapper over shadcn `Badge`. Preserves the legacy API
 * (`tone`, `variant`, `onClose`, `onClick`) so existing call sites keep
 * working while the underlying impl is now standard shadcn.
 *
 * - tone="info" → secondary badge with info dot
 * - tone="warn" → outline badge tinted warn
 * - tone="danger" → destructive badge
 * - variant="solid" → primary (filled) badge
 * - default → outline badge
 *
 * Use directly via `<Badge variant="…">` in new code.
 */

import { X } from 'lucide-react';
import type { CSSProperties, ReactNode } from 'react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export type ChipTone = 'info' | 'warn' | 'danger';
export type ChipVariant = 'solid';

export interface ChipProps {
  children: ReactNode;
  tone?: ChipTone;
  variant?: ChipVariant;
  onClose?: () => void;
  onClick?: () => void;
  style?: CSSProperties;
  className?: string;
}

const DOT_COLOR: Record<ChipTone, string> = {
  info: 'bg-[var(--info)]',
  warn: 'bg-[var(--warn)]',
  danger: 'bg-[var(--danger)]',
};

export function Chip({ children, tone, variant, onClose, onClick, style, className }: ChipProps) {
  const isSolid = variant === 'solid';
  const showDot = tone !== undefined && !isSolid;
  const badgeVariant: 'default' | 'secondary' | 'destructive' | 'outline' = isSolid
    ? 'default'
    : tone === 'danger'
      ? 'destructive'
      : tone === 'info'
        ? 'secondary'
        : 'outline';

  const content = (
    <>
      {showDot && tone && (
        <span className={cn('inline-block size-1.5 rounded-full', DOT_COLOR[tone])} aria-hidden="true" />
      )}
      <span className="truncate">{children}</span>
      {onClose && (
        <button
          type="button"
          className="-mr-0.5 inline-flex size-3.5 items-center justify-center rounded-sm text-[color:var(--fg-mute)] hover:text-[color:var(--fg)]"
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          aria-label="Remove"
        >
          <X className="size-3" />
        </button>
      )}
    </>
  );

  const cls = cn(
    'gap-1.5 rounded-sm border-[var(--stroke)] bg-[var(--bg-elev)] px-1.5 py-0.5 font-mono text-[var(--text-xs)] font-medium uppercase tracking-[0.04em] text-[color:var(--fg-dim)]',
    isSolid && 'border-transparent bg-[color:var(--accent)] text-[color:var(--accent-ink)]',
    tone === 'danger' && 'border-[color:var(--danger)] bg-transparent text-[color:var(--danger)]',
    tone === 'info' && 'border-[color:var(--stroke)] bg-[color:var(--bg-elev)] text-[color:var(--fg)]',
    onClick && 'cursor-pointer hover:bg-[color:var(--bg-hover)]',
    className,
  );

  if (onClick) {
    return (
      <button type="button" onClick={onClick} className={cn('inline-flex', cls)} style={style}>
        {content}
      </button>
    );
  }

  return (
    <Badge variant={badgeVariant} className={cls} style={style as React.CSSProperties}>
      {content}
    </Badge>
  );
}
