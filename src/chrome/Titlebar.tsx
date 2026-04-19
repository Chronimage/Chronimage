import { Icon } from '../primitives/Icon';
import type { Screen } from '../state/ui';

export interface TitlebarProps {
  screen: Screen;
  appName: string;
}

export function Titlebar({ screen, appName }: TitlebarProps) {
  const head = appName.slice(0, -2);
  const tail = appName.slice(-2);

  return (
    <div className="titlebar">
      <div className="brand">
        {head}
        <em>{tail}</em>
      </div>
      <div className="crumbs mono">
        <span>Catalog</span>
        <span className="sep">/</span>
        <span style={{ color: 'var(--fg)' }}>{screen.label}</span>
      </div>
      <div className="spacer" />
      <div className="win-ctrls">
        <button type="button" aria-label="minimize">
          <Icon name="min" size={13} />
        </button>
        <button type="button" aria-label="maximize">
          <Icon name="max" size={11} />
        </button>
        <button type="button" aria-label="close" className="close">
          <Icon name="close" size={13} />
        </button>
      </div>
    </div>
  );
}
