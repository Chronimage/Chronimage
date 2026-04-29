import type { Screen } from '../state/ui';

export interface StatusBarProps {
  readonly screen: Screen;
  readonly version: string;
  readonly channel: string;
}

export function StatusBar({ screen, version, channel }: StatusBarProps) {
  return (
    <footer className="statusbar">
      <span className="statusbar-pill">
        <span className="statusbar-dot" aria-hidden="true" />
        <span>moondream2</span>
        <span className="statusbar-sep">·</span>
        <span className="statusbar-soft">on-device</span>
      </span>
      <span className="statusbar-sep">·</span>
      <span className="statusbar-soft">{screen.label}</span>
      <div className="statusbar-right">
        <span>v{version}</span>
        {channel !== 'stable' && (
          <>
            <span className="statusbar-sep">·</span>
            <span className="statusbar-channel">{channel}</span>
          </>
        )}
      </div>
    </footer>
  );
}
