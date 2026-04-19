/**
 * Generic "this screen ships in Phase N" placeholder. Used by Cull, Cull Bin,
 * Develop, and Settings until those phases land.
 */

export interface PlaceholderScreenProps {
  title: string;
  phase: string;
  description: string;
}

export function PlaceholderScreen({ title, phase, description }: PlaceholderScreenProps) {
  return (
    <div className="canvas" style={{ gridColumn: '2 / -1' }}>
      <div
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 14,
          padding: 48,
          textAlign: 'center',
        }}
      >
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em' }}>
          {phase.toUpperCase()}
        </div>
        <h1 className="display" style={{ fontSize: 44, margin: 0 }}>
          {title}
          <em>.</em>
        </h1>
        <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>{description}</p>
      </div>
    </div>
  );
}
