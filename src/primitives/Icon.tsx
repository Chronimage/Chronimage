/**
 * Icon set — line icons only, no emoji, no hand-drawn. Use the `name` union
 * for type safety.
 *
 * All icons are treated as decorative (aria-hidden). Parent elements
 * (buttons, links) must provide their own aria-label.
 */

export type IconName =
  | 'home'
  | 'grid'
  | 'layers'
  | 'cull'
  | 'wand'
  | 'sparkles'
  | 'prompt'
  | 'search'
  | 'export'
  | 'settings'
  | 'link'
  | 'disk'
  | 'cloud'
  | 'nas'
  | 'card'
  | 'iphone'
  | 'android'
  | 'usb'
  | 'close'
  | 'min'
  | 'max'
  | 'chevR'
  | 'chevL'
  | 'chevD'
  | 'plus'
  | 'keep'
  | 'reject'
  | 'star'
  | 'flag'
  | 'tag'
  | 'eye'
  | 'history'
  | 'download'
  | 'crop'
  | 'brush'
  | 'faces'
  | 'ai'
  | 'compare';

export interface IconProps {
  name: IconName;
  size?: number;
  stroke?: number;
  className?: string;
}

interface SvgProps {
  viewBox: string;
  width: number;
  height: number;
  fill: 'none';
  stroke: string;
  strokeWidth: number;
  strokeLinecap: 'round';
  strokeLinejoin: 'round';
  'aria-hidden': true;
  role: 'presentation';
  focusable: false;
  className?: string;
}

const PATHS: Record<IconName, React.ReactElement> = {
  home: <path d="M3 11l9-7 9 7v9a2 2 0 0 1-2 2h-3v-6h-8v6H5a2 2 0 0 1-2-2z" />,
  grid: (
    <>
      <rect x="3" y="3" width="7" height="7" />
      <rect x="14" y="3" width="7" height="7" />
      <rect x="3" y="14" width="7" height="7" />
      <rect x="14" y="14" width="7" height="7" />
    </>
  ),
  layers: (
    <>
      <path d="M12 3l9 5-9 5-9-5z" />
      <path d="M3 12l9 5 9-5" />
      <path d="M3 17l9 5 9-5" />
    </>
  ),
  cull: (
    <>
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M5 6l1 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-14" />
    </>
  ),
  wand: (
    <>
      <path d="M15 4V2" />
      <path d="M15 10V8" />
      <path d="M12 7h-2" />
      <path d="M20 7h-2" />
      <path d="M18 11l-2-2" />
      <path d="M14 3l-2 2" />
      <path d="M3 18l11-11 3 3L6 21z" />
    </>
  ),
  sparkles: (
    <>
      <path d="M12 3l1.6 4.6L18 9l-4.4 1.4L12 15l-1.6-4.6L6 9l4.4-1.4z" />
      <path d="M18 15l.9 2.4L21 18l-2.1.6L18 21l-.9-2.4L15 18l2.1-.6z" />
    </>
  ),
  prompt: (
    <>
      <path d="M4 4h16v12H8l-4 4z" />
      <path d="M8 10h.01" />
      <path d="M12 10h.01" />
      <path d="M16 10h.01" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-4-4" />
    </>
  ),
  export: (
    <>
      <path d="M12 3v12" />
      <path d="M8 7l4-4 4 4" />
      <path d="M5 21h14" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3.9a7 7 0 0 0-2-1.2L14 3h-4l-.6 2.6a7 7 0 0 0-2 1.2l-2.3-.9-2 3.4 2 1.5a7 7 0 0 0 0 2.4l-2 1.5 2 3.4 2.3-.9a7 7 0 0 0 2 1.2L10 21h4l.6-2.6a7 7 0 0 0 2-1.2l2.3.9 2-3.4-2-1.5a7 7 0 0 0 .1-1.2z" />
    </>
  ),
  link: (
    <>
      <path d="M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1" />
      <path d="M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1" />
    </>
  ),
  disk: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <circle cx="12" cy="12" r="4" />
      <circle cx="12" cy="12" r="1" />
    </>
  ),
  cloud: <path d="M7 18a5 5 0 0 1-1-9.9A6 6 0 0 1 18 9a4 4 0 0 1 0 8H7z" />,
  nas: (
    <>
      <rect x="3" y="5" width="18" height="6" rx="1" />
      <rect x="3" y="13" width="18" height="6" rx="1" />
      <path d="M7 8h.01" />
      <path d="M7 16h.01" />
    </>
  ),
  card: (
    <>
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M8 3v5M12 3v5M16 3v5" />
    </>
  ),
  iphone: (
    <>
      <rect x="7" y="2" width="10" height="20" rx="2" />
      <path d="M11 18h2" />
    </>
  ),
  android: (
    <>
      <rect x="6" y="3" width="12" height="18" rx="2" />
      <path d="M10 6h4" />
      <circle cx="12" cy="17" r=".8" />
    </>
  ),
  usb: (
    <>
      <circle cx="12" cy="4" r="1.5" />
      <path d="M12 5v9l-4 4v2h8v-2l-4-4V5z" />
      <path d="M10 11h4" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6L6 18" />,
  min: <path d="M5 12h14" />,
  max: <rect x="5" y="5" width="14" height="14" />,
  chevR: <path d="M9 6l6 6-6 6" />,
  chevL: <path d="M15 6l-6 6 6 6" />,
  chevD: <path d="M6 9l6 6 6-6" />,
  plus: <path d="M12 5v14M5 12h14" />,
  keep: <path d="M5 12l5 5 9-12" />,
  reject: <path d="M6 6l12 12M18 6L6 18" />,
  star: <path d="M12 3l2.9 6 6.6.9-4.8 4.6 1.1 6.5-5.8-3-5.8 3 1.1-6.5L2.5 9.9 9.1 9z" />,
  flag: <path d="M5 21V4h11l-2 4 2 4H5" />,
  tag: (
    <>
      <path d="M3 13V4h9l9 9-9 9z" />
      <circle cx="8" cy="8" r="1.5" />
    </>
  ),
  eye: (
    <>
      <path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  history: (
    <>
      <path d="M3 12a9 9 0 1 0 3-6.7" />
      <path d="M3 4v5h5" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  download: (
    <>
      <path d="M12 3v14" />
      <path d="M7 12l5 5 5-5" />
      <path d="M5 21h14" />
    </>
  ),
  crop: (
    <>
      <path d="M6 2v16h16" />
      <path d="M2 6h16v16" />
    </>
  ),
  brush: (
    <>
      <path d="M9 11l4-4 7 7-4 4z" />
      <path d="M9 11l-5 5 3 3 5-5" />
      <path d="M4 20l1-1" />
    </>
  ),
  faces: (
    <>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21c0-4 4-7 8-7s8 3 8 7" />
    </>
  ),
  ai: (
    <>
      <path d="M12 3v4" />
      <path d="M12 17v4" />
      <path d="M3 12h4" />
      <path d="M17 12h4" />
      <rect x="7" y="7" width="10" height="10" rx="2" />
      <path d="M10 11h4" />
      <path d="M10 13h3" />
    </>
  ),
  compare: (
    <>
      <rect x="3" y="5" width="8" height="14" rx="1" />
      <rect x="13" y="5" width="8" height="14" rx="1" />
      <path d="M12 2v20" strokeDasharray="2 2" />
    </>
  ),
};

export function Icon({ name, size = 18, stroke = 1.6, className }: IconProps) {
  const s: SvgProps = {
    viewBox: '0 0 24 24',
    width: size,
    height: size,
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: stroke,
    strokeLinecap: 'round',
    strokeLinejoin: 'round',
    'aria-hidden': true,
    role: 'presentation',
    focusable: false,
    ...(className ? { className } : {}),
  };
  return <svg {...s}>{PATHS[name]}</svg>;
}
