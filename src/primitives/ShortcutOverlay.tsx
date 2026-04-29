/**
 * ShortcutOverlay — Phase 4 §6 discovery modal + inline rebinder.
 *
 * Built on shadcn `Dialog`. Press `?` anywhere (outside a text input)
 * to open. Click any keys-cell to capture a new combo.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import { type ShortcutRow, shortcutsList, shortcutsSet } from '../tauri/invoke';
import { warn } from '../util/log';

interface StaticShortcut {
  command_id: string;
  label: string;
  keys: string[];
  context: string;
}

const STATIC_SHORTCUTS: StaticShortcut[] = [
  { command_id: 'app.help', label: 'Show this overlay', keys: ['?'], context: 'Global' },
  { command_id: 'app.search', label: 'Focus search', keys: ['/'], context: 'Global' },

  { command_id: 'detail.prev', label: 'Previous photo', keys: ['←'], context: 'Detail view' },
  { command_id: 'detail.next', label: 'Next photo', keys: ['→'], context: 'Detail view' },
  { command_id: 'detail.rate.clear', label: 'Clear rating', keys: ['0'], context: 'Detail view' },
  {
    command_id: 'detail.rate',
    label: 'Rate 1–5 stars',
    keys: ['1', '2', '3', '4', '5'],
    context: 'Detail view',
  },
  { command_id: 'detail.flag', label: 'Toggle flag', keys: ['X'], context: 'Detail view' },

  { command_id: 'cull.reject_a', label: 'Reject A', keys: ['A'], context: 'Cull' },
  { command_id: 'cull.reject_b', label: 'Reject B', keys: ['B'], context: 'Cull' },
  { command_id: 'cull.accept', label: 'Accept AI verdict', keys: ['↵'], context: 'Cull' },
  { command_id: 'cull.prev', label: 'Previous pair', keys: ['←'], context: 'Cull' },
  { command_id: 'cull.next', label: 'Next pair', keys: ['→'], context: 'Cull' },

  {
    command_id: 'catalog.select_all',
    label: 'Select all visible',
    keys: ['Ctrl', 'A'],
    context: 'Catalog grid',
  },
];

export interface ShortcutOverlayProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

function keyBindingFromEvent(e: KeyboardEvent): string | null {
  const key = e.key;
  if (key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta') {
    return null;
  }
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Meta');
  const normalised = key.length === 1 ? key.toUpperCase() : key;
  parts.push(normalised);
  return parts.join('+');
}

export function ShortcutOverlay({ open, onClose }: ShortcutOverlayProps) {
  const [overrides, setOverrides] = useState<ShortcutRow[]>([]);
  const [capturing, setCapturing] = useState<{ commandId: string; context: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    shortcutsList()
      .then(setOverrides)
      .catch((e) => warn('shortcuts_list failed', e));
  }, []);

  useEffect(() => {
    if (!open) return;
    refresh();
  }, [open, refresh]);

  useEffect(() => {
    if (!open || !capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') {
        setCapturing(null);
        return;
      }
      const binding = keyBindingFromEvent(e);
      if (!binding) return;
      shortcutsSet(capturing.commandId, binding, capturing.context)
        .then(() => {
          setCapturing(null);
          setError(null);
          refresh();
        })
        .catch((err) => setError(String(err)));
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [open, capturing, refresh]);

  const grouped = useMemo(() => {
    const overrideByCmd = new Map(overrides.map((o) => [o.command_id, o]));
    const groups = new Map<string, (StaticShortcut & { override?: ShortcutRow })[]>();
    for (const s of STATIC_SHORTCUTS) {
      const bucket = groups.get(s.context) ?? [];
      bucket.push({ ...s, override: overrideByCmd.get(s.command_id) });
      groups.set(s.context, bucket);
    }
    return [...groups.entries()];
  }, [overrides]);

  const resetToDefault = useCallback(
    async (commandId: string, context: string, defaultKeys: string[]) => {
      try {
        await shortcutsSet(commandId, defaultKeys.join('+'), context);
        setError(null);
        refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className={cn(
          'max-h-[80vh] max-w-3xl gap-0 overflow-hidden p-0',
          'border-[color:var(--stroke-strong)] bg-[color:var(--bg-chrome)]',
        )}
      >
        <DialogHeader className="border-b border-[color:var(--stroke)] px-6 py-4">
          <DialogTitle className="font-display text-[var(--text-display-sm)] font-normal leading-none tracking-[var(--tracking-display)] text-[color:var(--fg)]">
            Keyboard shortcuts<span className="italic text-[color:var(--accent)]">.</span>
          </DialogTitle>
          <DialogDescription className="font-mono text-[var(--text-2xs)] uppercase tracking-[0.1em] text-[color:var(--fg-mute)]">
            Click any combo to rebind · Esc closes
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[60vh] overflow-y-auto px-6 py-4">
          {error && (
            <div className="mb-3 rounded-sm border border-[color:var(--danger)] bg-[color:var(--danger)]/10 px-3 py-2 font-mono text-[var(--text-xs)] text-[color:var(--danger)]">
              {error}
            </div>
          )}
          <div className="flex flex-col gap-5">
            {grouped.map(([context, items]) => (
              <section key={context}>
                <div className="eyebrow mb-2">{context}</div>
                <div className="flex flex-col gap-0.5">
                  {items.map((item) => {
                    const isCapturing = capturing?.commandId === item.command_id;
                    const activeKeys = item.override
                      ? item.override.key_binding.split('+').map((k) => k.trim())
                      : item.keys;
                    return (
                      <div
                        key={item.command_id}
                        className="flex items-center justify-between gap-3 rounded-sm px-2 py-1.5 hover:bg-[color:var(--bg-hover)]"
                      >
                        <div className="text-[var(--text-base)] text-[color:var(--fg)]">{item.label}</div>
                        <div className="flex items-center gap-1.5">
                          {isCapturing ? (
                            <span className="font-mono text-[var(--text-xs)] uppercase tracking-[0.08em] text-[color:var(--accent)]">
                              Press new combo…
                            </span>
                          ) : (
                            <button
                              type="button"
                              onClick={() =>
                                setCapturing({
                                  commandId: item.command_id,
                                  context: item.context.toLowerCase(),
                                })
                              }
                              aria-label={`Rebind ${item.label}`}
                              title="Click to rebind"
                              className="inline-flex items-center gap-1 rounded-xs px-1 py-0.5 transition-colors hover:bg-[color:var(--bg-elev)]"
                            >
                              {activeKeys.map((key) => (
                                <kbd key={`${item.command_id}-${key}`}>{key}</kbd>
                              ))}
                            </button>
                          )}
                          {item.override && !isCapturing && (
                            <>
                              <span className="font-mono text-[var(--text-2xs)] uppercase tracking-[0.08em] text-[color:var(--accent)]">
                                custom
                              </span>
                              <button
                                type="button"
                                onClick={() => resetToDefault(item.command_id, item.context, item.keys)}
                                title="Reset to built-in default"
                                className="font-mono text-[var(--text-2xs)] uppercase tracking-[0.08em] text-[color:var(--fg-mute)] underline-offset-2 hover:text-[color:var(--fg)] hover:underline"
                              >
                                reset
                              </button>
                            </>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>
            ))}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Global key handler — mount once at app root to open the overlay on `?`. */
export function useShortcutOverlay(): [boolean, () => void, () => void] {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        (e.target as HTMLElement | null)?.isContentEditable
      ) {
        return;
      }
      if (e.key === '?' || (e.key === '/' && e.shiftKey)) {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);
  return [open, () => setOpen(true), () => setOpen(false)];
}

/**
 * Subscribe a handler to a command id, respecting any user override
 * stored in the `shortcuts` table.
 */
export function useShortcut(
  commandId: string,
  defaultBinding: string,
  handler: (e: KeyboardEvent) => void,
  opts?: { enabled?: boolean; context?: string },
) {
  const enabled = opts?.enabled ?? true;
  const [binding, setBinding] = useState<string>(defaultBinding);

  useEffect(() => {
    if (!enabled) return;
    shortcutsList()
      .then((rows) => {
        const override = rows.find((r) => r.command_id === commandId);
        if (override?.key_binding) setBinding(override.key_binding);
      })
      .catch((e) => warn('shortcuts_list failed', e));
  }, [commandId, enabled]);

  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        (e.target as HTMLElement | null)?.isContentEditable
      ) {
        return;
      }
      const pressed = keyBindingFromEvent(e);
      if (pressed && pressed === binding) {
        handler(e);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [binding, handler, enabled]);
}
