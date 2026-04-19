import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Rail } from './chrome/Rail';
import { StatusBar } from './chrome/StatusBar';
import { Titlebar } from './chrome/Titlebar';
import { CatalogScreen, CatalogSidePanel } from './screens/catalog';
import { OnboardScreen } from './screens/OnboardScreen';
import { PlaceholderScreen } from './screens/PlaceholderScreen';
import { useUi } from './state/ui';
import { appVersion, currentChannel } from './tauri/invoke';
import { error as logError } from './util/log';

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
});

export function App() {
  const screen = useUi((s) => s.screen);
  const setScreen = useUi((s) => s.setScreen);
  const tweaks = useUi((s) => s.tweaks);

  const [albumId, setAlbumId] = useState<string>('all');
  const [version, setVersion] = useState<string>('0.0.0-dev');
  const [channel, setChannel] = useState<string>('dev');

  useEffect(() => {
    appVersion()
      .then(setVersion)
      .catch((e) => logError('appVersion failed', e));
    currentChannel()
      .then((c) => setChannel(c.channel))
      .catch((e) => logError('currentChannel failed', e));
  }, []);

  useEffect(() => {
    const html = document.documentElement;
    html.setAttribute('data-theme', tweaks.theme);
    html.setAttribute('data-accent', tweaks.accent);
  }, [tweaks.theme, tweaks.accent]);

  let sidePanel: React.ReactNode = null;
  let mainPanel: React.ReactNode = null;

  switch (screen.id) {
    case 'onboard':
      mainPanel = <OnboardScreen />;
      break;
    case 'catalog':
      sidePanel = <CatalogSidePanel albumId={albumId} onAlbumChange={setAlbumId} />;
      mainPanel = <CatalogScreen albumId={albumId} />;
      break;
    case 'cull':
      mainPanel = (
        <PlaceholderScreen
          title="Cull"
          phase="Phase 2 · not yet shipped"
          description="Compare / Grid / Swipe review modes with AI picks, issue flags, keyboard verdicts, and a recoverable Cull Bin. Lands after the catalog MVP is stable."
        />
      );
      break;
    case 'cullbin':
      mainPanel = (
        <PlaceholderScreen
          title="Cull Bin"
          phase="Phase 2 · not yet shipped"
          description="Rejects live here for 30 days before permanent deletion. Restore any item, audit every cleanup action."
        />
      );
      break;
    case 'develop':
      mainPanel = (
        <PlaceholderScreen
          title="Develop"
          phase="Phase 3 · not yet shipped"
          description="Lightweight RAW editor — exposure, curves, masks, and the AI preset library. Sony A7 IV ARW is the priority RAW format."
        />
      );
      break;
    case 'settings':
      mainPanel = (
        <PlaceholderScreen
          title="Settings"
          phase="Phase 1+ · partial"
          description="Model picker, storage location, cull thresholds, release channel. Wires up as Phase 1 features land."
        />
      );
      break;
  }

  return (
    <QueryClientProvider client={queryClient}>
      <div
        className="app compact"
        data-theme={tweaks.theme}
        data-accent={tweaks.accent}
        data-density={tweaks.gridDensity}
        data-facet={tweaks.facetPlacement}
        data-editor={tweaks.editorLayout}
        style={{ ['--display-font' as string]: `'${tweaks.displayFont}', serif` }}
      >
        <Titlebar screen={screen} appName={tweaks.appName} />
        <div className="body">
          <Rail screen={screen} onScreenChange={setScreen} />
          {sidePanel}
          {mainPanel}
        </div>
        <StatusBar screen={screen} version={version} channel={channel} />
      </div>
    </QueryClientProvider>
  );
}
