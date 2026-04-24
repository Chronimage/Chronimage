import { useState } from 'react';

type CurveChannel = 'RGB' | 'R' | 'G' | 'B' | 'L';

const CHANNELS: CurveChannel[] = ['RGB', 'R', 'G', 'B', 'L'];

export function CurvesPanel() {
  const [channel, setChannel] = useState<CurveChannel>('RGB');

  return (
    <div>
      <div style={{ display: 'flex', gap: 4, marginBottom: 8 }}>
        {CHANNELS.map((c) => {
          const on = channel === c;
          return (
            <button
              key={c}
              type="button"
              onClick={() => setChannel(c)}
              className="mono"
              style={{
                padding: '3px 8px',
                borderRadius: 5,
                fontSize: 11,
                background: on ? 'var(--bg-elev)' : 'transparent',
                color: on ? 'var(--fg)' : 'var(--fg-dim)',
                border: `1px solid ${on ? 'var(--stroke-strong)' : 'var(--stroke)'}`,
              }}
              aria-pressed={on}
            >
              {c}
            </button>
          );
        })}
      </div>
      <div className="curves-box">
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
          <defs>
            <pattern id="curves-grid" width="25" height="25" patternUnits="userSpaceOnUse">
              <path d="M 25 0 L 0 0 0 25" fill="none" stroke="var(--stroke)" strokeWidth="0.3" />
            </pattern>
          </defs>
          <rect width="100" height="100" fill="url(#curves-grid)" />
          <path
            d="M0,100 L5,92 L12,78 L22,60 L33,44 L45,52 L58,58 L68,62 L78,72 L88,84 L95,92 L100,100 Z"
            fill="color-mix(in oklch, var(--fg) 15%, transparent)"
          />
          <path
            d="M0,100 C 20,96 34,68 50,48 S 80,14 100,0"
            fill="none"
            stroke="var(--accent)"
            strokeWidth="1.4"
          />
          <line
            x1="0"
            y1="100"
            x2="100"
            y2="0"
            stroke="var(--stroke-strong)"
            strokeWidth="0.4"
            strokeDasharray="1 1"
          />
          <circle cx="20" cy="88" r="2" fill="var(--accent)" />
          <circle cx="50" cy="48" r="2" fill="var(--accent)" />
          <circle cx="80" cy="14" r="2" fill="var(--accent)" />
        </svg>
      </div>
      <div
        className="mono"
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          marginTop: 6,
          fontSize: 10,
          color: 'var(--fg-mute)',
        }}
      >
        <span>Blacks</span>
        <span>Shadows</span>
        <span>Mids</span>
        <span>Highlights</span>
        <span>Whites</span>
      </div>
    </div>
  );
}
