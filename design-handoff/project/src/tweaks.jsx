// Tweaks panel
const { useState: useS_tw, useEffect: useE_tw } = React;

const TweaksPanel = ({ open, onClose, tweaks, setTweaks }) => {
  if (!open) return null;
  const set = (k, v) => setTweaks({...tweaks, [k]: v});
  return (
    <div className="tweaks">
      <div className="thead">
        <h4>Tweaks<em>.</em></h4>
        <button onClick={onClose} style={{color:'var(--fg-mute)'}}><Icon name="close" size={14}/></button>
      </div>
      <div className="tbody">
        <div className="tgroup">
          <div className="tlabel">Theme</div>
          <div className="tseg"><button className={tweaks.theme==='dark'?'on':''} onClick={()=>set('theme','dark')}>Dark</button><button className={tweaks.theme==='light'?'on':''} onClick={()=>set('theme','light')}>Light</button></div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Accent</div>
          <div className="swatches">
            {[
              {k:'mint',   c:'oklch(0.88 0.18 150)'},
              {k:'ember',  c:'oklch(0.78 0.17 40)'},
              {k:'violet', c:'oklch(0.78 0.17 295)'},
              {k:'sky',    c:'oklch(0.82 0.14 230)'},
              {k:'gold',   c:'oklch(0.87 0.17 85)'},
            ].map(s => (
              <div key={s.k} className={`sw ${tweaks.accent===s.k?'on':''}`} style={{background: s.c}} onClick={()=>set('accent', s.k)}/>
            ))}
          </div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Display typeface</div>
          <div className="tseg three">
            {[
              {v:'Instrument Serif', l:'Instrument'},
              {v:'Fraunces', l:'Fraunces'},
              {v:'Inter Tight', l:'Inter'},
            ].map(o => <button key={o.v} className={tweaks.displayFont===o.v?'on':''} onClick={()=>set('displayFont', o.v)}>{o.l}</button>)}
          </div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Library · grid density</div>
          <div className="tseg three">
            {[{v:'compact',l:'Compact'},{v:'comfortable',l:'Default'},{v:'spacious',l:'Spacious'}].map(o => (
              <button key={o.v} className={tweaks.gridDensity===o.v?'on':''} onClick={()=>set('gridDensity', o.v)}>{o.l}</button>
            ))}
          </div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Library · facet placement</div>
          <div className="tseg">
            {[{v:'left',l:'Left panel'},{v:'bottom',l:'Top bar'}].map(o => (
              <button key={o.v} className={tweaks.facetPlacement===o.v?'on':''} onClick={()=>set('facetPlacement', o.v)}>{o.l}</button>
            ))}
          </div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Cull interaction</div>
          <div className="tseg three">
            {[{v:'compare',l:'Compare'},{v:'grid',l:'Grid'},{v:'swipe',l:'Swipe'}].map(o => (
              <button key={o.v} className={tweaks.cullMode===o.v?'on':''} onClick={()=>set('cullMode', o.v)}>{o.l}</button>
            ))}
          </div>
        </div>
        <div className="tgroup">
          <div className="tlabel">Editor layout</div>
          <div className="tseg">
            {[{v:'right-panel',l:'Panel right'},{v:'left-panel',l:'Panel left'}].map(o => (
              <button key={o.v} className={tweaks.editorLayout===o.v?'on':''} onClick={()=>set('editorLayout', o.v)}>{o.l}</button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};

Object.assign(window, { TweaksPanel });
