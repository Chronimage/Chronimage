// Small shared primitives
const Chip = ({ children, tone, variant, onClose, onClick, style }) => (
  <span className={`chip ${tone||""} ${variant||""}`} onClick={onClick} style={{cursor: onClick?"pointer":undefined, ...style}}>
    {(tone || variant==="solid") && <span className="dot" />}
    {children}
    {onClose && <span className="x" onClick={(e)=>{e.stopPropagation(); onClose();}}>×</span>}
  </span>
);

const Seg = ({ value, onChange, options }) => (
  <div className="seg">
    {options.map(o => (
      <button key={o.value} className={value===o.value?"on":""} onClick={()=>onChange(o.value)}>{o.label}</button>
    ))}
  </div>
);

const Slider = ({ label, value, onChange, min=-100, max=100, step=1, suffix="" }) => (
  <div className="slider-row">
    <div className="lbl">{label}</div>
    <input type="range" min={min} max={max} step={step} value={value} onChange={e=>onChange(+e.target.value)} />
    <div className="val">{value > 0 ? "+" + value : value}{suffix}</div>
  </div>
);

const Toggle = ({ on, onChange, label }) => (
  <button onClick={()=>onChange(!on)} style={{
    display:'inline-flex', alignItems:'center', gap: 8
  }}>
    <span style={{
      width: 32, height: 18, borderRadius: 999,
      background: on ? 'var(--accent)' : 'var(--bg-elev)',
      border: '1px solid var(--stroke)',
      position: 'relative', transition: 'all 0.15s'
    }}>
      <span style={{
        position: 'absolute', top: 1, left: on ? 15 : 1,
        width: 14, height: 14, borderRadius: '50%',
        background: on ? 'var(--accent-ink)' : 'var(--fg-dim)',
        transition: 'left 0.15s'
      }}/>
    </span>
    {label && <span style={{fontSize: 12, color:'var(--fg-dim)'}}>{label}</span>}
  </button>
);

Object.assign(window, { Chip, Seg, Slider, Toggle });
