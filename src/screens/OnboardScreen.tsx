import { Chip } from '../primitives/Chip';
import { Icon } from '../primitives/Icon';
import { SOURCES } from '../state/fixtures';

/**
 * Phase 1 stub onboarding. The full 5-step flow (Welcome → Sources → Import
 * → Models → People) lands in a dedicated session after the import pipeline
 * is wired. Here we render the Sources landing so the screen isn't dead.
 */
export function OnboardScreen() {
  const connected = SOURCES.filter((s) => s.kind === 'iphone' || s.kind === 'android' || s.kind === 'card');
  const rest = SOURCES.filter((s) => !['iphone', 'android', 'card'].includes(s.kind)).slice(0, 6);

  return (
    <div className="canvas" style={{ gridColumn: '2 / -1' }}>
      <div style={{ padding: 48, maxWidth: 820, margin: '0 auto' }}>
        <div
          className="mono"
          style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em', marginBottom: 14 }}
        >
          WELCOME
        </div>
        <h1 className="display" style={{ fontSize: 64, margin: 0, lineHeight: 0.96 }}>
          Your photos,
          <br />
          <em>in one light.</em>
        </h1>
        <p style={{ color: 'var(--fg-dim)', fontSize: 14, lineHeight: 1.55, maxWidth: 560, marginTop: 18 }}>
          Chronimage unifies your fragmented library (Google Photos, iCloud, iPhone, local disks) into one
          owned catalog. On-device AI tags, clusters faces, detects duplicates (including RAW+JPG pairs), and
          safely frees up source storage once local copies are verified.
        </p>

        {connected.length > 0 && (
          <div
            style={{
              marginTop: 24,
              padding: 14,
              border: '1px solid var(--accent)',
              borderRadius: 10,
              background: 'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))',
            }}
          >
            <div
              className="mono"
              style={{ fontSize: 10.5, color: 'var(--accent)', marginBottom: 10, letterSpacing: '0.08em' }}
            >
              <Icon name="usb" size={11} /> CONNECTED NOW · USB
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {connected.map((s) => (
                <div
                  key={s.id}
                  className="source-row"
                  style={{ padding: '10px 12px', marginBottom: 0, background: 'var(--bg)' }}
                >
                  <div className="ico">
                    <Icon name={s.kind} size={18} />
                  </div>
                  <div style={{ flex: 1 }}>
                    <div className="name">{s.name}</div>
                    <div className="sub">
                      {s.count} photos · {s.sub}
                    </div>
                  </div>
                  <button type="button" className="btn2 primary" style={{ padding: '6px 12px' }}>
                    Import new
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        <div
          className="mono"
          style={{
            fontSize: 10.5,
            color: 'var(--fg-mute)',
            marginTop: 24,
            marginBottom: 10,
            letterSpacing: '0.08em',
          }}
        >
          ALL SOURCES
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {rest.map((s) => (
            <div key={s.id} className="source-row" style={{ padding: '10px 12px', marginBottom: 0 }}>
              <div className="ico">
                <Icon name={s.kind} size={18} />
              </div>
              <div style={{ flex: 1 }}>
                <div className="name">{s.name}</div>
                <div className="sub">
                  {s.count} photos · {s.sub}
                </div>
              </div>
              <Chip
                {...(s.status === 'ready' ? ({ variant: 'solid' } as const) : {})}
                {...(s.status === 'syncing'
                  ? ({ tone: 'info' } as const)
                  : s.status === 'paused'
                    ? ({ tone: 'warn' } as const)
                    : {})}
              >
                {s.status === 'synced'
                  ? 'Connected'
                  : s.status === 'syncing'
                    ? 'Syncing'
                    : s.status === 'paused'
                      ? 'Paused'
                      : s.status === 'ready'
                        ? 'Ready'
                        : 'Idle'}
              </Chip>
            </div>
          ))}
        </div>

        <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 32 }}>
          Phase 1 in progress — real source connectors + import pipeline land in the next session.
        </div>
      </div>
    </div>
  );
}
