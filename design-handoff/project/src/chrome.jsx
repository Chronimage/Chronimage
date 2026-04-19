// Titlebar + Rail + Status bar
const { useState } = React;

const Titlebar = ({ screen, appName = "Halide" }) => {
  const brand = appName;
  const head = brand.slice(0, -2);
  const tail = brand.slice(-2);
  return (
    <div className="titlebar">
      <div className="brand">{head}<em>{tail}</em></div>
      <div className="crumbs mono">
        <span>Catalog</span>
        <span className="sep">/</span>
        <span style={{color:'var(--fg)'}}>{screen.label}</span>
      </div>
      <div className="spacer" />
      <div className="mono" style={{fontSize: 11, color:'var(--fg-mute)', display:'flex', alignItems:'center', gap: 12}}>
        <span><span style={{display:'inline-block', width:6, height:6, borderRadius:'50%', background:'var(--info)', marginRight:6}}/>Importing · 62%</span>
        <span><span style={{display:'inline-block', width:6, height:6, borderRadius:'50%', background:'var(--accent)', marginRight:6}}/>gemma4-27b · local</span>
      </div>
      <div style={{width: 16}} />
      <div className="win-ctrls">
        <button><Icon name="min" size={13} /></button>
        <button><Icon name="max" size={11} /></button>
        <button className="close"><Icon name="close" size={13} /></button>
      </div>
    </div>
  );
};

const Rail = ({ screen, setScreen }) => {
  const items = [
    { id: "onboard",  icon: "link",     label: "Sources" },
    { id: "catalog",  icon: "grid",     label: "Catalog" },
    { id: "cull",     icon: "cull",     label: "Cull" },
    { id: "cullbin",  icon: "flag",     label: "Cull Bin" },
    { id: "develop",  icon: "brush",    label: "Develop" },
  ];
  return (
    <div className="rail">
      {items.map(it => (
        <button key={it.id} className={screen.id===it.id ? "active":""} onClick={()=>setScreen(it)} title={it.label}>
          <Icon name={it.icon} size={16}/>
        </button>
      ))}
      <div style={{flex: 1}} />
      <button className={screen.id==="settings"?"active":""} onClick={()=>setScreen({id:"settings", label:"Settings"})} title="Settings">
        <Icon name="settings" size={16}/>
      </button>
    </div>
  );
};

const StatusBar = ({ screen }) => (
  <div className="statusbar">
    <span className="pill"><span className="dot"/>gemma4-27b · on-device</span>
    <span>Cataloging 847,291 / 851,002 · 99.6%</span>
    <span>Importing G:/ · 3,712 of 6,003</span>
    <div className="right">
      <span>Catalog: D:/Halide · 2.4 TB</span>
      <span>v0.9.0 (build 418)</span>
    </div>
  </div>
);

Object.assign(window, { Titlebar, Rail, StatusBar });
