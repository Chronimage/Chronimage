// Onboarding — multi-step flow
const { useState: useS_onb } = React;

const STEPS = [
  { id: 'welcome',  t: 'Welcome',        s: 'Choose your catalog home' },
  { id: 'sources',  t: 'Sources',        s: 'Connect every library' },
  { id: 'import',   t: 'Import',         s: 'Indexing in progress' },
  { id: 'models',   t: 'Models',         s: 'Pick your AI' },
  { id: 'people',   t: 'Name people',    s: 'So faces stick for life' },
];

const OnboardScreen = ({ appName }) => {
  const [stepIdx, setStepIdx] = useS_onb(0);
  const step = STEPS[stepIdx].id;

  return (
    <div className="canvas" style={{gridColumn: '2 / -1'}}>
      <div className="onb-wrap">
        <div className="onb-left">
          <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 14, letterSpacing:'0.1em'}}>WELCOME TO</div>
          <h1>{appName?.slice(0,-2) || "Hali"}<em>{appName?.slice(-2) || "de"}.</em></h1>
          <div className="caption">Your photos, <em style={{fontStyle:'italic', color:'var(--fg)'}}>in one light.</em><br/>On-device AI · no re-uploads · works on RAW.</div>
          <div className="onb-steps">
            {STEPS.map((s, i) => (
              <div key={s.id} className={`onb-step ${i===stepIdx?'on':''} ${i<stepIdx?'done':''}`}>
                <div className="n">{i < stepIdx ? '✓' : i + 1}</div>
                <div>
                  <div className="t">{s.t}</div>
                  <div className="s">{s.s}</div>
                </div>
              </div>
            ))}
          </div>
          <div style={{marginTop:'auto', paddingTop: 20, fontSize: 11, color:'var(--fg-mute)', fontFamily:'var(--mono-font)'}}>
            Skip setup — you can reconnect anytime.
          </div>
        </div>
        <div className="onb-right">
          {step === 'welcome' && <OnbWelcome/>}
          {step === 'sources' && <OnbSources/>}
          {step === 'import'  && <OnbImport/>}
          {step === 'models'  && <OnbModels/>}
          {step === 'people'  && <OnbPeople/>}
          <div className="onb-actions">
            <button className="btn2 ghost" onClick={()=>setStepIdx(i=>Math.max(0, i-1))} disabled={stepIdx===0} style={{opacity: stepIdx===0?0.4:1}}>
              <Icon name="chevL" size={13}/> Back
            </button>
            <div style={{display:'flex', gap: 10}}>
              {stepIdx < STEPS.length - 1 ? (
                <button className="btn2 primary" onClick={()=>setStepIdx(i=>i+1)}>Continue <Icon name="chevR" size={13}/></button>
              ) : (
                <button className="btn2 primary">Open Catalog <Icon name="chevR" size={13}/></button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

const OnbWelcome = () => (
  <div>
    <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 8}}>STEP 1 · CATALOG HOME</div>
    <h1 className="onb-title">Lift & shift<br/>your <em>whole library</em> into one place.</h1>
    <p style={{color:'var(--fg-dim)', fontSize: 13.5, lineHeight: 1.55, maxWidth: 560}}>
      Halide can unify fragmented photo libraries — D:/Photos, old exports, Dropbox archives, random SD dumps — into a single catalog. Originals stay untouched; a manifest makes them browseable, movable, and restorable in one shot.
    </p>

    <div style={{marginTop: 24, display:'grid', gridTemplateColumns:'1fr 1fr', gap: 12}}>
      <button className="preset-card on">
        <div style={{width: 42, height: 42, borderRadius: 8, background:'color-mix(in oklch, var(--accent) 25%, var(--bg))', display:'flex', alignItems:'center', justifyContent:'center', color:'var(--accent)'}}><Icon name="disk" size={18}/></div>
        <div><div className="name">Consolidate into D:/Halide</div><div className="sub">Copy originals · 2.4 TB free of 4 TB</div></div>
        <Chip variant="solid">Recommended</Chip>
      </button>
      <button className="preset-card">
        <div style={{width: 42, height: 42, borderRadius: 8, background:'var(--bg)', display:'flex', alignItems:'center', justifyContent:'center', color:'var(--fg-dim)'}}><Icon name="layers" size={18}/></div>
        <div><div className="name">Index in place</div><div className="sub">Read-only · no files move</div></div>
      </button>
    </div>

    <div style={{marginTop: 18, padding: 14, background:'var(--bg-elev)', border:'1px solid var(--stroke)', borderRadius: 10}}>
      <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 6}}>CATALOG LOCATION</div>
      <div style={{fontFamily: 'var(--mono-font)', fontSize: 13}}>D:/Halide/</div>
      <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginTop: 10, display:'flex', gap: 24}}>
        <span>Est. move: 1.8 TB · 6,412 folders</span>
        <span>ETA: 4h 12m on this disk</span>
        <span style={{color:'var(--accent)'}}>Bit-exact · checksummed</span>
      </div>
    </div>
  </div>
);

const OnbSources = () => {
  const connected = SOURCES.filter(s => s.kind === 'iphone' || s.kind === 'android' || s.kind === 'card');
  const rest = SOURCES.filter(s => !(s.kind === 'iphone' || s.kind === 'android' || s.kind === 'card')).slice(0, 5);
  return (
    <div>
      <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 8}}>STEP 2 · SOURCES</div>
      <h1 className="onb-title">Connect <em>everywhere</em> your photos live.</h1>

      {connected.length > 0 && (
        <div style={{marginTop: 16, padding: 12, border:'1px solid var(--accent)', borderRadius: 10,
                     background:'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))'}}>
          <div className="mono" style={{fontSize: 10.5, color:'var(--accent)', marginBottom: 8, letterSpacing:'0.08em'}}>
            <Icon name="usb" size={11}/> CONNECTED NOW · USB
          </div>
          <div style={{display:'flex', flexDirection:'column', gap: 6}}>
            {connected.map(s => (
              <div key={s.id} className="source-row" style={{padding: '10px 12px', marginBottom: 0, background:'var(--bg)'}}>
                <div className="ico"><Icon name={s.kind} size={18}/></div>
                <div style={{flex: 1}}>
                  <div className="name">{s.name}</div>
                  <div className="sub">{s.count} photos · {s.sub}</div>
                </div>
                <button className="btn2 primary" style={{padding:'6px 12px'}}>Import new</button>
              </div>
            ))}
          </div>
          <div style={{fontSize: 11, color:'var(--fg-mute)', marginTop: 8, fontFamily:'var(--mono-font)'}}>
            Trust prompt on device required · HEIC / RAW / Live Photos preserved
          </div>
        </div>
      )}

      <div style={{display:'flex', flexDirection:'column', gap: 6, marginTop: 14}}>
        {rest.map(s => (
          <div key={s.id} className="source-row" style={{padding: '10px 12px', marginBottom: 0}}>
            <div className="ico"><Icon name={s.kind} size={18}/></div>
            <div style={{flex: 1}}>
              <div className="name">{s.name}</div>
              <div className="sub">{s.count} photos · {s.sub}</div>
            </div>
            <Chip variant={s.status==='ready'?'solid':''} tone={s.status==='syncing'?'info':s.status==='paused'?'warn':''}>
              {s.status === 'synced' ? 'Connected' : s.status === 'syncing' ? 'Syncing' : s.status === 'paused' ? 'Paused' : s.status === 'ready' ? 'Ready' : 'Idle'}
            </Chip>
          </div>
        ))}
      </div>
    </div>
  );
};

const OnbImport = () => (
  <div>
    <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 8}}>STEP 3 · IMPORT</div>
    <h1 className="onb-title">Indexing <em>847,291</em> photos.</h1>
    <p style={{color:'var(--fg-dim)', fontSize: 13, maxWidth: 560}}>Faces, scenes, OCR, duplicates, and CLIP embeddings. You can keep setting things up — we'll keep chewing in the background.</p>

    <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 10, marginTop: 18}}>
      {[
        {t:'D:/Photos',          c:'212,481', pct:100, d:'Done · 2h 14m'},
        {t:'Google Photos',      c:'96,312',  pct:62,  d:'Syncing · 3m remaining'},
        {t:'OneDrive · Camera Roll', c:'14,880', pct:100, d:'Done · 4m'},
        {t:'NAS · \\\\synology', c:'402,009', pct:88,  d:'Incremental · 18m'},
        {t:'SD · Sony A7 IV',    c:'214',     pct:0,   d:'Waiting'},
        {t:'Dropbox · Archive',  c:'62,140',  pct:41,  d:'Syncing · 14m remaining'},
      ].map(s => (
        <div key={s.t} className="progress-card">
          <div className="pc-head"><span>{s.t}</span><span className="mono" style={{color: s.pct===100?'var(--accent)':'var(--fg-dim)'}}>{s.pct}%</span></div>
          <div className="progress"><div style={{width: s.pct + '%', background: s.pct===100?'var(--accent)':'var(--info)'}}/></div>
          <div className="pc-stats"><span>{s.c} photos</span><span>{s.d}</span></div>
        </div>
      ))}
    </div>

    <div style={{marginTop: 18, padding: 14, background:'var(--bg-elev)', border:'1px solid var(--stroke)', borderRadius: 10, display:'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 14}}>
      <div><div className="display" style={{fontSize: 28}}>847K</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>INDEXED</div></div>
      <div><div className="display" style={{fontSize: 28}}>487</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>DUPLICATES FOUND</div></div>
      <div><div className="display" style={{fontSize: 28}}>58</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>FACES CLUSTERED</div></div>
      <div><div className="display" style={{fontSize: 28, color:'var(--accent)'}}>18m</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>TO COMPLETION</div></div>
    </div>
  </div>
);

const OnbModels = () => (
  <div>
    <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 8}}>STEP 4 · MODELS</div>
    <h1 className="onb-title">Pick the models<br/>that <em>read your photos.</em></h1>
    <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 12, marginTop: 18}}>
      {[
        {t:'Cataloging & scenes', m:'gemma4-27b', alt:'gemma4-9b · LLaVA-1.6', pick:'gemma4-27b', sub:'17.3 GB · VRAM 12GB · best quality'},
        {t:'Semantic search',     m:'CLIP-L14',   alt:'SigLIP',               pick:'CLIP-L14',   sub:'1.4 GB · fastest recall'},
        {t:'Face recognition',    m:'ArcFace R100', alt:'InsightFace',       pick:'ArcFace',    sub:'Local encrypted DB · 58 people'},
        {t:'Prompt / generative', m:'Flux-dev',   alt:'SDXL inpaint · Cloud', pick:'Flux-dev',   sub:'12 GB · on-device inpainting'},
      ].map(r => (
        <div key={r.t} style={{padding: 14, border: '1px solid var(--stroke)', borderRadius: 10, background: 'var(--bg-elev)'}}>
          <div className="mono" style={{fontSize: 10, color:'var(--fg-mute)', marginBottom: 6}}>{r.t.toUpperCase()}</div>
          <div style={{fontSize: 16, fontFamily:'var(--display-font)', marginBottom: 4}}>{r.m}</div>
          <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 10}}>{r.sub}</div>
          <div style={{fontSize: 11, color:'var(--fg-dim)'}}>Alternates · {r.alt}</div>
        </div>
      ))}
    </div>
    <div style={{marginTop: 18, padding: '12px 14px', background:'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))', border: '1px solid color-mix(in oklch, var(--accent) 30%, var(--stroke))', borderRadius: 10, display: 'flex', alignItems:'center', gap: 12}}>
      <Icon name="ai" size={20}/>
      <div style={{flex: 1, fontSize: 12.5, color:'var(--fg-dim)'}}>
        <strong style={{color:'var(--fg)'}}>All on-device.</strong> Nothing leaves your PC unless you pick a cloud model. You can swap models later without re-indexing.
      </div>
    </div>
  </div>
);

const OnbPeople = () => {
  const [names, setNames] = useS_onb({});
  const people = [
    {id: 0, ct: 3240, faces: [0, 1, 2]},
    {id: 1, ct: 1922, faces: [6, 7, 8]},
    {id: 2, ct: 982, faces: [23, 14, 11]},
    {id: 3, ct: 611, faces: [1, 5, 16]},
    {id: 4, ct: 711, faces: [11, 2, 23]},
    {id: 5, ct: 587, faces: [5, 16, 22]},
    {id: 6, ct: 244, faces: [16, 17, 19]},
    {id: 7, ct: 189, faces: [8, 13, 18]},
  ];
  return (
    <div>
      <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', marginBottom: 8}}>STEP 5 · NAME PEOPLE</div>
      <h1 className="onb-title">Name them once.<br/><em>Forever categorised</em> — even for photos you import tomorrow.</h1>
      <p style={{color:'var(--fg-dim)', fontSize: 13, maxWidth: 560, marginBottom: 20}}>
        Halide clustered 58 distinct faces. Name the ones you care about — the rest stay unnamed and private. New photos auto-assign as they arrive.
      </p>
      <div className="person-grid">
        {people.map(p => (
          <div key={p.id} className="person-card">
            <div className="faces">
              {p.faces.map((f, i) => (
                <div key={i} className="face" style={{background: `oklch(0.55 0.15 ${PHOTOS[f].hue})`, backgroundImage: 'repeating-linear-gradient(-45deg, transparent 0 4px, rgba(255,255,255,0.08) 4px 5px)'}}/>
              ))}
            </div>
            <div className="meta">
              <input placeholder={`Unnamed · cluster ${p.id + 1}`} value={names[p.id] || ''} onChange={e=>setNames({...names, [p.id]: e.target.value})}/>
            </div>
            <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', display:'flex', justifyContent:'space-between'}}>
              <span>{p.ct.toLocaleString()} photos</span>
              <button style={{color:'var(--fg-mute)'}}>Merge…</button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

Object.assign(window, { OnboardScreen });
