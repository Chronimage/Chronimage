/**
 * CollapsibleSection — Lightroom-style develop-panel section. Built on
 * shadcn `Collapsible`; preserves the legacy API (id-keyed open state,
 * optional eye toggle, action slot).
 */

import { ChevronDown, ChevronRight, Eye, EyeOff } from 'lucide-react';
import type { ReactNode } from 'react';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { cn } from '@/lib/utils';
import { useDevelopUi } from '../state/develop';

export interface CollapsibleSectionProps {
  readonly id: string;
  readonly title: string;
  readonly children: ReactNode;
  readonly action?: ReactNode;
  readonly visibility?: { visible: boolean; onToggle: () => void; label?: string };
  readonly defaultOpen?: boolean;
  readonly alwaysOpen?: boolean;
}

export function CollapsibleSection({
  id,
  title,
  children,
  action,
  visibility,
  defaultOpen = false,
  alwaysOpen = false,
}: CollapsibleSectionProps) {
  const open = useDevelopUi((s) => s.panelOpen[id] ?? defaultOpen);
  const setPanelOpen = useDevelopUi((s) => s.setPanelOpen);
  const isOpen = alwaysOpen || open;

  return (
    <Collapsible
      open={isOpen}
      onOpenChange={(next) => {
        if (!alwaysOpen) setPanelOpen(id, next);
      }}
      className="border-b border-[color:var(--stroke-soft)] last:border-b-0"
      data-open={isOpen}
    >
      <header className="flex items-center justify-between gap-2 px-3 py-2">
        <CollapsibleTrigger
          disabled={alwaysOpen}
          className={cn(
            'flex flex-1 items-center gap-2 font-mono text-[var(--text-2xs)] font-medium uppercase tracking-[0.1em] text-[color:var(--fg-dim)]',
            'transition-colors hover:text-[color:var(--fg)]',
            'disabled:cursor-default',
            'focus-visible:outline-none',
          )}
          aria-controls={`section-${id}`}
        >
          <span
            aria-hidden="true"
            className="inline-flex size-3 items-center justify-center text-[color:var(--fg-mute)]"
          >
            {isOpen ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
          </span>
          <span>{title}</span>
        </CollapsibleTrigger>
        <div className="flex items-center gap-1.5">
          {action}
          {visibility && (
            <button
              type="button"
              onClick={visibility.onToggle}
              aria-pressed={visibility.visible}
              aria-label={visibility.label ?? `Toggle ${title} visibility`}
              title={visibility.visible ? 'Hide effect' : 'Show effect'}
              className={cn(
                'inline-flex size-5 items-center justify-center rounded-xs text-[color:var(--fg-mute)]',
                'transition-colors hover:bg-[color:var(--bg-hover)] hover:text-[color:var(--fg)]',
                visibility.visible && 'text-[color:var(--accent)]',
              )}
              data-active={visibility.visible}
            >
              {visibility.visible ? <Eye className="size-3" /> : <EyeOff className="size-3" />}
            </button>
          )}
        </div>
      </header>
      <CollapsibleContent
        id={`section-${id}`}
        className="overflow-hidden data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down"
      >
        <div className="px-3 pb-3 pt-0.5">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}
