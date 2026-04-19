// Develop — unified RAW editor with Prompt tab, Curves, Copy/Paste edits, Export+Archive
const { useState: useS_dev } = React;

const DevelopSidePanel = ({ tab, setTab }) => {
  const [cat, setCat] = useS_dev('face');
  const cats = [
    {id:'face', l:'Face'},
    {id:'scene', l:'Scene'},
    {id:'quality', l:'Quality'},
    {id:'style', l:'Style'},
    {id:'custom', l:'My presets'},
  ];
  const catMap = {face:'Face', scene:'Scene', quality:'Quality', style:'Style'};
  const filtered = PRESETS.filter(p => cat==='custom' ? false : p.group === catMap[cat]);

  return (
    <div className="sidepanel">
      <div className="head"><h3>Presets</h3><span className="count mono">gemma4</span></div>

      <div className="preset-active-sticky">
        <div className="lbl">Active · Clean up face</div>
        <div className="main"><span>Strength</span><span className="val">55</span></div>
        <input type="range" min="0" max="100" defaultValue={55}/>
        <div style={{display:'flex', gap: 6, marginTop: 8, flexWrap:'wrap'}}>
          <Chip variant="solid" onClose={()=>{}}>Clean up face · 55</Chip>
          <Chip onClose={()=>{}}>Whiten teeth · 30</Chip>
          <Chip onClose={()=>{}}>Enhance sky · 72</Chip>
        </div>
      </div>

      <div className="preset-cats">
        {cats.map(c => <button key={c.id} className={cat===c.id?'on':''} onClick={()=>setCat(c.id)}>{c.l}</button>)}
      </div>

      <div style={{padding: '10px 12px', display:'flex', flexDirection:'column', gap: 6, overflow:'auto'}}>
        {cat === 'custom' ? (
          <div style={{padding: 24, textAlign:'center', color:'var(--fg-mute)', fontSize: 12.5}}>
            <Icon name="sparkles" size={18}/>
            <div style={{marginTop: 8}}>Save any combination as a preset.</div>
            <button className="btn2" style={{marginTop: 12, justifyContent:'center', width:'100%'}}><Icon name="plus" size={13}/> Save current edits</button>
          </div>
        ) : filtered.map((p, i) => (
          <button key={p.id} className={`preset-card ${i===0?'on':''}`}>
            <div className="pv" style={{background: `linear-gradient(135deg, oklch(0.6 0.18 ${(i*47)%360}), oklch(0.25 0.08 ${(i*47)%360}))`}}/>
            <div>
              <div className="name">{p.name}</div>
              <div className="sub">{p.sub}</div>
            </div>
            <div className="val">{i===0?'55':'—'}</div>
          </button>
        ))}
      </div>

      <div style={{marginTop:'auto', padding: 10, borderTop:'1px solid var(--stroke)', display:'flex', gap: 6}}>
        <button className="btn2 ghost" style={{flex: 1, justifyContent:'center', fontSize: 12, padding: '7px'}}><Icon name="plus" size={12}/> Save as preset</button>
        <button className="btn2 ghost" style={{justifyContent:'center', fontSize: 12, padding: '7px'}}><Icon name="download" size={12}/></button>
      </div>
    </div>
  );
};

const CurvesPanel = () => (
  <div>
    <div style={{display:'flex', gap: 4, marginBottom: 8}}>
      {['RGB','R','G','B','L'].map((c,i) => (
        <button key={c} style={{
          padding: '3px 8px', borderRadius: 5, fontSize: 11, fontFamily: 'var(--mono-font)',
          background: i===0?'var(--bg-elev)':'transparent', color: i===0?'var(--fg)':'var(--fg-dim)',
          border: '1px solid ' + (i===0?'var(--stroke-strong)':'var(--stroke)')
        }}>{c}</button>
      ))}
    </div>
    <div className="curves-box">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none">
        <defs>
          <pattern id="grid" width="25" height="25" patternUnits="userSpaceOnUse">
            <path d="M 25 0 L 0 0 0 25" fill="none" stroke="var(--stroke)" strokeWidth="0.3"/>
          </pattern>
        </defs>
        <rect width="100" height="100" fill="url(#grid)"/>
        {/* histogram */}
        <path d="M0,100 L5,92 L12,78 L22,60 L33,44 L45,52 L58,58 L68,62 L78,72 L88,84 L95,92 L100,100 Z" fill="color-mix(in oklch, var(--fg) 15%, transparent)"/>
        {/* curve */}
        <path d="M0,100 C 20,96 34,68 50,48 S 80,14 100,0" fill="none" stroke="var(--accent)" strokeWidth="1.4"/>
        <line x1="0" y1="100" x2="100" y2="0" stroke="var(--stroke-strong)" strokeWidth="0.4" strokeDasharray="1 1"/>
        <circle cx="20" cy="88" r="2" fill="var(--accent)"/>
        <circle cx="50" cy="48" r="2" fill="var(--accent)"/>
        <circle cx="80" cy="14" r="2" fill="var(--accent)"/>
      </svg>
    </div>
    <div style={{display:'flex', justifyContent:'space-between', marginTop: 6, fontFamily:'var(--mono-font)', fontSize: 10, color:'var(--fg-mute)'}}>
      <span>Blacks</span><span>Shadows</span><span>Mids</span><span>Highlights</span><span>Whites</span>
    </div>
  </div>
);

const EditorInspector = () => {
  const [v, setV] = useS_dev({ exp: 24, con: -8, hi: -42, sh: 55, temp: 12, tint: -4, vib: 18, sat: 6, clarity: 14, dehaze: 20 });
  const set = (k, x) => setV(s => ({...s, [k]: x}));
  return (
    <div className="inspector editor-order-panel">
      <div className="ihead">
        <div className="name mono">IMG_4067.ARW · 42MP · A7 IV</div>
        <div style={{display:'flex', gap: 4}}>
          <button title="Copy edits" style={{color:'var(--fg-mute)', padding: 4}}><Icon name="layers" size={13}/></button>
          <button title="History" style={{color:'var(--fg-mute)', padding: 4}}><Icon name="history" size={13}/></button>
        </div>
      </div>
      <div className="body2">
        <div className="group">
          <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', marginBottom: 8}}>
            <h4 style={{margin: 0}}>Auto</h4>
            <button className="btn2 primary" style={{padding: '5px 10px', fontSize: 11.5}}><Icon name="sparkles" size={12}/> Auto light · ⌘A</button>
          </div>
          <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
            {['Neutral', 'Vivid', 'Match batch'].map(m => (
              <button key={m} className="btn2 ghost" style={{padding: '6px', fontSize: 11, justifyContent:'center'}}>{m}</button>
            ))}
          </div>
        </div>
        <div className="group">
          <h4>Light</h4>
          <Slider label="Exposure" value={v.exp} onChange={x=>set('exp', x)} suffix=" EV"/>
          <Slider label="Contrast" value={v.con} onChange={x=>set('con', x)}/>
          <Slider label="Highlights" value={v.hi} onChange={x=>set('hi', x)}/>
          <Slider label="Shadows" value={v.sh} onChange={x=>set('sh', x)}/>
        </div>
        <div className="group">
          <h4>Curves</h4>
          <CurvesPanel/>
        </div>
        <div className="group">
          <h4>Color</h4>
          <Slider label="Temp" value={v.temp} onChange={x=>set('temp', x)} suffix="K"/>
          <Slider label="Tint" value={v.tint} onChange={x=>set('tint', x)}/>
          <Slider label="Vibrance" value={v.vib} onChange={x=>set('vib', x)}/>
          <Slider label="Saturation" value={v.sat} onChange={x=>set('sat', x)}/>
        </div>
        <div className="group">
          <h4>Detail</h4>
          <Slider label="Clarity" value={v.clarity} onChange={x=>set('clarity', x)}/>
          <Slider label="Dehaze" value={v.dehaze} onChange={x=>set('dehaze', x)}/>
        </div>
        <div className="group">
          <h4>Copy · Paste · Sync</h4>
          <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 6}}>
            <button className="btn2 ghost" style={{padding: '7px', fontSize: 11.5, justifyContent:'center'}}>Copy <span className="mono" style={{color:'var(--fg-mute)', marginLeft: 4}}>⌘C</span></button>
            <button className="btn2 ghost" style={{padding: '7px', fontSize: 11.5, justifyContent:'center'}}>Paste <span className="mono" style={{color:'var(--fg-mute)', marginLeft: 4}}>⌘V</span></button>
            <button className="btn2" style={{padding: '7px', fontSize: 11.5, justifyContent:'center', gridColumn: 'span 2'}}><Icon name="layers" size={12}/> Sync to 12 selected</button>
          </div>
        </div>
        <div className="group">
          <h4>Export & Archive</h4>
          <div style={{display:'flex', flexDirection:'column', gap: 6}}>
            <button className="btn2 primary" onClick={()=>window.__openExport && window.__openExport(1, 'develop')} style={{justifyContent:'center', fontSize: 12.5}}><Icon name="download" size={13}/> Export JPG · keep original</button>
            <button className="btn2" onClick={()=>window.__openExport && window.__openExport(1, 'develop')} style={{justifyContent:'center', fontSize: 12, padding: '7px'}}><Icon name="export" size={12}/> Export + archive original</button>
            <div style={{fontFamily:'var(--mono-font)', fontSize: 10.5, color:'var(--fg-mute)', padding: '4px 2px'}}>
              D:/Halide/_edits/2026-04/ · non-destructive history
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

const DevelopScreen = ({ tab, setTab }) => {
  const p = PHOTOS[0];
  return (
    <div className="canvas editor-order-canvas">
      <div className="toolbar">
        <button className="btn" style={{padding: '5px 8px'}}><Icon name="chevL" size={13}/></button>
        <button className="btn" style={{padding: '5px 8px'}}><Icon name="chevR" size={13}/></button>
        <div className="divider"/>
        <span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>{p.filename} · RAW · {p.w}×{p.h}</span>
        <div style={{flex: 1}}/>
        <Seg value={tab} onChange={setTab} options={[
          {value:'develop', label:'Develop'},
          {value:'mask', label:'Mask'},
          {value:'prompt', label:'Prompt'},
        ]}/>
        <div className="divider"/>
        <button className="btn"><Icon name="crop" size={13}/></button>
        <button className="btn"><Icon name="eye" size={13}/> Before/After</button>
        <div className="divider"/>
        <button className="btn"><Icon name="layers" size={13}/> Copy edits</button>
        <button className="btn2 primary" onClick={()=>window.__openExport && window.__openExport(1, 'develop')}><Icon name="download" size={13}/> Export</button>
      </div>

      {tab !== 'prompt' ? (
        <div className="editor-stage">
          <div className="editor-canvas">
            <div style={{width:'min(100%, 1100px)', aspectRatio:'3/2', maxHeight:'100%', position:'relative'}}>
              <Placeholder photo={p} idx={0} showLabel={false}/>
              <div style={{position:'absolute', top:'22%', left:'40%', width:'18%', aspectRatio:'1', border:'1px dashed var(--accent)', borderRadius:'50%'}}>
                <div style={{position:'absolute', top:-18, left:0, background:'var(--accent)', color:'var(--accent-ink)', fontFamily:'var(--mono-font)', fontSize:9.5, padding:'2px 6px', borderRadius:3, whiteSpace:'nowrap', fontWeight:600}}>FACE · clean-up 55</div>
              </div>
              <div style={{position:'absolute', top:'6%', left:'10%', right:'10%', height:'22%', border:'1px dashed var(--info)'}}>
                <div style={{position:'absolute', top:-18, left:0, background:'var(--info)', color:'var(--bg)', fontFamily:'var(--mono-font)', fontSize:9.5, padding:'2px 6px', borderRadius:3, whiteSpace:'nowrap', fontWeight:600}}>SKY · enhance 72</div>
              </div>
              <div style={{position:'absolute', bottom:12, left:12, width:180, height:56, background:'rgba(0,0,0,0.55)', border:'1px solid var(--stroke)', borderRadius:6, padding:6}}>
                <svg viewBox="0 0 100 40" style={{width:'100%', height:'100%'}} preserveAspectRatio="none">
                  <path d="M0,40 L5,30 L12,20 L20,14 L30,8 L42,12 L55,18 L65,22 L72,16 L80,24 L88,30 L95,36 L100,40 Z" fill="rgba(255,255,255,0.25)"/>
                  <path d="M0,40 L6,34 L14,26 L22,22 L33,14 L45,10 L58,14 L68,18 L78,22 L86,28 L94,34 L100,40 Z" fill="rgba(136, 255, 193, 0.3)"/>
                </svg>
              </div>
            </div>
          </div>
          <div className="editor-strip">
            {PHOTOS.slice(0, 14).map((pp, i) => (
              <div key={pp.id} className={`thumb ${i===0?'active':''}`}>
                <Placeholder photo={pp} idx={i} showLabel={false}/>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div style={{flex: 1, display:'flex', flexDirection:'column', minHeight: 0}}>
          <div style={{flex: 1, display:'grid', gridTemplateColumns:'1fr 1fr', gap: 2, background:'var(--stroke)', minHeight: 0}}>
            <div style={{background:'var(--bg)', padding: 20, display:'flex', flexDirection:'column', gap: 8, minHeight: 0}}>
              <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)'}}>BEFORE · original RAW</div>
              <div style={{flex: 1, minHeight: 0}}><Placeholder photo={p} idx={0} showLabel={false}/></div>
            </div>
            <div style={{background:'var(--bg)', padding: 20, display:'flex', flexDirection:'column', gap: 8, minHeight: 0, position:'relative'}}>
              <div className="mono" style={{fontSize: 10.5, color:'var(--accent)'}}>AFTER · prompt v4 · just now</div>
              <div style={{flex: 1, minHeight: 0, position:'relative'}}>
                <Placeholder photo={{...p, hue: 220}} idx={0} showLabel={false}/>
                <div style={{position:'absolute', top: 10, right: 10}}><Chip variant="solid">AI generated</Chip></div>
              </div>
            </div>
          </div>
          <div style={{padding: 14, borderTop:'1px solid var(--stroke)', background:'var(--bg-chrome)'}}>
            <div className="prompt-box" style={{padding: 10}}>
              <div className="row" style={{color:'var(--fg-mute)', fontSize: 10.5, fontFamily:'var(--mono-font)', letterSpacing:'0.06em'}}>
                <Icon name="sparkles" size={11}/> DESCRIBE YOUR EDIT
              </div>
              <textarea defaultValue="remove the power lines in the top-right and replace the dull sky with a dramatic late-afternoon one, keep skin tones natural" style={{minHeight: 40}}/>
              <div className="row">
                <button className="btn2 ghost" style={{padding:'5px 9px', fontSize: 11.5}}><Icon name="brush" size={12}/> Mask</button>
                <Chip onClose={()=>{}}>keep faces sharp</Chip>
                <Chip onClose={()=>{}}>natural tones</Chip>
                <div style={{flex: 1}}/>
                <span className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)'}}>Strength 65</span>
                <input type="range" min="0" max="100" defaultValue="65" style={{width: 80, accentColor:'var(--accent)'}}/>
                <button className="btn2 primary" style={{padding:'6px 12px', fontSize: 12}}><Icon name="sparkles" size={12}/> Generate · ⌘↵</button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

Object.assign(window, { DevelopSidePanel, DevelopScreen, EditorInspector });
