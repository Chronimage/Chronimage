/**
 * ShortcutOverlay — Phase 4 §6 discovery modal + inline rebinder.
 *
 * Press `?` anywhere (outside a text input) to open. Shows every
 * registered keyboard shortcut grouped by context, overlaid on top of
 * the current screen.
 *
 * Rebinding — click the keys column on any row to capture a new
 * combination; the next keyboard event is recorded and persisted via
 * `shortcuts_set`. The override appears with a `custom` badge and a
 * reset link that deletes it (reverts to the built-in default).
 *
 * The shortcut list has two sources:
 *   - **Static** — hard-coded hotkeys wired directly in screens
 *     (Catalog detail view, Cull, Develop). These show as the
 *     baseline "built-in" bindings.
 *   - **Overrides** — rows in the `shortcuts` table, fetched via
 *     `shortcuts_list`.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { type ShortcutRow, shortcutsList, shortcutsSet } from '../tauri/invoke';
import { warn } from '../util/log';
import { Icon } from './Icon';

interface StaticShortcut {
  command_id: string;
  label: string;
  keys: string[];
  context: string;
}

const STATIC_SHORTCUTS: StaticShortcut[] = [
  // Global
  { command_id: 'app.help', label: 'Show this overlay', keys: ['?'], context: 'Global' },
  { command_id: 'app.search', label: 'Focus search', keys: ['/'], context: 'Global' },

  // Catalog detail view
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

  // Cull screen
  { command_id: 'cull.reject_a', label: 'Reject A', keys: ['A'], context: 'Cull' },
  { command_id: 'cull.reject_b', label: 'Reject B', keys: ['B'], context: 'Cull' },
  { command_id: 'cull.accept', label: 'Accept AI verdict', keys: ['↵'], context: 'Cull' },
  { command_id: 'cull.prev', label: 'Previous pair', keys: ['←'], context: 'Cull' },
  { command_id: 'cull.next', label: 'Next pair', keys: ['→'], context: 'Cull' },

  // Multi-select
  {
    command_id: 'catalog.select_all',
    label: 'Select all visible',
    keys: ['Ctrl', 'A'],
    context: 'Catalog grid',
  },
];

export interface ShortcutOverlayProps {
  open: boolean;
  onClose: () => void;
}

/** Normalise a KeyboardEvent into a stable `Ctrl+Shift+A`-style string. */
function keyBindingFromEvent(e: KeyboardEvent): string | null {
  const key = e.key;
  // Ignore pure modifier presses — wait for the combo.
  if (key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta') {
    return null;
  }
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Meta');
  // Normalise single printable characters to uppercase so `Ctrl+a` and
  // `Ctrl+A` collapse into one binding.
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
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (capturing) {
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
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [open, onClose, capturing, refresh]);

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
        // There's no dedicated "delete override" command; write the default
        // binding back so overrides-table shows the same as the built-in.
        await shortcutsSet(commandId, defaultKeys.join('+'), context);
        setError(null);
        refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  if (!open) return null;

  return (
    <button type="button" className="shortcut-backdrop" onClick={onClose} aria-label="Close shortcut overlay">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        className="shortcut-modal"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div className="shortcut-modal-head">
          <h2>
            Keyboard shortcuts<em>.</em>
          </h2>
          <button type="button" className="btn" onClick={onClose} aria-label="Close">
            <Icon name="close" size={14} />
          </button>
        </div>
        <div className="shortcut-modal-body">
          {error && <div className="shortcut-error mono">{error}</div>}
          {grouped.map(([context, items]) => (
            <section key={context}>
              <div className="mono shortcut-section-label">{context.toUpperCase()}</div>
              <div className="shortcut-list">
                {items.map((item) => {
                  const isCapturing = capturing?.commandId === item.command_id;
                  const activeKeys = item.override
                    ? item.override.key_binding.split('+').map((k) => k.trim())
                    : item.keys;
                  return (
                    <div key={item.command_id} className="shortcut-row">
                      <div className="shortcut-label">{item.label}</div>
                      <div className="shortcut-keys">
                        {isCapturing ? (
                          <span className="shortcut-capturing mono">Press new combo…</span>
                        ) : (
                          <button
                            type="button"
                            className="shortcut-keys-btn"
                            onClick={() =>
                              setCapturing({
                                commandId: item.command_id,
                                context: item.context.toLowerCase(),
                              })
                            }
                            aria-label={`Rebind ${item.label}`}
                            title="Click to rebind"
                          >
                            {activeKeys.map((key) => (
                              <kbd key={`${item.command_id}-${key}`}>{key}</kbd>
                            ))}
                          </button>
                        )}
                        {item.override && !isCapturing && (
                          <>
                            <span
                              className="mono"
                              style={{ fontSize: 10, color: 'var(--accent)', marginLeft: 6 }}
                            >
                              custom
                            </span>
                            <button
                              type="button"
                              className="shortcut-reset mono"
                              onClick={() => resetToDefault(item.command_id, item.context, item.keys)}
                              title="Reset to built-in default"
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
        <div className="shortcut-modal-foot mono">
          Press <kbd>Esc</kbd> to close · click any key combo to rebind
        </div>
      </div>
    </button>
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
      // Match both `?` and shift+/ (depending on layout both can fire)
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
 * stored in the `shortcuts` table. Use at the top of screens so a
 * custom binding takes effect without a restart.
 *
 * NOTE: the list is fetched once on mount and not refreshed; the
 * overlay is the canonical place to rebind, and rebinds take effect on
 * the next screen remount. A global subscription system is tracked for
 * a future pass.
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
