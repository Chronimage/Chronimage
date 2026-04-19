import { currentMonitor, getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import logoUrl from '../assets/logo.svg';
import { Icon } from '../primitives/Icon';
import type { Screen } from '../state/ui';

export interface TitlebarProps {
  screen: Screen;
  appName: string;
}

async function handleMaximize() {
  const win = getCurrentWindow();
  const maximized = await win.isMaximized();
  if (!maximized) {
    await win.maximize();
  } else {
    await win.unmaximize();
    const monitor = await currentMonitor();
    if (monitor) {
      const { width, height } = monitor.size;
      const scaleFactor = monitor.scaleFactor;
      // Convert physical pixels → logical, then take 3/4
      const logicalW = Math.round((width / scaleFactor) * 0.75);
      const logicalH = Math.round((height / scaleFactor) * 0.75);
      await win.setSize(new LogicalSize(logicalW, logicalH));
      await win.center();
    }
  }
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
        <button type="button" aria-label="maximize" onClick={handleMaximize}>
          <Icon name="max" size={11} />
        </button>
        <button type="button" aria-label="close" className="close" onClick={() => win.close()}>
          <Icon name="close" size={13} />
        </button>
      </div>
    </div>
  );
}
