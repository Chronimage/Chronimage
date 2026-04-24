/**
 * ShortcutOverlay — Phase 4 §6 discovery modal.
 *
 * Press `?` anywhere (outside a text input) to open. Shows every
 * registered keyboard shortcut grouped by context, overlaid on top of
 * the current screen. No rebinding UI yet — that's a follow-up.
 *
 * The shortcut list has two sources:
 *   - **Static** — hard-coded hotkeys wired directly in screens
 *     (Catalog detail view, Cull, Develop). These show as the
 *     baseline "built-in" bindings.
 *   - **Overrides** — rows in the `shortcuts` table, fetched via
 *     `shortcuts_list`. Overrides render alongside the static entry
 *     with a `custom` badge.
 */

import { useEffect, useMemo, useState } from 'react';
import { type ShortcutRow, shortcutsList } from '../tauri/invoke';
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

export function ShortcutOverlay({ open, onClose }: ShortcutOverlayProps) {
  const [overrides, setOverrides] = useState<ShortcutRow[]>([]);

  useEffect(() => {
    if (!open) return;
    shortcutsList()
      .then(setOverrides)
      .catch((e) => warn('shortcuts_list failed', e));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

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
          {grouped.map(([context, items]) => (
            <section key={context}>
              <div className="mono shortcut-section-label">{context.toUpperCase()}</div>
              <div className="shortcut-list">
                {items.map((item) => (
                  <div key={item.command_id} className="shortcut-row">
                    <div className="shortcut-label">{item.label}</div>
                    <div className="shortcut-keys">
                      {(item.override
                        ? item.override.key_binding.split('+').map((k) => k.trim())
                        : item.keys
                      ).map((key) => (
                        <kbd key={`${item.command_id}-${key}`}>{key}</kbd>
                      ))}
                      {item.override && (
                        <span
                          className="mono"
                          style={{ fontSize: 10, color: 'var(--accent)', marginLeft: 6 }}
                        >
                          custom
                        </span>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
        <div className="shortcut-modal-foot mono">
          Press <kbd>Esc</kbd> to close · rebinding UI lands with the Phase 4 Settings update
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
