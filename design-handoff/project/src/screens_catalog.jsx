// Unified Catalog — grid + detail (merged Library + Search + Detail)
const { useState: useS_cat, useMemo: useM_cat } = React;

// Expose focused photo via a module-level ref so app.jsx can pick the right inspector.
window.__catalogState = { focused: null };

const CatalogSidePanel = ({ albumId, setAlbumId }) => (
  <div className="sidepanel">
    <div className="head"><h3>Catalog</h3><span className="count">851,002</span></div>

    <div style={{padding: '0 10px 10px'}}>
      <div className="progress-card" style={{padding: 8}}>
        <div className="pc-head" style={{fontSize: 11.5}}><span>Cataloging</span><span className="mono" style={{color:'var(--accent)'}}>99.6%</span></div>
        <div className="progress" style={{height: 3}}><div style={{width: '99.6%'}}/></div>
        <div className="pc-stats"><span>gemma4 · scenes</span><span>~2m left</span></div>
      </div>
    </div>

    <div className="section-label"><span>Smart Albums</span><span className="ai-badge on"><Icon name="ai" size={10}/> AI</span></div>
    <div className="list">
      <button className={`item ${albumId==="all"?"active":""}`} onClick={()=>setAlbumId("all")}>
        <span className="ico"><Icon name="layers" size={14}/></span>All Photos<span className="n">851K</span>
      </button>
      {ALBUMS.filter(a=>a.tag!=="cull").slice(0, 9).map(a => (
        <button key={a.id} className={`item ${albumId===a.id?"active":""}`} onClick={()=>setAlbumId(a.id)}>
          <span className="ico"><Icon name={a.tag==="faces"||a.tag==="people"?"faces":"tag"} size={13}/></span>
          <span style={{flex:1, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{a.name}</span>
          <span className="n">{a.count > 999 ? (a.count/1000).toFixed(1)+'K' : a.count}</span>
        </button>
      ))}
    </div>

    <div className="section-label"><span>People</span><span className="ai-badge on">{PEOPLE.length}</span></div>
    <div style={{padding: '0 10px 10px'}}>
      <div style={{display:'grid', gridTemplateColumns:'repeat(6,1fr)', gap:4}}>
        {PEOPLE.map(p => (
          <div key={p.name} title={`${p.name} · ${p.count}`} style={{textAlign:'center'}}>
            <div style={{width: 28, height: 28, borderRadius:'50%', background:`oklch(0.5 0.15 ${PHOTOS[p.face].hue})`, border:'1px solid var(--stroke)'}}/>
            <div style={{fontSize: 9, color:'var(--fg-mute)', marginTop: 2, fontFamily:'var(--mono-font)'}}>{p.name}</div>
          </div>
        ))}
      </div>
    </div>

    <div className="section-label"><span>Sources</span></div>
    <div className="list">
      {SOURCES.slice(0, 7).map(s => (
        <button key={s.id} className="item">
          <span className="ico"><Icon name={s.kind} size={13}/></span>
          <span style={{flex:1, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap', fontSize: 12}}>{s.name.replace(/^.+· /, '')}</span>
          <span style={{width:6, height:6, borderRadius:'50%', background:
            s.status==='synced'?'var(--accent)':
            s.status==='syncing'?'var(--info)':
            s.status==='ready'?'var(--warn)':'var(--fg-mute)'}}/>
        </button>
      ))}
    </div>

    <div style={{marginTop:'auto', padding: 10, borderTop:'1px solid var(--stroke)'}}>
      <button className="btn2 ghost" style={{width:'100%', justifyContent:'center', fontSize: 12, padding: '7px'}}><Icon name="plus" size={12}/> New Smart Album</button>
    </div>
  </div>
);

const CatalogScreen = ({ albumId, onFocusChange }) => {
  const [q, setQ] = useS_cat("");
  const [selected, setSelected] = useS_cat(new Set([2, 5, 12]));
  const [focused, setFocused] = useS_cat(null); // photo index in focus, or null
  const searching = q.trim().length > 0;
  const photos = useM_cat(() => PHOTOS.slice(0, 48), []);
  const album = ALBUMS.find(a => a.id === albumId) || { name: 'All Photos', count: 851002, desc: 'Everything, everywhere' };

  const focus = (i) => { setFocused(i); window.__catalogState.focused = photos[i]; onFocusChange && onFocusChange(photos[i]); };
  const clearFocus = () => { setFocused(null); window.__catalogState.focused = null; onFocusChange && onFocusChange(null); };
  const toggle = (i) => {
    const s = new Set(selected);
    s.has(i) ? s.delete(i) : s.add(i);
    setSelected(s);
  };

  // ——— Focused (detail) view
  if (focused !== null) {
    const p = photos[focused];
    const prev = () => focus((focused - 1 + photos.length) % photos.length);
    const next = () => focus((focused + 1) % photos.length);
    return (
      <div className="canvas">
        <div className="toolbar">
          <button className="btn" onClick={clearFocus}><Icon name="chevL" size={13}/> Back to grid</button>
          <div className="divider"/>
          <button className="btn" onClick={prev} title="Previous"><Icon name="chevL" size={13}/></button>
          <button className="btn" onClick={next} title="Next"><Icon name="chevR" size={13}/></button>
          <div className="divider"/>
          <span className="mono" style={{fontSize: 11, color:'var(--fg-mute)'}}>{p.filename} · {p.w}×{p.h} · {p.ext} · {focused+1}/{photos.length}</span>
          <div style={{flex: 1}}/>
          <button className="btn"><Icon name="star" size={13}/> Rate</button>
          <button className="btn"><Icon name="flag" size={13}/> Flag</button>
          <div className="divider"/>
          <button className="btn"><Icon name="brush" size={13}/> Develop</button>
          <button className="btn" onClick={()=>window.__openExport && window.__openExport(1, 'detail')}><Icon name="export" size={13}/> Export</button>
          <button className="btn2 primary"><Icon name="sparkles" size={13}/> Edit with prompt</button>
        </div>
        <div className="detail-stage">
          <div className="detail-hero">
            <div style={{width:'100%', maxWidth: 1000, aspectRatio:'3/2', maxHeight:'100%'}}>
              <Placeholder photo={p} idx={focused} showLabel={false}/>
            </div>
          </div>
          <div style={{padding: '14px 24px 20px'}}>
            <div style={{display:'flex', alignItems:'baseline', gap: 16, marginBottom: 8}}>
              <div className="display" style={{fontSize: 32}}>{p.scene}</div>
              <span className="mono" style={{fontSize: 11.5, color:'var(--fg-mute)'}}>{p.date} · {p.cam}</span>
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
          {/* Filmstrip */}
          <div style={{display:'flex', gap: 4, padding: '0 24px 20px', overflowX:'auto'}}>
            {photos.map((ph, i) => (
              <div key={ph.id} onClick={()=>focus(i)}
                   style={{flex:'0 0 auto', width: 72, aspectRatio:'3/2', cursor:'pointer',
                           outline: i===focused?'2px solid var(--accent)':'1px solid var(--stroke)',
                           outlineOffset: i===focused?'-2px':'-1px', opacity: i===focused?1:0.7}}>
                <Placeholder photo={ph} idx={i} showLabel={false}/>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  // ——— Grid view
  return (
    <div className="canvas">
      <div className="toolbar">
        <div className="search" style={{flex: 1, maxWidth: 'none'}}>
          <Icon name="search" size={13}/>
          <input value={q} onChange={e=>setQ(e.target.value)} placeholder="Ask your library — 'Ari laughing outside', 'sunsets on 35mm', 'Milo in snow'…"/>
          <span className="kbd">⌘K</span>
        </div>
        <div className="divider"/>
        <button className="btn"><Icon name="grid" size={13}/></button>
        <button className="btn"><Icon name="layers" size={13}/></button>
        <div className="divider"/>
        {selected.size > 0 && (
          <>
            <span className="mono" style={{fontSize: 11, color:'var(--accent)'}}>{selected.size} selected</span>
            <button className="btn"><Icon name="brush" size={13}/> Develop</button>
            <button className="btn"><Icon name="cull" size={13}/> Cull</button>
            <button className="btn" onClick={()=>window.__openExport && window.__openExport(selected.size, 'catalog')}><Icon name="export" size={13}/> Export</button>
            <button className="btn"><Icon name="tag" size={13}/> Tag</button>
          </>
        )}
      </div>

      <div className="canvas-scroll">
        {!searching ? (
          <>
            <div className="catalog-hero">
              <div>
                <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 4, letterSpacing:'0.08em'}}>
                  {albumId === 'all' ? 'SMART ALBUM · ALL PHOTOS' : 'SMART ALBUM · AUTO-CURATED'}
                </div>
                <h1>{album.name}<em>.</em></h1>
                <div className="mono" style={{fontSize: 12, color:'var(--fg-dim)', marginTop: 6}}>{album.count.toLocaleString()} photos · {album.desc || 'Last updated 2h ago'}</div>
              </div>
              <div style={{display:'flex', gap: 6, flexWrap:'wrap'}}>
                <Chip variant="solid">Ari</Chip>
                <Chip>Backyard</Chip>
                <Chip>Golden hour</Chip>
                <Chip tone="info">CLIP 0.82+</Chip>
              </div>
            </div>

            <div className="facetbar">
              {['All', 'People', 'Places', 'Objects', 'Events', 'Colors', 'Cameras'].map(f => (
                <button key={f} className="btn" style={{border:'1px solid var(--stroke)'}}>{f}</button>
              ))}
            </div>

            <div className="libgrid">
              {photos.map((p, i) => (
                <div key={p.id} className="cell"
                     onClick={()=>toggle(i)}
                     onDoubleClick={()=>focus(i)}>
                  <Placeholder photo={p} idx={i} selected={selected.has(i)} subtle/>
                </div>
              ))}
            </div>
            <div style={{padding: '8px 18px 20px', color:'var(--fg-mute)', fontSize: 11, fontFamily:'var(--mono-font)'}}>
              Click to select · double-click to open
            </div>
          </>
        ) : (
          <div>
            <div style={{padding: '20px 20px 6px'}}>
              <div className="mono" style={{fontSize: 10.5, color:'var(--fg-mute)', marginBottom: 4, letterSpacing:'0.08em'}}>SEARCH · gemma4 + CLIP · 0.38s</div>
              <div className="display" style={{fontSize: 32}}>"{q}"<em>.</em></div>
              <div className="mono" style={{fontSize: 11.5, color:'var(--fg-dim)', marginTop: 6}}>312 results</div>
            </div>
            <div style={{padding: '6px 20px 0', display:'flex', gap:6, flexWrap:'wrap'}}>
              {SEARCH_SUGGESTIONS.slice(0, 5).map(s => <Chip key={s} onClick={()=>setQ(s)}>{s}</Chip>)}
            </div>
            <div className="libgrid">
              {photos.slice(0, 24).map((p, i) => (
                <div key={p.id} className="cell" style={{position:'relative'}}
                     onClick={()=>toggle(i)}
                     onDoubleClick={()=>focus(i)}>
                  <Placeholder photo={p} idx={i} selected={selected.has(i)} subtle/>
                  <div style={{position:'absolute', top: 6, left: 6}}><Chip variant="solid" style={{fontSize: 9.5, padding: '1px 5px'}}>{(0.95 - i*0.02).toFixed(2)}</Chip></div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

Object.assign(window, { CatalogSidePanel, CatalogScreen });
