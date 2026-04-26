import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Rail } from './chrome/Rail';
import { StatusBar } from './chrome/StatusBar';
import { Titlebar } from './chrome/Titlebar';
import { ShortcutOverlay, useShortcutOverlay } from './primitives/ShortcutOverlay';
import { CatalogScreen, CatalogSidePanel } from './screens/catalog';
import { CullScreen, CullSidePanel } from './screens/cull';
import { CullBinScreen, CullBinSidePanel } from './screens/cullbin';
import { DevelopScreen, DevelopSidePanel } from './screens/develop';
import { MapScreen } from './screens/map/MapScreen';
import { PeopleScreen } from './screens/PeopleScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { useImportProgressListener } from './state/import';
import { useSourceDeleteProgressListener } from './state/sourceDelete';
import { useUi } from './state/ui';
import { appVersion, currentChannel } from './tauri/invoke';
import { error as logError } from './util/log';

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
});

// `useImportProgressListener` depends on `useQueryClient()` to invalidate
// catalog queries live as an import streams in. Split into its own component
// so it mounts inside the `<QueryClientProvider>` tree instead of the App
// body (where the provider isn't in scope yet).
//
// The source-disconnect listener rides in the same bridge — it has the same
// QueryClient dependency and the same root-mount-once requirement.
function ImportProgressBridge() {
  useImportProgressListener();
  useSourceDeleteProgressListener();
  return null;
}

export function App() {
  const screen = useUi((s) => s.screen);
  const setScreen = useUi((s) => s.setScreen);
  const tweaks = useUi((s) => s.tweaks);
  const hydrateFromStore = useUi((s) => s.hydrateFromStore);

  // Phase 4 §6 — `?` anywhere opens the keyboard shortcut overlay.
  const [shortcutOpen, , closeShortcutOverlay] = useShortcutOverlay();

  useEffect(() => {
    // Restore persisted Settings tweaks from plugin-store exactly once at boot.
    hydrateFromStore().catch((e) => logError('hydrateFromStore failed', e));
  }, [hydrateFromStore]);

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
    case 'catalog':
      sidePanel = <CatalogSidePanel albumId={albumId} onAlbumChange={setAlbumId} />;
      mainPanel = <CatalogScreen albumId={albumId} />;
      break;
    case 'cull':
      sidePanel = <CullSidePanel total={40} />;
      mainPanel = <CullScreen />;
      break;
    case 'cullbin':
      sidePanel = <CullBinSidePanel />;
      mainPanel = <CullBinScreen />;
      break;
    case 'develop':
      sidePanel = <DevelopSidePanel />;
      mainPanel = <DevelopScreen />;
      break;
    case 'map':
      mainPanel = <MapScreen />;
      break;
    case 'people':
      mainPanel = <PeopleScreen />;
      break;
    case 'settings':
      mainPanel = <SettingsScreen />;
      break;
  }

  return (
    <QueryClientProvider client={queryClient}>
      <ImportProgressBridge />
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
          {/* Always render a grid slot for the side panel so the main panel
              lands in the `1fr` column; otherwise screens without a side
              panel (Settings, People) sit in the `auto` column and get
              sized to their content instead of filling the canvas. */}
          {sidePanel ?? <div />}
          {mainPanel}
        </div>
        <StatusBar screen={screen} version={version} channel={channel} />
      </div>
      <ShortcutOverlay open={shortcutOpen} onClose={closeShortcutOverlay} />
    </QueryClientProvider>
  );
}
