/**
 * Stub data used by screens until the Rust catalog backend is wired.
 * Derived from design-handoff/chronimage/project/src/data.jsx — keep in sync.
 *
 * Phase 1+ will swap these for real TanStack-Query hooks against Tauri
 * invoke() calls. The shape stays stable so screens don't care.
 */

export interface FixturePhoto {
  id: string;
  filename: string;
  ext: string;
  hue: number;
  scene: string;
  cam: string;
  lens: string;
  iso: number;
  ap: number;
  shut: string;
  mm: number;
  w: number;
  h: number;
  date: string;
  sizeMb: number;
}

export interface FixtureAlbum {
  id: string;
  name: string;
  count: number;
  tag: string;
  tint: number;
  desc: string;
  covers: number[];
}

export interface FixtureSource {
  id: string;
  name: string;
  count: string;
  status: 'synced' | 'syncing' | 'paused' | 'idle' | 'ready';
  sub: string;
  kind: 'disk' | 'cloud' | 'nas' | 'card' | 'iphone' | 'android';
}

export interface FixturePerson {
  name: string;
  count: number;
  face: number;
}

const HUES = [22, 40, 72, 150, 180, 210, 250, 290, 330, 8, 60, 200];
const SCENES = [
  "Kyoto · Philosopher's Path",
  'Studio · portrait session',
  'Oslo rooftops at dusk',
  'Backyard BBQ, July 4',
  'Lake Tahoe kayak trip',
  'Wedding · Maya & Rishi',
  'Newborn · Ari, 3 days',
  'Tokyo · Shinjuku night',
  'Moroccan souk, Fez',
  'Desk setup v3',
  'Family cabin weekend',
  'Milo, golden hour',
  'Product shots — Loop 2',
  'Marathon finish line',
  'Arctic circle road trip',
  "Aunt Priya's 60th",
  'Sunday pasta night',
  'Mountain biking — Moab',
  'Studio, chair series',
  'Hospital discharge day',
  'Concert — Phoebe Bridgers',
  'Apartment move-in',
  'Camping at Sequoia',
  'Birthday · Leo turns 5',
];
const CAMERAS = ['Sony A7 IV', 'Fuji X-T5', 'iPhone 15 Pro', 'Canon R6', 'Leica Q3', 'Nikon Zf'];
const LENSES = ['35/1.4 GM', 'XF 23/1.4', 'Main · 24mm', 'RF 50/1.2', '28/1.7 Summilux', '40/2'];
const EXT = ['ARW', 'RAF', 'HEIC', 'CR3', 'DNG', 'NEF'];
const ISOS = [100, 200, 400, 800, 1600, 3200, 6400];
const APS = [1.4, 1.8, 2.0, 2.8, 4.0, 5.6];
const SHUTS = ['1/1000', '1/500', '1/250', '1/125', '1/60', '1/30'];
const MMS = [24, 35, 50, 85, 135];

function makePhoto(i: number): FixturePhoto {
  const hue = HUES[i % HUES.length] ?? 0;
  const scene = SCENES[i % SCENES.length] ?? 'Untitled';
  const cam = CAMERAS[i % CAMERAS.length] ?? 'Sony A7 IV';
  const lens = LENSES[i % LENSES.length] ?? '35/1.4 GM';
  const ext = EXT[i % EXT.length] ?? 'ARW';
  const iso = ISOS[i % ISOS.length] ?? 400;
  const ap = APS[i % APS.length] ?? 2.8;
  const shut = SHUTS[i % SHUTS.length] ?? '1/250';
  const mm = MMS[i % MMS.length] ?? 50;
  const idNum = (4000 + i * 7).toString().padStart(4, '0');
  const month = String(1 + (i % 12)).padStart(2, '0');
  const day = String(1 + ((i * 3) % 27)).padStart(2, '0');
  return {
    id: `IMG_${idNum}`,
    filename: `IMG_${idNum}.${ext}`,
    ext,
    hue,
    scene,
    cam,
    lens,
    iso,
    ap,
    shut,
    mm,
    w: 6048,
    h: 4024,
    date: `2025-${month}-${day}`,
    sizeMb: Number((12 + ((i * 3.7) % 30)).toFixed(1)),
  };
}

export const PHOTOS: FixturePhoto[] = Array.from({ length: 60 }, (_, i) => makePhoto(i));

export const ALBUMS: FixtureAlbum[] = [
  {
    id: 'portraits',
    name: 'Portraits',
    count: 3240,
    tag: 'faces',
    tint: 22,
    desc: 'People in focus',
    covers: [0, 1, 8],
  },
  {
    id: 'goldenhour',
    name: 'Golden Hour',
    count: 814,
    tag: 'lighting',
    tint: 40,
    desc: 'Warm sunset light',
    covers: [2, 11, 15],
  },
  {
    id: 'kids',
    name: 'Kids — Ari & Leo',
    count: 1922,
    tag: 'people',
    tint: 330,
    desc: 'Family, 2024–26',
    covers: [6, 7, 23],
  },
  {
    id: 'food',
    name: 'Food & Kitchen',
    count: 488,
    tag: 'scenes',
    tint: 60,
    desc: 'Meals worth remembering',
    covers: [3, 16, 17],
  },
  {
    id: 'travel-jp',
    name: "Japan · Autumn '25",
    count: 612,
    tag: 'place',
    tint: 8,
    desc: 'Kyoto → Tokyo',
    covers: [0, 7, 14],
  },
  {
    id: 'product',
    name: 'Loop 2 · product shots',
    count: 142,
    tag: 'work',
    tint: 180,
    desc: 'Studio catalog',
    covers: [12, 18, 19],
  },
  {
    id: 'nightsky',
    name: 'Night & Low Light',
    count: 198,
    tag: 'lighting',
    tint: 250,
    desc: 'ISO ≥ 3200',
    covers: [7, 20, 21],
  },
  {
    id: 'events',
    name: 'Weddings & Events',
    count: 2104,
    tag: 'event',
    tint: 290,
    desc: 'Client deliveries',
    covers: [5, 16, 23],
  },
  {
    id: 'pets',
    name: 'Milo (golden retriever)',
    count: 711,
    tag: 'people',
    tint: 40,
    desc: 'On-device recognition',
    covers: [11, 2, 23],
  },
  {
    id: 'screenshots',
    name: 'Screenshots & Docs',
    count: 2841,
    tag: 'utility',
    tint: 200,
    desc: 'Auto-archived',
    covers: [9, 22, 13],
  },
  {
    id: 'burst',
    name: 'Burst & Duplicates',
    count: 487,
    tag: 'cull',
    tint: 25,
    desc: 'Flagged by similarity',
    covers: [1, 3, 5],
  },
  {
    id: 'blurry',
    name: 'Out-of-focus',
    count: 213,
    tag: 'cull',
    tint: 25,
    desc: 'Low sharpness score',
    covers: [4, 6, 14],
  },
];

export const SOURCES: FixtureSource[] = [
  {
    id: 'local-d',
    name: 'Local · D:/Photos',
    count: '212,481',
    status: 'synced',
    sub: 'Last scan 2h ago',
    kind: 'disk',
  },
  {
    id: 'local-ext',
    name: 'External · Samsung T7 (G:)',
    count: '48,207',
    status: 'idle',
    sub: 'Disconnected',
    kind: 'disk',
  },
  {
    id: 'google',
    name: 'Google Photos',
    count: '96,312',
    status: 'syncing',
    sub: '62% · 3m remaining',
    kind: 'cloud',
  },
  {
    id: 'onedrive',
    name: 'OneDrive · Camera Roll',
    count: '14,880',
    status: 'synced',
    sub: 'Last scan 18m ago',
    kind: 'cloud',
  },
  {
    id: 'icloud',
    name: 'iCloud · Shared Library',
    count: '8,312',
    status: 'paused',
    sub: 'Paused by user',
    kind: 'cloud',
  },
  {
    id: 'nas',
    name: 'NAS · \\\\synology\\photos',
    count: '402,009',
    status: 'synced',
    sub: 'Incremental · nightly',
    kind: 'nas',
  },
  {
    id: 'sd',
    name: 'SD card · Sony A7 IV',
    count: '214',
    status: 'ready',
    sub: 'Ready to import',
    kind: 'card',
  },
  {
    id: 'iphone',
    name: 'iPhone 15 Pro · USB',
    count: '3,214',
    status: 'ready',
    sub: "Ari's iPhone · 284 new since last sync",
    kind: 'iphone',
  },
  {
    id: 'pixel',
    name: 'Pixel 8 · USB (MTP)',
    count: '1,902',
    status: 'idle',
    sub: 'Plug in to import',
    kind: 'android',
  },
  {
    id: 'dropbox',
    name: 'Dropbox · Archive',
    count: '62,140',
    status: 'synced',
    sub: 'Last scan 1d ago',
    kind: 'cloud',
  },
];

export const PEOPLE: FixturePerson[] = [
  { name: 'Ari', count: 1243, face: 6 },
  { name: 'Leo', count: 982, face: 23 },
  { name: 'Maya', count: 611, face: 1 },
  { name: 'Rishi', count: 587, face: 5 },
  { name: 'Priya', count: 244, face: 16 },
  { name: 'Milo', count: 711, face: 11 },
];

export const SEARCH_SUGGESTIONS = [
  'Ari laughing at the beach',
  "Candid moments from Maya's wedding",
  'All photos of Milo in snow',
  'Sunset portraits on 35mm',
  'Food shots with warm light',
  'Tokyo night · neon',
  'Group photos where everyone smiles',
  'Photos Leo took himself',
];
