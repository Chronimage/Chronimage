import { getCurrentWindow } from '@tauri-apps/api/window';
import logoUrl from '../assets/logo.svg';
import { Icon } from '../primitives/Icon';
import type { Screen } from '../state/ui';

export interface TitlebarProps {
  screen: Screen;
  appName: string;
}

export function Titlebar({ screen, appName }: TitlebarProps) {
  const head = appName.slice(0, -2);
  const tail = appName.slice(-2);

  const win = getCurrentWindow();

  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="brand">
        <img src={logoUrl} alt="" width={22} height={22} className="brand-logo" />
        {head}
        <em>{tail}</em>
      </div>
      <div className="crumbs mono">
        <span>Catalog</span>
        <span className="sep">/</span>
        <span style={{ color: 'var(--fg)' }}>{screen.label}</span>
      </div>
      <div className="spacer" data-tauri-drag-region />
      <div className="win-ctrls">
        <button type="button" aria-label="minimize" onClick={() => win.minimize()}>
          <Icon name="min" size={13} />
        </button>
        <button type="button" aria-label="maximize" onClick={() => win.toggleMaximize()}>
          <Icon name="max" size={11} />
        </button>
        <button type="button" aria-label="close" className="close" onClick={() => win.close()}>
          <Icon name="close" size={13} />
        </button>
      </div>
    </div>
  );
}
