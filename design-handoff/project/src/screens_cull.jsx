// Cull review screen with multiple modes
const { useState: useS_cull } = React;

const CullSidePanel = ({ mode, setMode, idx, total }) => (
  <div className="sidepanel">
    <div className="head">
      <h3>Cull Queue</h3>
      <span className="count">{total - idx} left</span>
    </div>
    <div style={{padding: '4px 16px 14px'}}>
      <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 8}}>SESSION PROGRESS</div>
      <div className="progress" style={{height: 4}}><div style={{width: `${(idx/total)*100}%`}}/></div>
      <div className="mono" style={{fontSize: 10.5, color:'var(--fg-dim)', marginTop: 6, display:'flex', justifyContent:'space-between'}}>
        <span>{idx}/{total} reviewed</span><span>~{Math.max(1, (total-idx)*0.4).toFixed(0)} min left</span>
      </div>
    </div>

    <div className="section-label"><span>Mode</span></div>
    <div style={{padding: '0 12px 12px'}}>
      <div className="tseg three" style={{background:'var(--bg-elev)', padding:3, borderRadius:9, display:'grid', gridTemplateColumns:'repeat(3,1fr)', gap:3}}>
        {[{v:'compare',l:'Compare'},{v:'grid',l:'Grid'},{v:'swipe',l:'Swipe'}].map(o => (
          <button key={o.v} onClick={()=>setMode(o.v)} style={{padding:6, borderRadius:6, fontSize:11, color: mode===o.v?'var(--fg)':'var(--fg-dim)', background: mode===o.v?'var(--bg)':'transparent'}}>{o.l}</button>
        ))}
      </div>
    </div>

    <div className="section-label"><span>Filter issues</span></div>
    <div className="list">
      {[
        {n:'Near-duplicates', c: 184, on: true},
        {n:'Out of focus',    c: 213, on: true},
        {n:'Eyes closed',     c: 94,  on: true},
        {n:'Over/under exp.', c: 71,  on: false},
        {n:'Screenshots',     c: 2841,on: false},
        {n:'Low-res / web',   c: 412, on: false},
      ].map(i => (
        <button key={i.n} className={`item ${i.on?'active':''}`}>
          <span className="ico"><Icon name="flag" size={14}/></span>
          <span>{i.n}</span>
          <span className="n">{i.c}</span>
        </button>
      ))}
    </div>

    <div style={{marginTop:'auto', padding:14, borderTop:'1px solid var(--stroke)'}}>
      <div className="mono" style={{fontSize:10.5, color:'var(--fg-mute)', marginBottom:8}}>SESSION SUMMARY</div>
      <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 10}}>
        <div><div className="display" style={{fontSize: 28}}>184</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>KEPT</div></div>
        <div><div className="display" style={{fontSize: 28, color:'var(--warn)'}}>312</div><div className="mono" style={{fontSize: 10, color:'var(--fg-mute)'}}>REJECTED</div></div>
      </div>
      <button className="btn2 primary" style={{width:'100%', marginTop: 12, justifyContent:'center'}}>Review rejects before deleting</button>
    </div>
  </div>
);

const CullCompare = ({ pair }) => {
  const a = PHOTOS[pair.ids[0]], b = PHOTOS[pair.ids[1]];
  return (
    <div className="cull-stage">
      {[{p:a, issues: pair.issues_a, winner: pair.keep===0, loser: pair.keep===1},
        {p:b, issues: pair.issues_b, winner: pair.keep===1, loser: pair.keep===0}].map((c, i) => (
        <div key={i} className={`cull-card ${c.winner?'winner':''} ${c.loser?'loser':''}`}>
          <div className="label">
            <span>{c.p.filename}</span>
            <span>{c.p.cam} · {c.p.mm}mm · f/{c.p.ap} · {c.p.shut} · ISO {c.p.iso}</span>
          </div>
          <div className="frame"><Placeholder photo={c.p} idx={pair.ids[i]} subtle showLabel={false}/></div>
          <div className="tag-row">
            {c.winner && <Chip variant="solid">AI pick · keep</Chip>}
            {c.loser && <Chip tone="warn">Suggested reject</Chip>}
            {c.issues.map(is => <Chip key={is} tone="warn">{is}</Chip>)}
            <span className="mono" style={{marginLeft:'auto', fontSize: 11, color:'var(--fg-mute)'}}>sharpness {(0.6 + i*0.3).toFixed(2)} · eyes-open {(0.95 - i*0.5).toFixed(2)}</span>
          </div>
        </div>
      ))}
    </div>
  );
};

const CullGrid = () => {
  const photos = PHOTOS.slice(0, 18);
  return (
    <div style={{padding: 20, flex: 1, overflow:'auto'}}>
      <div style={{display:'grid', gridTemplateColumns:'repeat(4, 1fr)', gap: 10}}>
        {photos.map((p, i) => {
          const rej = i % 3 === 0;
          return (
            <div key={p.id} style={{position:'relative'}}>
              <div style={{aspectRatio:'4/3'}}>
                <Placeholder photo={p} idx={i} rejected={rej} keep={!rej && i%5===0} subtle/>
              </div>
              <div style={{position:'absolute', top: 8, left: 8, display:'flex', gap: 4, flexWrap:'wrap', maxWidth:'calc(100% - 40px)'}}>
                {i%3===0 && <Chip tone="warn">duplicate</Chip>}
                {i%4===1 && <Chip tone="warn">blur</Chip>}
                {i%7===2 && <Chip tone="warn">eyes closed</Chip>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const CullSwipe = ({ pair }) => {
  const p = PHOTOS[pair.ids[0]];
  return (
    <div style={{flex:1, display:'flex', alignItems:'center', justifyContent:'center', padding: 40, position:'relative'}}>
      <div style={{width: 'min(640px, 80%)', aspectRatio:'3/4', position:'relative', transform:'rotate(-3deg)'}}>
        <Placeholder photo={p} idx={pair.ids[0]} showLabel={false}/>
        <div style={{position:'absolute', inset:0, border:'1px solid var(--stroke)', borderRadius: 6, pointerEvents:'none'}}/>
        <div style={{position:'absolute', top: 20, left: 20, padding: '6px 12px', background:'var(--warn)', color:'var(--bg)', fontFamily:'var(--mono-font)', fontSize: 12, letterSpacing:'0.08em', textTransform:'uppercase', fontWeight:600, borderRadius: 4, transform:'rotate(-6deg)'}}>REJECT</div>
      </div>
      <div style={{position:'absolute', bottom: 40, left: 0, right:0, textAlign:'center', fontFamily:'var(--mono-font)', fontSize: 12, color:'var(--fg-mute)'}}>
        Drag left to reject · drag right to keep · space to skip
      </div>
    </div>
  );
};

const CullScreen = () => {
  const [mode, setMode] = useS_cull(window.__TWEAKS__.cullMode || 'compare');
  const [idx, setIdx] = useS_cull(2);
  const pair = CULL_PAIRS[idx % CULL_PAIRS.length];
  return (
    <div className="canvas">
      <div className="toolbar">
        <div>
          <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>CULL · {pair.reason.toUpperCase()}</div>
          <div style={{fontSize: 14, marginTop: 2}}>Similarity <span className="mono" style={{color:'var(--accent)'}}>{(pair.similarity*100).toFixed(0)}%</span> · pair {idx+1} of {CULL_PAIRS.length * 40}</div>
        </div>
        <div style={{flex: 1}}/>
        <Seg value={mode} onChange={setMode} options={[
          {value:'compare', label:'Compare'},
          {value:'grid',    label:'Grid'},
          {value:'swipe',   label:'Swipe'},
        ]}/>
        <div className="divider"/>
        <button className="btn"><Icon name="chevL" size={14}/> Prev</button>
        <button className="btn">Next <Icon name="chevR" size={14}/></button>
      </div>
      {mode==='compare' && <CullCompare pair={pair}/>}
      {mode==='grid'    && <CullGrid/>}
      {mode==='swipe'   && <CullSwipe pair={pair}/>}
      <div className="cull-filmstrip">
        {PHOTOS.slice(0, 24).map((p, i) => (
          <div key={p.id} className={`thumb ${i===idx*2?'active':''} ${i<idx*2?'done':''}`}>
            <Placeholder photo={p} idx={i} showLabel={false}/>
          </div>
        ))}
      </div>
      <div className="cull-verdict">
        <button className="btn2 danger"><Icon name="reject" size={14}/> Reject both <span className="kbd">⌃R</span></button>
        <button className="btn2"><Icon name="reject" size={14}/> Reject A <span className="kbd">A</span></button>
        <button className="btn2"><Icon name="reject" size={14}/> Reject B <span className="kbd">B</span></button>
        <div style={{flex: 1}}/>
        <button className="btn2 primary" onClick={()=>setIdx(i=>i+1)}><Icon name="keep" size={14}/> Accept AI verdict <span className="kbd" style={{color:'var(--accent-ink)', borderColor:'transparent', background:'color-mix(in oklch, var(--accent-ink) 15%, transparent)'}}>↵</span></button>
      </div>
    </div>
  );
};

Object.assign(window, { CullScreen, CullSidePanel });
