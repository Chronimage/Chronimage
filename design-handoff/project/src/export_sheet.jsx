// Export sheet — a modal overlay surfaced from Catalog / Develop / Cull
const { useState: useS_ex } = React;

const ExportSheet = ({ open, onClose, count = 42, context = 'catalog' }) => {
  const [fmt, setFmt] = useS_ex('jpg');
  const [color, setColor] = useS_ex('srgb');
  const [quality, setQuality] = useS_ex(88);
  const [longEdge, setLongEdge] = useS_ex(4000);
  const [strip, setStrip] = useS_ex(true);
  const [watermark, setWatermark] = useS_ex(false);
  const [upload, setUpload] = useS_ex(true);
  const [archive, setArchive] = useS_ex(true);
  const [copyEdits, setCopyEdits] = useS_ex(false);
  const [autoLight, setAutoLight] = useS_ex(false);

  if (!open) return null;

  const queue = PHOTOS.slice(0, 5).map((p, i) => ({
    p, i, status: i < 1 ? 'done' : i < 3 ? 'running' : 'queued',
    prog: i < 1 ? 100 : i === 1 ? 72 : i === 2 ? 34 : 0,
    op: i === 0 ? `Develop · Ari preset → ${fmt.toUpperCase()}` :
        i === 1 ? `Upscale 2× → ${fmt.toUpperCase()}` :
        i === 2 ? `Denoise → ${fmt.toUpperCase()}` :
        `Export ${fmt.toUpperCase()}`,
  }));

  return (
    <div onClick={onClose} style={{position:'fixed', inset: 0, background:'color-mix(in oklch, var(--bg) 78%, black)', backdropFilter:'blur(4px)', zIndex: 50, display:'flex', alignItems:'center', justifyContent:'center'}}>
      <div onClick={e=>e.stopPropagation()}
           style={{width: 'min(1100px, 92vw)', height: 'min(680px, 88vh)', background:'var(--bg-elev)', border:'1px solid var(--stroke)', borderRadius: 14, display:'grid', gridTemplateColumns:'1.3fr 1fr', overflow:'hidden'}}>
        {/* LEFT — queue */}
        <div style={{display:'flex', flexDirection:'column', borderRight:'1px solid var(--stroke)'}}>
          <div style={{padding:'16px 20px', borderBottom:'1px solid var(--stroke)', display:'flex', alignItems:'center', gap: 10}}>
            <div style={{flex: 1}}>
              <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', letterSpacing:'0.08em'}}>EXPORT QUEUE · FROM {context.toUpperCase()}</div>
              <div className="display" style={{fontSize: 24, marginTop: 2}}>{count} items<em>.</em></div>
            </div>
            <button className="btn2 ghost" onClick={onClose}><Icon name="close" size={12}/> Close</button>
          </div>
          <div style={{flex: 1, overflow:'auto', padding: 14}}>
            {queue.map(q => (
              <div key={q.i} className="queue-item" style={{marginBottom: 8}}>
                <Placeholder photo={q.p} idx={q.i} showLabel={false}/>
                <div>
                  <div className="name">{q.p.filename}</div>
                  <div className="sub">{q.op}</div>
                  <div className="progress"><div style={{width: q.prog + '%', background: q.status==='done' ? 'var(--accent)' : q.status==='running' ? 'var(--info)' : 'var(--stroke-strong)'}}/></div>
                </div>
                <div style={{textAlign:'right'}}>
                  <div className="mono" style={{fontSize: 11, color: q.status==='done' ? 'var(--accent)' : 'var(--fg-dim)'}}>
                    {q.status === 'done' ? 'Done' : q.status === 'running' ? q.prog + '%' : 'Queued'}
                  </div>
                </div>
              </div>
            ))}
            <div style={{display:'flex', gap: 8, marginTop: 14}}>
              <div style={{flex: 1, padding: 10, border: '1px solid var(--stroke)', borderRadius: 10, background: 'var(--bg)'}}>
                <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)'}}>GPU</div>
                <div className="display" style={{fontSize: 22}}>73<span style={{fontSize: 12}}>%</span></div>
              </div>
              <div style={{flex: 1, padding: 10, border: '1px solid var(--stroke)', borderRadius: 10, background: 'var(--bg)'}}>
                <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)'}}>ETA</div>
                <div className="display" style={{fontSize: 22}}>6<span style={{fontSize: 12}}>m 12s</span></div>
              </div>
              <div style={{flex: 1, padding: 10, border: '1px solid var(--stroke)', borderRadius: 10, background: 'var(--bg)'}}>
                <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)'}}>OUTPUT</div>
                <div className="display" style={{fontSize: 22}}>2.1<span style={{fontSize: 12}}>GB</span></div>
              </div>
            </div>
          </div>
        </div>

        {/* RIGHT — preset */}
        <div style={{display:'flex', flexDirection:'column'}}>
          <div style={{padding:'16px 20px', borderBottom:'1px solid var(--stroke)'}}>
            <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', letterSpacing:'0.08em'}}>EXPORT PRESET</div>
            <div className="display" style={{fontSize: 20, marginTop: 2}}>Web · sRGB<em>.</em></div>
          </div>
          <div style={{flex: 1, overflow:'auto', padding: 16, display:'flex', flexDirection:'column', gap: 12}}>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 10}}>
              <div><div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 6}}>FORMAT</div>
                <Seg value={fmt} onChange={setFmt} options={[{value:'jpg',label:'JPEG'},{value:'heic',label:'HEIC'},{value:'tiff',label:'TIFF'}]}/></div>
              <div><div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 6}}>COLOR</div>
                <Seg value={color} onChange={setColor} options={[{value:'srgb',label:'sRGB'},{value:'p3',label:'P3'},{value:'raw',label:'AdobeRGB'}]}/></div>
            </div>
            <Slider label="Quality" value={quality} onChange={setQuality} min={0} max={100} suffix=""/>
            <Slider label="Long edge" value={longEdge} onChange={setLongEdge} min={800} max={8000} step={200} suffix=" px"/>

            <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginTop: 6, letterSpacing:'0.08em'}}>PROCESSING</div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Archive originals alongside export</span>
              <Toggle on={archive} onChange={setArchive}/>
            </div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Copy-paste edits from last developed photo</span>
              <Toggle on={copyEdits} onChange={setCopyEdits}/>
            </div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Auto-light adjustments per photo</span>
              <Toggle on={autoLight} onChange={setAutoLight}/>
            </div>

            <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginTop: 6, letterSpacing:'0.08em'}}>METADATA & DESTINATION</div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Strip GPS & metadata</span>
              <Toggle on={strip} onChange={setStrip}/>
            </div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Watermark</span>
              <Toggle on={watermark} onChange={setWatermark}/>
            </div>
            <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
              <span style={{fontSize: 12.5, color:'var(--fg-dim)'}}>Also upload to Google Photos</span>
              <Toggle on={upload} onChange={setUpload}/>
            </div>
            <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginTop: 4, paddingTop: 8, borderTop:'1px dashed var(--stroke)'}}>
              D:/Photos/_exports/2026-04-19/
            </div>
          </div>
          <div style={{padding: 14, borderTop:'1px solid var(--stroke)', display:'flex', gap: 8, justifyContent:'flex-end'}}>
            <button className="btn2" onClick={onClose}>Cancel</button>
            <button className="btn2 primary"><Icon name="download" size={13}/> Start {count} tasks</button>
          </div>
        </div>
      </div>
    </div>
  );
};

Object.assign(window, { ExportSheet });
