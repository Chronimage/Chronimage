// Placeholder "photos" — striped SVG tiles colored by hue, with mono caption
const Placeholder = ({ photo, idx, selected, rejected, keep, showLabel = true, subtle = false, className = "" }) => {
  const hue = photo?.hue ?? (idx * 31) % 360;
  const p = photo || { hue, filename: `IMG_${idx || 0}.ARW`, scene: "", ap: 1.8 };
  const tintA = `oklch(0.58 0.18 ${hue})`;
  const tintB = `oklch(0.22 0.08 ${hue})`;
  const seed = ((p.id?.charCodeAt?.(4) || 7) * 17) % 100;
  return (
    <div className={`ph ${selected ? "selected":""} ${rejected ? "rejected":""} ${className}`}>
      <div className="tint" style={{
        background: `linear-gradient(${135 + (seed % 60)}deg, ${tintA} 0%, ${tintB} 100%)`
      }} />
      <div className="tint" style={{
        backgroundImage: `repeating-linear-gradient( -45deg, transparent 0 10px, rgba(255,255,255,0.04) 10px 11px )`
      }} />
      {rejected && <div className="corner-rej">×</div>}
      {keep && <div className="corner-keep">✓</div>}
      {showLabel && !subtle && (
        <div className="cap">
          <div style={{fontWeight:500, color: 'rgba(255,255,255,0.9)'}}>{p.filename}</div>
          <div style={{opacity:0.65}}>{p.scene}</div>
        </div>
      )}
      {showLabel && subtle && (
        <div className="cap" style={{opacity: 0.75}}>
          <div>{p.filename}</div>
        </div>
      )}
    </div>
  );
};

// Icon set — simple line icons (no emoji, no hand-drawn)
const Icon = ({name, size=18, stroke=1.6}) => {
  const s = { width: size, height: size, fill: 'none', stroke: 'currentColor', strokeWidth: stroke, strokeLinecap: 'round', strokeLinejoin: 'round' };
  switch (name) {
    case 'home':     return <svg viewBox="0 0 24 24" {...s}><path d="M3 11l9-7 9 7v9a2 2 0 0 1-2 2h-3v-6h-8v6H5a2 2 0 0 1-2-2z"/></svg>;
    case 'grid':     return <svg viewBox="0 0 24 24" {...s}><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>;
    case 'layers':   return <svg viewBox="0 0 24 24" {...s}><path d="M12 3l9 5-9 5-9-5z"/><path d="M3 12l9 5 9-5"/><path d="M3 17l9 5 9-5"/></svg>;
    case 'cull':     return <svg viewBox="0 0 24 24" {...s}><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M5 6l1 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-14"/></svg>;
    case 'wand':     return <svg viewBox="0 0 24 24" {...s}><path d="M15 4V2"/><path d="M15 10V8"/><path d="M12 7h-2"/><path d="M20 7h-2"/><path d="M18 11l-2-2"/><path d="M14 3l-2 2"/><path d="M3 21l11-11 3 3L6 24z" transform="translate(0, -3)"/></svg>;
    case 'sparkles': return <svg viewBox="0 0 24 24" {...s}><path d="M12 3l1.6 4.6L18 9l-4.4 1.4L12 15l-1.6-4.6L6 9l4.4-1.4z"/><path d="M18 15l.9 2.4L21 18l-2.1.6L18 21l-.9-2.4L15 18l2.1-.6z"/></svg>;
    case 'prompt':   return <svg viewBox="0 0 24 24" {...s}><path d="M4 4h16v12H8l-4 4z"/><path d="M8 10h.01"/><path d="M12 10h.01"/><path d="M16 10h.01"/></svg>;
    case 'search':   return <svg viewBox="0 0 24 24" {...s}><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>;
    case 'export':   return <svg viewBox="0 0 24 24" {...s}><path d="M12 3v12"/><path d="M8 7l4-4 4 4"/><path d="M5 21h14"/></svg>;
    case 'settings': return <svg viewBox="0 0 24 24" {...s}><circle cx="12" cy="12" r="3"/><path d="M19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3.9a7 7 0 0 0-2-1.2L14 3h-4l-.6 2.6a7 7 0 0 0-2 1.2l-2.3-.9-2 3.4 2 1.5a7 7 0 0 0 0 2.4l-2 1.5 2 3.4 2.3-.9a7 7 0 0 0 2 1.2L10 21h4l.6-2.6a7 7 0 0 0 2-1.2l2.3.9 2-3.4-2-1.5a7 7 0 0 0 .1-1.2z"/></svg>;
    case 'link':     return <svg viewBox="0 0 24 24" {...s}><path d="M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1"/><path d="M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1"/></svg>;
    case 'disk':     return <svg viewBox="0 0 24 24" {...s}><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="12" cy="12" r="4"/><circle cx="12" cy="12" r="1"/></svg>;
    case 'cloud':    return <svg viewBox="0 0 24 24" {...s}><path d="M7 18a5 5 0 0 1-1-9.9A6 6 0 0 1 18 9a4 4 0 0 1 0 8H7z"/></svg>;
    case 'nas':      return <svg viewBox="0 0 24 24" {...s}><rect x="3" y="5" width="18" height="6" rx="1"/><rect x="3" y="13" width="18" height="6" rx="1"/><path d="M7 8h.01"/><path d="M7 16h.01"/></svg>;
    case 'card':     return <svg viewBox="0 0 24 24" {...s}><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M8 3v5M12 3v5M16 3v5"/></svg>;
    case 'iphone':   return <svg viewBox="0 0 24 24" {...s}><rect x="7" y="2" width="10" height="20" rx="2"/><path d="M11 18h2"/></svg>;
    case 'android':  return <svg viewBox="0 0 24 24" {...s}><rect x="6" y="3" width="12" height="18" rx="2"/><path d="M10 6h4"/><circle cx="12" cy="17" r=".8"/></svg>;
    case 'usb':      return <svg viewBox="0 0 24 24" {...s}><circle cx="12" cy="4" r="1.5"/><path d="M12 5v9l-4 4v2h8v-2l-4-4V5z"/><path d="M10 11h4"/></svg>;
    case 'close':    return <svg viewBox="0 0 24 24" {...s}><path d="M6 6l12 12M18 6L6 18"/></svg>;
    case 'min':      return <svg viewBox="0 0 24 24" {...s}><path d="M5 12h14"/></svg>;
    case 'max':      return <svg viewBox="0 0 24 24" {...s}><rect x="5" y="5" width="14" height="14"/></svg>;
    case 'chevR':    return <svg viewBox="0 0 24 24" {...s}><path d="M9 6l6 6-6 6"/></svg>;
    case 'chevL':    return <svg viewBox="0 0 24 24" {...s}><path d="M15 6l-6 6 6 6"/></svg>;
    case 'chevD':    return <svg viewBox="0 0 24 24" {...s}><path d="M6 9l6 6 6-6"/></svg>;
    case 'plus':     return <svg viewBox="0 0 24 24" {...s}><path d="M12 5v14M5 12h14"/></svg>;
    case 'keep':     return <svg viewBox="0 0 24 24" {...s}><path d="M5 12l5 5 9-12"/></svg>;
    case 'reject':   return <svg viewBox="0 0 24 24" {...s}><path d="M6 6l12 12M18 6L6 18"/></svg>;
    case 'star':     return <svg viewBox="0 0 24 24" {...s}><path d="M12 3l2.9 6 6.6.9-4.8 4.6 1.1 6.5-5.8-3-5.8 3 1.1-6.5L2.5 9.9 9.1 9z"/></svg>;
    case 'flag':     return <svg viewBox="0 0 24 24" {...s}><path d="M5 21V4h11l-2 4 2 4H5"/></svg>;
    case 'tag':      return <svg viewBox="0 0 24 24" {...s}><path d="M3 13V4h9l9 9-9 9z"/><circle cx="8" cy="8" r="1.5"/></svg>;
    case 'eye':      return <svg viewBox="0 0 24 24" {...s}><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></svg>;
    case 'history':  return <svg viewBox="0 0 24 24" {...s}><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v5h5"/><path d="M12 7v5l3 2"/></svg>;
    case 'download': return <svg viewBox="0 0 24 24" {...s}><path d="M12 3v14"/><path d="M7 12l5 5 5-5"/><path d="M5 21h14"/></svg>;
    case 'crop':     return <svg viewBox="0 0 24 24" {...s}><path d="M6 2v16h16"/><path d="M2 6h16v16"/></svg>;
    case 'brush':    return <svg viewBox="0 0 24 24" {...s}><path d="M9 11l4-4 7 7-4 4z"/><path d="M9 11l-5 5 3 3 5-5"/><path d="M4 20l1-1"/></svg>;
    case 'faces':    return <svg viewBox="0 0 24 24" {...s}><circle cx="12" cy="8" r="4"/><path d="M4 21c0-4 4-7 8-7s8 3 8 7"/></svg>;
    case 'ai':       return <svg viewBox="0 0 24 24" {...s}><path d="M12 3v4"/><path d="M12 17v4"/><path d="M3 12h4"/><path d="M17 12h4"/><rect x="7" y="7" width="10" height="10" rx="2"/><path d="M10 11h4"/><path d="M10 13h3"/></svg>;
    case 'compare':  return <svg viewBox="0 0 24 24" {...s}><rect x="3" y="5" width="8" height="14" rx="1"/><rect x="13" y="5" width="8" height="14" rx="1"/><path d="M12 2v20" strokeDasharray="2 2"/></svg>;
    default: return <svg viewBox="0 0 24 24" {...s}><circle cx="12" cy="12" r="9"/></svg>;
  }
};

Object.assign(window, { Placeholder, Icon });
