/**
 * ConfirmDialog — reusable confirmation modal with optional checkbox rows.
 *
 * Usage:
 *
 *   <ConfirmDialog
 *     open={open}
 *     title="Remove 12 photos?"
 *     description="..."
 *     confirmLabel="Remove 12 photos"
 *     confirmTone="danger"
 *     onCancel={() => setOpen(false)}
 *     onConfirm={(selectedOptions) => { ... }}
 *     options={[
 *       { id: 'recycle', label: 'Also move files to Recycle Bin', defaultChecked: false },
 *     ]}
 *   />
 *
 * The overlay captures ESC to cancel. Options are locally controlled and
 * returned to `onConfirm` as a Set<string> of selected ids.
 */

import { useEffect, useRef, useState } from 'react';

export interface ConfirmDialogOption {
  id: string;
  label: string;
  description?: string;
  defaultChecked?: boolean;
  disabled?: boolean;
}

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description?: React.ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  confirmTone?: 'default' | 'danger';
  options?: ConfirmDialogOption[];
  busy?: boolean;
  onCancel: () => void;
  onConfirm: (selected: Set<string>) => void;
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

  // Reset selection when the dialog transitions from closed → open so the
  // user always sees the specified defaults on a fresh open.
  const prevOpen = useRef(open);
  useEffect(() => {
    if (open && !prevOpen.current) {
      const init = new Set<string>();
      for (const o of options) if (o.defaultChecked) init.add(o.id);
      setSelected(init);
    }
    prevOpen.current = open;
  }, [open, options]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) onCancel();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, busy, onCancel]);

  if (!open) return null;

  function toggleOption(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const confirmButtonStyle: React.CSSProperties = {
    padding: '8px 18px',
    fontSize: 13,
    fontWeight: 500,
    borderRadius: 'var(--radius-sm)',
    border: 'none',
    cursor: busy ? 'wait' : 'pointer',
    background: confirmTone === 'danger' ? 'var(--danger)' : 'var(--accent)',
    color: confirmTone === 'danger' ? 'white' : 'var(--accent-ink)',
    opacity: busy ? 0.6 : 1,
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      tabIndex={-1}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0, 0, 0, 0.55)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 100,
        padding: 24,
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
      onKeyDown={(e) => {
        // Mirror the ESC handler on the backdrop itself so tools that check
        // for keyboard parity with click handlers are satisfied. The
        // window-level listener still covers every other focus target.
        if (e.key === 'Escape' && !busy) onCancel();
      }}
    >
      <div
        style={{
          background: 'var(--bg-elev)',
          border: '1px solid var(--stroke)',
          borderRadius: 'var(--radius-lg)',
          maxWidth: 520,
          width: '100%',
          padding: 'var(--space-5)',
          boxShadow: '0 20px 60px rgba(0, 0, 0, 0.5)',
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-4)',
        }}
      >
        <h2
          id="confirm-dialog-title"
          style={{ margin: 0, fontSize: 18, fontWeight: 500, color: 'var(--fg)' }}
        >
          {title}
        </h2>

        {description && (
          <div style={{ fontSize: 13, color: 'var(--fg-dim)', lineHeight: 1.5 }}>{description}</div>
        )}

        {options.length > 0 && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            {options.map((opt) => (
              <label
                key={opt.id}
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 10,
                  padding: '8px 10px',
                  border: '1px solid var(--stroke)',
                  borderRadius: 'var(--radius-sm)',
                  cursor: opt.disabled ? 'default' : 'pointer',
                  opacity: opt.disabled ? 0.6 : 1,
                }}
              >
                <input
                  type="checkbox"
                  checked={selected.has(opt.id)}
                  disabled={opt.disabled || busy}
                  onChange={() => toggleOption(opt.id)}
                  style={{ marginTop: 3, flexShrink: 0 }}
                />
                <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                  <span style={{ fontSize: 13, color: 'var(--fg)' }}>{opt.label}</span>
                  {opt.description && (
                    <span style={{ fontSize: 11.5, color: 'var(--fg-mute)' }}>{opt.description}</span>
                  )}
                </div>
              </label>
            ))}
          </div>
        )}

        <div
          style={{
            display: 'flex',
            justifyContent: 'flex-end',
            gap: 'var(--space-2)',
            marginTop: 'var(--space-2)',
          }}
        >
          <button
            type="button"
            className="btn2 ghost"
            onClick={onCancel}
            disabled={busy}
            style={{ padding: '8px 14px', fontSize: 13 }}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            onClick={() => onConfirm(selected)}
            disabled={busy}
            style={confirmButtonStyle}
          >
            {busy ? 'Working…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
