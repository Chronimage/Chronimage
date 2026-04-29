import { currentMonitor, getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { Minus, Square, X } from 'lucide-react';
import logoUrl from '../assets/logo.svg';
import type { Screen } from '../state/ui';

export interface TitlebarProps {
  readonly screen: Screen;
  readonly appName: string;
}

async function handleMaximize() {
  const win = getCurrentWindow();
  const maximized = await win.isMaximized();
  if (maximized) {
    await win.unmaximize();
    const monitor = await currentMonitor();
    if (monitor) {
      const { width, height } = monitor.size;
      const scaleFactor = monitor.scaleFactor;
      const logicalW = Math.round((width / scaleFactor) * 0.75);
      const logicalH = Math.round((height / scaleFactor) * 0.75);
      await win.setSize(new LogicalSize(logicalW, logicalH));
      await win.center();
    }
  } else {
    await win.maximize();
  }
}

export function Titlebar({ screen, appName }: TitlebarProps) {
  const head = appName.slice(0, -2);
  const tail = appName.slice(-2);
  const win = getCurrentWindow();

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand">
        <img src={logoUrl} alt="" width={20} height={20} className="titlebar-logo" />
        <span className="titlebar-wordmark">
          {head}
          <em>{tail}</em>
        </span>
      </div>

      <div className="titlebar-crumbs label-mono">
        <span>Catalog</span>
        <span aria-hidden="true" className="titlebar-crumb-sep">
          /
        </span>
        <span className="titlebar-crumb-current">{screen.label}</span>
      </div>

      <div className="titlebar-spacer" data-tauri-drag-region />

      <div className="titlebar-ctrls">
        <button type="button" aria-label="Minimize" onClick={() => win.minimize()}>
          <Minus className="size-3" strokeWidth={1.75} />
        </button>
        <button type="button" aria-label="Maximize" onClick={handleMaximize}>
          <Square className="size-2.5" strokeWidth={1.75} />
        </button>
        <button type="button" aria-label="Close" className="close" onClick={() => win.close()}>
          <X className="size-3" strokeWidth={1.75} />
        </button>
      </div>
    </header>
  );
}
