export interface ToggleProps {
  on: boolean;
  onChange: (on: boolean) => void;
  label?: string;
}

export function Toggle({ on, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      onClick={() => onChange(!on)}
      aria-pressed={on}
      aria-label={label ?? 'Toggle'}
      style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}
    >
      <span
        style={{
          width: 32,
          height: 18,
          borderRadius: 999,
          background: on ? 'var(--accent)' : 'var(--bg-elev)',
          border: '1px solid var(--stroke)',
          position: 'relative',
          transition: 'all 0.15s',
        }}
      >
        <span
          style={{
            position: 'absolute',
            top: 1,
            left: on ? 15 : 1,
            width: 14,
            height: 14,
            borderRadius: '50%',
            background: on ? 'var(--accent-ink)' : 'var(--fg-dim)',
            transition: 'left 0.15s',
          }}
        />
      </span>
      {label && <span style={{ fontSize: 12, color: 'var(--fg-dim)' }}>{label}</span>}
    </button>
  );
}
