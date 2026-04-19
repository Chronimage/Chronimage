// App root
const { useState: useS_app, useEffect: useE_app } = React;

function App() {
  const [screen, setScreen] = useS_app({ id: 'onboard', label: 'Sources' });
  const [albumId, setAlbumId] = useS_app('all');
  const [tweaks, setTweaks] = useS_app({ ...window.__TWEAKS__ });
  const [tweaksOpen, setTweaksOpen] = useS_app(false);
  const [devTab, setDevTab] = useS_app('develop');
  const [focusedPhoto, setFocusedPhoto] = useS_app(null);
  const [exportOpen, setExportOpen] = useS_app(false);
  const [exportCtx, setExportCtx] = useS_app({ count: 42, context: 'catalog' });
  const openExport = (count, context) => { setExportCtx({ count, context }); setExportOpen(true); };
  window.__openExport = openExport;
  const appName = tweaks.appName || 'Halide';

  useE_app(() => {
    const handler = (e) => {
      if (e.data?.type === '__activate_edit_mode') setTweaksOpen(true);
      if (e.data?.type === '__deactivate_edit_mode') setTweaksOpen(false);
    };
    window.addEventListener('message', handler);
    window.parent.postMessage({type:'__edit_mode_available'}, '*');
    return () => window.removeEventListener('message', handler);
  }, []);

  useE_app(() => {
    window.parent.postMessage({type:'__edit_mode_set_keys', edits: tweaks}, '*');
  }, [tweaks]);

  let sidePanel = null, mainPanel = null, inspector = null;

  if (screen.id === 'catalog') {
    sidePanel = <CatalogSidePanel albumId={albumId} setAlbumId={setAlbumId}/>;
    mainPanel = <CatalogScreen albumId={albumId} onFocusChange={setFocusedPhoto}/>;
    inspector = focusedPhoto ? <DetailInspector photo={focusedPhoto}/> : null;
  } else if (screen.id === 'cull') {
    sidePanel = <CullSidePanel mode={tweaks.cullMode} setMode={(m)=>setTweaks({...tweaks, cullMode:m})} idx={184} total={496}/>;
    mainPanel = <CullScreen/>;
  } else if (screen.id === 'cullbin') {
    sidePanel = <CullBinSidePanel/>;
    mainPanel = <CullBinScreen/>;
  } else if (screen.id === 'develop') {
    sidePanel = <DevelopSidePanel tab={devTab} setTab={setDevTab}/>;
    mainPanel = <DevelopScreen tab={devTab} setTab={setDevTab}/>;
    inspector = devTab !== 'prompt' ? <EditorInspector/> : null;
  } else if (screen.id === 'detail') {
    // legacy route — redirect to catalog with a focused photo
    sidePanel = <CatalogSidePanel albumId={albumId} setAlbumId={setAlbumId}/>;
    mainPanel = <CatalogScreen albumId={albumId} onFocusChange={setFocusedPhoto}/>;
    inspector = focusedPhoto ? <DetailInspector photo={focusedPhoto}/> : null;
  } else if (screen.id === 'settings') {
    mainPanel = <SettingsScreen appName={appName} setAppName={(n)=>setTweaks({...tweaks, appName:n})}/>;
  } else if (screen.id === 'onboard') {
    mainPanel = <OnboardScreen appName={appName}/>;
  }

  return (
    <div className="app compact"
         data-theme={tweaks.theme}
         data-accent={tweaks.accent}
         data-density={tweaks.gridDensity}
         data-facet={tweaks.facetPlacement}
         data-editor={tweaks.editorLayout}
         style={{'--display-font': `'${tweaks.displayFont}', serif`}}>
      <Titlebar screen={screen} appName={appName}/>
      <div className="body">
        <Rail screen={screen} setScreen={setScreen}/>
        {sidePanel}
        {mainPanel}
        {inspector}
      </div>
      <StatusBar screen={screen}/>
      <TweaksPanel open={tweaksOpen} onClose={()=>setTweaksOpen(false)} tweaks={tweaks} setTweaks={setTweaks}/>
      <ExportSheet open={exportOpen} onClose={()=>setExportOpen(false)} count={exportCtx.count} context={exportCtx.context}/>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App/>);
