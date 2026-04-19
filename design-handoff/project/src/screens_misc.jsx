// Onboarding, Detail, Search, Batch, Settings


const DetailInspector = ({ photo }) => (
  <div className="inspector">
    <div className="ihead">
      <div className="name">{photo.filename}</div>
      <div style={{display:'flex', gap: 4}}>
        <button style={{color:'var(--fg-dim)'}}><Icon name="star" size={14}/></button>
        <button style={{color:'var(--fg-dim)'}}><Icon name="flag" size={14}/></button>
      </div>
    </div>
    <div className="body2">
      <div className="group">
        <h4>AI Tags · confidence</h4>
        <div style={{display:'flex', flexWrap:'wrap', gap: 6}}>
          <Chip variant="solid">Ari · 99%</Chip>
          <Chip tone="">portrait · 96%</Chip>
          <Chip tone="">outdoor · 93%</Chip>
          <Chip tone="">golden hour · 88%</Chip>
          <Chip tone="">garden · 81%</Chip>
          <Chip tone="">smiling · 78%</Chip>
          <Chip tone="">shallow DoF · 74%</Chip>
        </div>
        <button className="btn2 ghost" style={{width:'100%', justifyContent:'center', marginTop: 10, padding: '8px'}}>
          <Icon name="sparkles" size={13}/> Re-tag with gemma4
        </button>
      </div>
      <div className="group">
        <h4>Stack · 6 similar</h4>
        <div style={{display:'grid', gridTemplateColumns:'repeat(6, 1fr)', gap: 4}}>
          {PHOTOS.slice(0, 6).map((p, i) => (
            <div key={p.id} style={{aspectRatio: '1'}}><Placeholder photo={p} idx={i} showLabel={false}/></div>
          ))}
        </div>
      </div>
      <div className="group">
        <h4>Quality</h4>
        <div className="kv"><span>Sharpness</span><b>0.91</b></div>
        <div className="kv"><span>Face clarity</span><b>0.96</b></div>
        <div className="kv"><span>Eyes open</span><b>Yes · 0.98</b></div>
        <div className="kv"><span>Exposure</span><b>Balanced</b></div>
        <div className="kv"><span>Aesthetic score</span><b>8.4 / 10</b></div>
      </div>
      <div className="group">
        <h4>EXIF</h4>
        <div className="kv"><span>Camera</span><b>{photo.cam}</b></div>
        <div className="kv"><span>Lens</span><b>{photo.lens}</b></div>
        <div className="kv"><span>Aperture</span><b>f/{photo.ap}</b></div>
        <div className="kv"><span>Shutter</span><b>{photo.shut}</b></div>
        <div className="kv"><span>ISO</span><b>{photo.iso}</b></div>
        <div className="kv"><span>Focal</span><b>{photo.mm} mm</b></div>
        <div className="kv"><span>Captured</span><b>{photo.date}</b></div>
        <div className="kv"><span>Size</span><b>{photo.sizeMb} MB</b></div>
      </div>
      <div className="group">
        <h4>Location</h4>
        <div style={{aspectRatio: '16/10', background: 'var(--bg-elev)', border: '1px solid var(--stroke)', borderRadius: 8, position: 'relative', overflow:'hidden'}}>
          <svg width="100%" height="100%" viewBox="0 0 200 120" preserveAspectRatio="none">
            <defs>
              <pattern id="gr" width="10" height="10" patternUnits="userSpaceOnUse"><path d="M 10 0 L 0 0 0 10" fill="none" stroke="rgba(255,255,255,0.05)" strokeWidth="0.5"/></pattern>
            </defs>
            <rect width="200" height="120" fill="url(#gr)"/>
            <path d="M0,80 Q50,60 100,72 T200,64" stroke="var(--fg-mute)" strokeWidth="0.8" fill="none"/>
            <path d="M0,40 Q60,30 120,42 T200,36" stroke="var(--fg-mute)" strokeWidth="0.5" fill="none" strokeDasharray="2 2"/>
            <circle cx="108" cy="62" r="5" fill="var(--accent)"/>
            <circle cx="108" cy="62" r="10" fill="none" stroke="var(--accent)" strokeWidth="1" opacity="0.4"/>
          </svg>
        </div>
        <div className="kv" style={{marginTop: 8}}><span>Place</span><b>Kyoto, JP</b></div>
        <div className="kv"><span>GPS</span><b>35.0116, 135.7681</b></div>
      </div>
    </div>
  </div>
);

const DetailScreen = ({ photo }) => (
  <div className="canvas">
    <div className="toolbar">
      <button className="btn"><Icon name="chevL" size={14}/> Back</button>
      <div className="divider"/>
      <span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>{photo.filename} · {photo.w}×{photo.h} · {photo.ext}</span>
      <div style={{flex: 1}}/>
      <button className="btn"><Icon name="star" size={14}/> Rate</button>
      <button className="btn"><Icon name="flag" size={14}/> Flag</button>
      <div className="divider"/>
      <button className="btn"><Icon name="brush" size={14}/> Develop</button>
      <button className="btn2 primary"><Icon name="sparkles" size={14}/> Edit with prompt</button>
    </div>
    <div className="detail-stage">
      <div className="detail-hero">
        <div style={{width:'100%', maxWidth: 1000, aspectRatio:'3/2', maxHeight:'100%'}}>
          <Placeholder photo={photo} idx={0} showLabel={false}/>
        </div>
      </div>
      <div style={{padding: '16px 28px 24px'}}>
        <div style={{display:'flex', alignItems:'baseline', gap: 16, marginBottom: 10}}>
          <div className="display" style={{fontSize: 36}}>{photo.scene}</div>
          <span className="mono" style={{fontSize: 12, color:'var(--fg-mute)'}}>{photo.date} · {photo.cam}</span>
        </div>
        <div style={{display:'flex', gap: 6, flexWrap:'wrap'}}>
          <Chip variant="solid">Ari</Chip>
          <Chip>portrait</Chip>
          <Chip>outdoor</Chip>
          <Chip>golden hour</Chip>
          <Chip>garden</Chip>
          <Chip>smiling</Chip>
          <Chip>shallow DoF</Chip>
          <Chip tone="info">aesthetic 8.4</Chip>
        </div>
      </div>
    </div>
  </div>
);

const SearchScreen = () => {
  const [q, setQ] = React.useState("Ari laughing outside in late afternoon");
  const results = PHOTOS.slice(0, 12);
  return (
    <div className="canvas">
      <div className="canvas-scroll">
        <div className="search-stage">
          <div className="search-hero">
            <h1>Ask your library<br/><em>in plain English.</em></h1>
            <div className="search-input">
              <Icon name="search" size={18}/>
              <input value={q} onChange={e=>setQ(e.target.value)}/>
              <span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>⌘↵</span>
              <button className="btn2 primary" style={{padding: '8px 14px'}}>Search</button>
            </div>
            <div className="search-sug">
              {SEARCH_SUGGESTIONS.map(s => <span key={s} className="sug" onClick={()=>setQ(s)}>{s}</span>)}
            </div>
          </div>
          <div style={{marginTop: 36}}>
            <div style={{display:'flex', alignItems:'baseline', justifyContent:'space-between', marginBottom: 12}}>
              <div>
                <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>312 RESULTS · CLIP + gemma4 · 0.38s</div>
                <div className="display" style={{fontSize: 28, marginTop: 4}}>"{q}"</div>
              </div>
              <div style={{display:'flex', gap: 8}}>
                <Chip>last 2 years</Chip>
                <Chip>any camera</Chip>
                <Chip variant="solid">Ari only</Chip>
              </div>
            </div>
            <div style={{display:'grid', gridTemplateColumns:'repeat(6,1fr)', gap: 8, paddingBottom: 28}}>
              {results.map((p, i) => (
                <div key={p.id} style={{position:'relative', aspectRatio:'4/3'}}>
                  <Placeholder photo={p} idx={i} showLabel={false}/>
                  <div style={{position:'absolute', top: 8, left: 8}}><Chip variant="solid" style={{fontSize: 10, padding: '2px 6px'}}>{(0.95 - i*0.03).toFixed(2)}</Chip></div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

const SettingsScreen = ({ appName = 'Halide', setAppName }) => (
  <div className="canvas">
    <div className="canvas-scroll">
      <div className="settings">
        <h1>Settings<em>.</em></h1>
        <div className="set-section">
          <h3>Identity</h3>
          <div className="set-row">
            <div className="lbl">App name<div className="sub">Shown in titlebar & dock</div></div>
            <input className="tx-input" defaultValue={appName} onBlur={(e)=>setAppName && setAppName(e.target.value)}
              style={{background:'var(--surface-2)', border:'1px solid var(--stroke)', borderRadius:6, padding:'6px 10px', color:'var(--fg)', fontFamily:'var(--mono-font)', fontSize:12, minWidth:220}}/>
          </div>
        </div>
        <div className="set-section">
          <h3>AI Models</h3>
          <div className="set-row">
            <div className="lbl">Tagging & scene<div className="sub">On-device · CPU/GPU</div></div>
            <div style={{display:'flex', flexDirection:'column', gap: 8}}>
              <Seg value="gemma4-27b" onChange={()=>{}} options={[{value:'gemma4-27b',label:'gemma4-27b'},{value:'gemma4-9b',label:'gemma4-9b'},{value:'llava-1.6',label:'LLaVA-1.6'}]}/>
              <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>17.3 GB · VRAM 12GB required · runs on NVIDIA 3060+</div>
            </div>
          </div>
          <div className="set-row">
            <div className="lbl">Embeddings<div className="sub">For semantic search</div></div>
            <Seg value="clip-l14" onChange={()=>{}} options={[{value:'clip-l14',label:'CLIP-L14'},{value:'siglip',label:'SigLIP'}]}/>
          </div>
          <div className="set-row">
            <div className="lbl">Face recognition<div className="sub">Local face DB, encrypted</div></div>
            <div style={{display:'flex', alignItems:'center', gap: 12}}><Toggle on={true} onChange={()=>{}}/><span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>58 people · 12,430 face embeddings</span></div>
          </div>
          <div className="set-row">
            <div className="lbl">Prompt editing<div className="sub">Generative edits</div></div>
            <Seg value="sdxl-inpaint" onChange={()=>{}} options={[{value:'sdxl-inpaint',label:'SDXL inpaint'},{value:'flux-dev',label:'Flux-dev'},{value:'cloud',label:'Cloud · optional'}]}/>
          </div>
        </div>
        <div className="set-section">
          <h3>Culling thresholds</h3>
          <div className="set-row"><div className="lbl">Duplicate similarity</div><Slider label="Threshold" value={85} onChange={()=>{}} min={50} max={100} suffix="%"/></div>
          <div className="set-row"><div className="lbl">Sharpness cutoff</div><Slider label="Score" value={32} onChange={()=>{}} min={0} max={100}/></div>
          <div className="set-row"><div className="lbl">Require final review</div><div style={{display:'flex', alignItems:'center', gap: 12}}><Toggle on={true} onChange={()=>{}}/><span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>Rejects moved to trash only after you confirm</span></div></div>
        </div>
        <div className="set-section">
          <h3>Storage & indexing</h3>
          <div className="set-row"><div className="lbl">Cache location</div><div style={{fontFamily:'var(--mono-font)', fontSize:12}}>D:/Chronimage/cache · 84.2 GB</div></div>
          <div className="set-row"><div className="lbl">Nightly re-index</div><div style={{display:'flex', alignItems:'center', gap: 12}}><Toggle on={true} onChange={()=>{}}/><span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>02:00 · Wake from sleep</span></div></div>
        </div>
      </div>
    </div>
  </div>
);

Object.assign(window, { DetailScreen, DetailInspector, SearchScreen, SettingsScreen });
