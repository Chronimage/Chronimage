// Cull Bin — trash/recover
const { useState: useS_cb } = React;

const CullBinSidePanel = () => (
  <div className="sidepanel">
    <div className="head"><h3>Cull Bin</h3><span className="count">312 items</span></div>
    <div style={{padding:'4px 14px 14px'}}>
      <div className="mono" style={{fontSize:10.5, color:'var(--fg-mute)', marginBottom:8}}>RECLAIMABLE</div>
      <div className="display" style={{fontSize: 40}}>8.2<span style={{fontSize:16}}>GB</span></div>
      <div className="mono" style={{fontSize:10.5, color:'var(--fg-mute)', marginTop: 4}}>Originals preserved · metadata intact</div>
    </div>
    <div className="section-label"><span>Filter</span></div>
    <div className="list">
      {[{n:'All rejects',c:312,on:true},{n:'Near-duplicates',c:184},{n:'Out of focus',c:71},{n:'Eyes closed',c:34},{n:'Screenshots',c:23}].map(f=>(
        <button key={f.n} className={`item ${f.on?'active':''}`}>
          <span className="ico"><Icon name="flag" size={13}/></span><span>{f.n}</span><span className="n">{f.c}</span>
        </button>
      ))}
    </div>
    <div className="section-label"><span>Retention</span></div>
    <div style={{padding:'0 14px 14px', fontSize:12, color:'var(--fg-dim)'}}>
      Auto-empty after <span className="mono" style={{color:'var(--accent)'}}>30 days</span>. Nothing leaves your disk without confirmation.
    </div>
    <div style={{marginTop:'auto', padding:12, borderTop:'1px solid var(--stroke)', display:'flex', flexDirection:'column', gap:6}}>
      <button className="btn2" style={{width:'100%', justifyContent:'center'}}>Restore all to catalog</button>
      <button className="btn2 danger" style={{width:'100%', justifyContent:'center'}}><Icon name="reject" size={13}/> Empty bin permanently</button>
    </div>
  </div>
);

const CullBinScreen = () => {
  const [sel, setSel] = useS_cb(new Set([1, 3]));
  const items = PHOTOS.slice(0, 14).map((p, i) => ({ p, i,
    reason: ['Near-duplicate of IMG_4067','Sharpness 0.18','Eyes closed','Burst 4/6','Screenshot','Low-res web export','Near-duplicate of IMG_4088'][i%7],
    when: ['12m ago','34m ago','2h ago','4h ago','yesterday','2 days ago','3 days ago'][i%7],
  }));
  return (
    <div className="canvas">
      <div className="toolbar">
        <div>
          <div className="mono" style={{fontSize:11, color:'var(--fg-mute)'}}>CULL BIN · RECOVERABLE</div>
          <div style={{fontSize:14, marginTop:2}}>312 items · 8.2 GB · kept until you confirm</div>
        </div>
        <div style={{flex:1}}/>
        {sel.size > 0 && <>
          <span className="mono" style={{fontSize:11, color:'var(--accent)'}}>{sel.size} selected</span>
          <button className="btn"><Icon name="keep" size={13}/> Restore</button>
          <button className="btn2 danger" style={{padding:'6px 10px'}}><Icon name="reject" size={13}/> Delete forever</button>
        </>}
        <div className="divider"/>
        <button className="btn2 primary"><Icon name="reject" size={13}/> Empty bin</button>
      </div>
      <div className="canvas-scroll" style={{padding:18}}>
        {items.map(it => {
          const isSel = sel.has(it.i);
          return (
            <div key={it.i} className="cullbin-row" onClick={()=>{
              const s = new Set(sel); s.has(it.i)?s.delete(it.i):s.add(it.i); setSel(s);
            }} style={{cursor:'pointer', borderColor: isSel?'var(--accent)':'var(--stroke)'}}>
              <div style={{width:64, height:48}}><Placeholder photo={it.p} idx={it.i} showLabel={false}/></div>
              <div>
                <div style={{fontFamily:'var(--mono-font)', fontSize:12}}>{it.p.filename}</div>
                <div style={{fontSize:11, color:'var(--fg-mute)', marginTop:3, display:'flex', gap:10}}>
                  <Chip tone="warn">{it.reason}</Chip>
                  <span className="mono">Rejected {it.when}</span>
                </div>
              </div>
              <div className="mono" style={{fontSize:11, color:'var(--fg-dim)'}}>{it.p.sizeMb} MB</div>
              <div style={{display:'flex', gap:6}}>
                <button className="btn2 ghost" style={{padding:'6px 10px', fontSize:11.5}}>Restore</button>
                <button className="btn2 ghost" style={{padding:'6px 10px', fontSize:11.5, color:'var(--warn)'}}>Delete</button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

Object.assign(window, { CullBinScreen, CullBinSidePanel });
