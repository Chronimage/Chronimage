import type { Screen } from '../state/ui';

export interface StatusBarProps {
  screen: Screen;
  version: string;
  channel: string;
}

export function StatusBar({ screen, version, channel }: StatusBarProps) {
  return (
    <div className="statusbar">
      <span className="pill">
        <span className="dot" />
        moondream2 · on-device
      </span>
      <span>Chronimage — {screen.label}</span>
      <div className="right">
        <span>
          v{version}
          {channel !== 'stable' ? ` · ${channel}` : ''}
        </span>
      </div>
    </div>
  );
}
