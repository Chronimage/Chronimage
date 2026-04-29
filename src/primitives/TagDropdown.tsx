/**
 * TagDropdown — manual tag-management menu for a multi-select. Built on
 * shadcn `Command` for the searchable list with the same outside-click
 * + Escape-to-close behaviour from the legacy version.
 *
 * Anchored inline by the caller (a `position: relative` parent renders
 * this absolutely-positioned card next to the trigger button).
 */

import { Plus, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import { useAddUserTag, useRemoveUserTag, useUserTags } from '../state/queries';

export interface TagDropdownProps {
  readonly photoIds: number[];
  readonly onClose: () => void;
}

export function TagDropdown({ photoIds, onClose }: TagDropdownProps) {
  const [input, setInput] = useState('');
  const ref = useRef<HTMLDivElement | null>(null);
  const { data: tags = [] } = useUserTags();
  const addTag = useAddUserTag();
  const removeTag = useRemoveUserTag();

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  const onAdd = useCallback(
    (label: string) => {
      const trimmed = label.trim();
      if (!trimmed || photoIds.length === 0) return;
      addTag.mutate({ photoIds, label: trimmed }, { onSuccess: () => setInput('') });
    },
    [addTag, photoIds],
  );

  const onRemove = useCallback(
    (label: string) => {
      if (!label || photoIds.length === 0) return;
      removeTag.mutate({ photoIds, label });
    },
    [photoIds, removeTag],
  );

  const trimmed = input.trim();
  const hasExactMatch = useMemo(
    () => tags.some((t) => t.label.toLowerCase() === trimmed.toLowerCase()),
    [tags, trimmed],
  );

  return (
    <div
      ref={ref}
      role="menu"
      className="absolute right-0 top-[calc(100%+6px)] z-[var(--z-overlay)] w-72 overflow-hidden rounded-md border border-[color:var(--stroke)] bg-[color:var(--bg-chrome)] shadow-[var(--shadow-lg)]"
    >
      <div className="flex items-center justify-between gap-2 border-b border-[color:var(--stroke)] px-3 py-2">
        <span className="font-mono text-[var(--text-2xs)] uppercase tracking-[0.1em] text-[color:var(--fg-mute)]">
          Tag {photoIds.length} photo{photoIds.length === 1 ? '' : 's'}
        </span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close tag menu"
          className="inline-flex size-5 items-center justify-center rounded-xs text-[color:var(--fg-mute)] hover:bg-[color:var(--bg-hover)] hover:text-[color:var(--fg)]"
        >
          <X className="size-3" />
        </button>
      </div>

      <Command shouldFilter className="[&_[cmdk-input-wrapper]]:border-b-[color:var(--stroke)]">
        <CommandInput
          placeholder="Add tag…"
          value={input}
          onValueChange={setInput}
          autoFocus
          onKeyDown={(e) => {
            if (e.key === 'Enter' && trimmed && !hasExactMatch) {
              e.preventDefault();
              onAdd(trimmed);
            }
          }}
          className="text-[var(--text-base)]"
        />
        <CommandList className="max-h-60">
          {trimmed && !hasExactMatch && (
            <CommandGroup>
              <CommandItem
                onSelect={() => onAdd(trimmed)}
                className="flex items-center gap-2 text-[var(--text-base)]"
              >
                <Plus className="size-3 text-[color:var(--accent)]" />
                <span>
                  Create <span className="text-[color:var(--accent)]">"{trimmed}"</span>
                </span>
              </CommandItem>
            </CommandGroup>
          )}
          <CommandEmpty>
            <span className="text-[var(--text-sm)] text-[color:var(--fg-mute)]">
              {trimmed ? `No tag named "${trimmed}".` : 'No tags yet — type to create one.'}
            </span>
          </CommandEmpty>
          {tags.length > 0 && (
            <CommandGroup heading="Library">
              {tags.slice(0, 24).map((t) => (
                <CommandItem
                  key={t.label}
                  value={t.label}
                  onSelect={() => onAdd(t.label)}
                  className="flex items-center gap-2 text-[var(--text-base)]"
                >
                  <Plus className="size-3 text-[color:var(--fg-mute)]" />
                  <span className="flex-1 truncate">{t.label}</span>
                  <span className="font-mono text-[var(--text-2xs)] tabular-nums text-[color:var(--fg-mute)]">
                    {t.photo_count}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>

      {tags.length > 0 && (
        <div className="border-t border-[color:var(--stroke)] px-3 py-2">
          <div className="eyebrow mb-1.5">Library tags · click to remove</div>
          <div className="flex flex-wrap gap-1">
            {tags.slice(0, 18).map((t) => (
              <Badge
                key={t.label}
                variant="outline"
                className="cursor-pointer gap-1 rounded-sm border-[color:var(--stroke)] bg-[color:var(--bg-elev)] px-1.5 py-0.5 font-sans text-[var(--text-xs)] font-normal text-[color:var(--fg-dim)] hover:border-[color:var(--danger)] hover:text-[color:var(--danger)]"
                onClick={() => onRemove(t.label)}
              >
                <span>{t.label}</span>
                <X className="size-2.5 opacity-60" aria-hidden="true" />
              </Badge>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
