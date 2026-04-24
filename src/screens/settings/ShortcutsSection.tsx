/**
 * Settings → Shortcuts — full rebinding table with conflict detection.
 *
 * Each row renders one known shortcut; clicking the keys column starts
 * a capture session that records the next keystroke combo and persists
 * it via `shortcuts_set`. Conflicts (same binding on two distinct
 * commands within the same context) render with a warning badge.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { type ShortcutRow, shortcutsList, shortcutsSet } from '../../tauri/invoke';
import { warn } from '../../util/log';

interface StaticShortcut {
  command_id: string;
  label: string;
  keys: string[];
  context: string;
}

/** Duplicated from ShortcutOverlay so the Settings page is self-contained. */
const STATIC_SHORTCUTS: StaticShortcut[] = [
  { command_id: 'app.help', label: 'Show shortcut overlay', keys: ['?'], context: 'Global' },
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

function keyBindingFromEvent(e: KeyboardEvent): string | null {
  const key = e.key;
  if (key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta') return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Meta');
  parts.push(key.length === 1 ? key.toUpperCase() : key);
  return parts.join('+');
}

interface EffectiveShortcut extends StaticShortcut {
  effective_binding: string;
  override?: ShortcutRow;
}

/** Rank by (binding, context). Any binding shared by >1 row in the same context = conflict. */
function detectConflicts(rows: EffectiveShortcut[]): Set<string> {
  const seen = new Map<string, string>(); // "context|binding" -> command_id
  const conflicting = new Set<string>();
  for (const r of rows) {
    // For multi-key shortcuts (Rate 1-5), check each binding independently.
    const bindings = r.override ? [r.effective_binding] : r.effective_binding.split(',').map((b) => b.trim());
    for (const b of bindings) {
      if (!b) continue;
      const key = `${r.context.toLowerCase()}|${b}`;
      const prev = seen.get(key);
      if (prev && prev !== r.command_id) {
        conflicting.add(r.command_id);
        conflicting.add(prev);
      } else {
        seen.set(key, r.command_id);
      }
    }
  }
  return conflicting;
}

export function ShortcutsSection() {
  const [overrides, setOverrides] = useState<ShortcutRow[]>([]);
  const [capturing, setCapturing] = useState<StaticShortcut | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    shortcutsList()
      .then(setOverrides)
      .catch((e) => warn('shortcuts_list failed', e));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') {
        setCapturing(null);
        return;
      }
      const binding = keyBindingFromEvent(e);
      if (!binding) return;
      shortcutsSet(capturing.command_id, binding, capturing.context.toLowerCase())
        .then(() => {
          setCapturing(null);
          setError(null);
          refresh();
        })
        .catch((err) => setError(String(err)));
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [capturing, refresh]);

  const effective: EffectiveShortcut[] = useMemo(() => {
    const overrideByCmd = new Map(overrides.map((o) => [o.command_id, o]));
    return STATIC_SHORTCUTS.map((s) => ({
      ...s,
      override: overrideByCmd.get(s.command_id),
      effective_binding: overrideByCmd.get(s.command_id)?.key_binding ?? s.keys.join(','),
    }));
  }, [overrides]);

  const conflicts = useMemo(() => detectConflicts(effective), [effective]);

  const resetToDefault = useCallback(
    async (s: StaticShortcut) => {
      try {
        // There is no "delete override" command; rewrite the default so the
        // stored binding == the built-in.
        await shortcutsSet(s.command_id, s.keys.join('+'), s.context.toLowerCase());
        setError(null);
        refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  return (
    <div className="set-section" style={{ marginTop: 40 }}>
      <h3
        style={{
          margin: '0 0 14px',
          fontSize: 13,
          color: 'var(--fg-dim)',
          fontFamily: 'var(--mono-font)',
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
        }}
      >
        Keyboard shortcuts
      </h3>
      <p style={{ margin: '0 0 12px', color: 'var(--fg-mute)', fontSize: 12 }}>
        Click any binding to rebind. <kbd>Esc</kbd> cancels.
      </p>
      {error && (
        <div
          className="mono"
          style={{
            margin: '0 0 12px',
            padding: '6px 10px',
            borderRadius: 4,
            color: 'var(--danger, #d66)',
            background: 'color-mix(in oklch, var(--danger, #d66) 10%, transparent)',
            fontSize: 11,
          }}
        >
          {error}
        </div>
      )}
      <table className="shortcuts-table">
        <thead>
          <tr>
            <th style={{ textAlign: 'left', padding: '6px 8px', width: 180 }}>Context</th>
            <th style={{ textAlign: 'left', padding: '6px 8px' }}>Action</th>
            <th style={{ textAlign: 'left', padding: '6px 8px', width: 260 }}>Binding</th>
            <th style={{ textAlign: 'left', padding: '6px 8px', width: 120 }}>State</th>
          </tr>
        </thead>
        <tbody>
          {effective.map((row) => {
            const isConflict = conflicts.has(row.command_id);
            const isCapturing = capturing?.command_id === row.command_id;
            const isOverridden = !!row.override;
            return (
              <tr key={row.command_id}>
                <td
                  style={{
                    padding: '6px 8px',
                    color: 'var(--fg-mute)',
                    fontFamily: 'var(--mono-font)',
                    fontSize: 11,
                  }}
                >
                  {row.context}
                </td>
                <td style={{ padding: '6px 8px', fontSize: 13 }}>{row.label}</td>
                <td style={{ padding: '6px 8px' }}>
                  {isCapturing ? (
                    <span className="mono shortcut-capturing">Press new combo…</span>
                  ) : (
                    <button
                      type="button"
                      className="shortcut-keys-btn"
                      onClick={() => setCapturing(row)}
                      aria-label={`Rebind ${row.label}`}
                      title="Click to rebind"
                    >
                      {row.effective_binding
                        .split(isOverridden ? '+' : ',')
                        .map((k) => k.trim())
                        .map((k) => (
                          <kbd key={`${row.command_id}-${k}`}>{k}</kbd>
                        ))}
                    </button>
                  )}
                </td>
                <td style={{ padding: '6px 8px', fontSize: 11 }}>
                  {isConflict && (
                    <span
                      className="mono"
                      style={{
                        color: 'var(--danger, #d66)',
                        padding: '2px 6px',
                        borderRadius: 4,
                        background: 'color-mix(in oklch, var(--danger, #d66) 12%, transparent)',
                      }}
                    >
                      conflict
                    </span>
                  )}
                  {isOverridden && !isConflict && (
                    <>
                      <span
                        className="mono"
                        style={{
                          color: 'var(--accent)',
                          marginRight: 8,
                        }}
                      >
                        custom
                      </span>
                      <button
                        type="button"
                        className="shortcut-reset mono"
                        onClick={() => resetToDefault(row)}
                      >
                        reset
                      </button>
                    </>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
