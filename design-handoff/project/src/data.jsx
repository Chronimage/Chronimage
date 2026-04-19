// Seed data for Chronimage prototype

// Deterministic "photo" placeholders — hue + label + meta
const HUES = [22, 40, 72, 150, 180, 210, 250, 290, 330, 8, 60, 200];
const SCENES = [
  "Kyoto · Philosopher's Path", "Studio · portrait session", "Oslo rooftops at dusk",
  "Backyard BBQ, July 4", "Lake Tahoe kayak trip", "Wedding · Maya & Rishi",
  "Newborn · Ari, 3 days", "Tokyo · Shinjuku night", "Moroccan souk, Fez",
  "Desk setup v3", "Family cabin weekend", "Milo, golden hour",
  "Product shots — Loop 2", "Marathon finish line", "Arctic circle road trip",
  "Aunt Priya's 60th", "Sunday pasta night", "Mountain biking — Moab",
  "Studio, chair series", "Hospital discharge day", "Concert — Phoebe Bridgers",
  "Apartment move-in", "Camping at Sequoia", "Birthday · Leo turns 5",
];
const CAMERAS = ["Sony A7 IV", "Fuji X-T5", "iPhone 15 Pro", "Canon R6", "Leica Q3", "Nikon Zf"];
const LENSES = ["35/1.4 GM", "XF 23/1.4", "Main · 24mm", "RF 50/1.2", "28/1.7 Summilux", "40/2"];
const EXT = ["ARW", "RAF", "HEIC", "CR3", "DNG", "NEF"];

function photo(i, opts={}) {
  const hue = HUES[i % HUES.length];
  const scene = SCENES[i % SCENES.length];
  const cam = CAMERAS[i % CAMERAS.length];
  const lens = LENSES[i % LENSES.length];
  const ext = EXT[i % EXT.length];
  const iso = [100, 200, 400, 800, 1600, 3200, 6400][i % 7];
  const ap = [1.4, 1.8, 2.0, 2.8, 4.0, 5.6][i % 6];
  const shut = ["1/1000", "1/500", "1/250", "1/125", "1/60", "1/30"][i % 6];
  const mm = [24, 35, 50, 85, 135][i % 5];
  return {
    id: "IMG_" + (4000 + i * 7).toString().padStart(4, "0"),
    filename: "IMG_" + (4000 + i * 7).toString().padStart(4, "0") + "." + ext,
    ext, hue, scene, cam, lens,
    iso, ap, shut, mm,
    w: 6048, h: 4024,
    date: "2025-" + String(1 + (i % 12)).padStart(2, "0") + "-" + String(1 + (i*3 % 27)).padStart(2, "0"),
    sizeMb: (12 + (i * 3.7) % 30).toFixed(1),
    ...opts
  };
}

const PHOTOS = Array.from({length: 60}, (_, i) => photo(i));

// Smart albums auto-generated
const ALBUMS = [
  { id: "portraits",   name: "Portraits",                count: 3240, tag: "faces",         tint: 22,  desc: "People in focus",  covers: [0, 1, 8] },
  { id: "goldenhour",  name: "Golden Hour",              count: 814,  tag: "lighting",      tint: 40,  desc: "Warm sunset light", covers: [2, 11, 15] },
  { id: "kids",        name: "Kids — Ari & Leo",         count: 1922, tag: "people",        tint: 330, desc: "Family, 2024–26",  covers: [6, 7, 23] },
  { id: "food",        name: "Food & Kitchen",           count: 488,  tag: "scenes",        tint: 60,  desc: "Meals worth remembering", covers: [3, 16, 17] },
  { id: "travel-jp",   name: "Japan · Autumn '25",       count: 612,  tag: "place",         tint: 8,   desc: "Kyoto → Tokyo",    covers: [0, 7, 14] },
  { id: "product",     name: "Loop 2 · product shots",   count: 142,  tag: "work",          tint: 180, desc: "Studio catalog",   covers: [12, 18, 19] },
  { id: "nightsky",    name: "Night & Low Light",        count: 198,  tag: "lighting",      tint: 250, desc: "ISO ≥ 3200",       covers: [7, 20, 21] },
  { id: "events",      name: "Weddings & Events",        count: 2104, tag: "event",         tint: 290, desc: "Client deliveries",covers: [5, 16, 23] },
  { id: "pets",        name: "Milo (golden retriever)",  count: 711,  tag: "people",        tint: 40,  desc: "On-device recognition", covers: [11, 2, 23] },
  { id: "screenshots", name: "Screenshots & Docs",       count: 2841, tag: "utility",       tint: 200, desc: "Auto-archived",    covers: [9, 22, 13] },
  { id: "burst",       name: "Burst & Duplicates",       count: 487,  tag: "cull",          tint: 25,  desc: "Flagged by similarity", covers: [1, 3, 5] },
  { id: "blurry",      name: "Out-of-focus",             count: 213,  tag: "cull",          tint: 25,  desc: "Low sharpness score", covers: [4, 6, 14] },
];

// Cull pairs (duplicates / near-duplicates)
const CULL_PAIRS = [
  { ids: [10, 11], reason: "Near-duplicate", similarity: 0.97, keep: 1, issues_a: ["eyes closed"], issues_b: [] },
  { ids: [12, 13], reason: "Burst · 6 frames", similarity: 0.92, keep: 0, issues_a: [], issues_b: ["slight blur"] },
  { ids: [3, 14],  reason: "Near-duplicate", similarity: 0.89, keep: 0, issues_a: [], issues_b: ["eyes closed", "low contrast"] },
  { ids: [20, 21], reason: "Out of focus", similarity: 0.74, keep: 1, issues_a: ["out of focus"], issues_b: [] },
  { ids: [2, 8],   reason: "Very similar", similarity: 0.85, keep: 0, issues_a: [], issues_b: ["head cropped"] },
];

// AI presets
const PRESETS = [
  { id: "skin",       group: "Face",       name: "Clean up face",      sub: "Even tone · reduce blemishes" },
  { id: "lips",       group: "Face",       name: "Beautify lips",      sub: "Enhance color · smooth edge" },
  { id: "teeth",      group: "Face",       name: "Whiten teeth",       sub: "Natural brightness" },
  { id: "relight",    group: "Face",       name: "Portrait relight",   sub: "Simulate key & fill" },
  { id: "bg-remove",  group: "Scene",      name: "Remove background",  sub: "Subject on alpha" },
  { id: "sky",        group: "Scene",      name: "Enhance sky",        sub: "Re-expose + sub-horizon lift" },
  { id: "expose",     group: "Scene",      name: "Fix exposure",       sub: "Auto white balance + EV" },
  { id: "remove",     group: "Scene",      name: "Remove object",      sub: "Inpaint masked region" },
  { id: "upscale",    group: "Quality",    name: "Upscale 2×",         sub: "Preserve micro-detail" },
  { id: "denoise",    group: "Quality",    name: "Denoise",            sub: "Low-light grain reduction" },
  { id: "bw",         group: "Style",      name: "B&W film",           sub: "Tri-X 400 emulation" },
];

// Sources (Windows-flavored)
const SOURCES = [
  { id: "local-d",   name: "Local · D:/Photos",            count: "212,481", status: "synced",  sub: "Last scan 2h ago", kind: "disk" },
  { id: "local-ext", name: "External · Samsung T7 (G:)",   count: "48,207",  status: "idle",    sub: "Disconnected", kind: "disk" },
  { id: "google",    name: "Google Photos",                count: "96,312",  status: "syncing", sub: "62% · 3m remaining", kind: "cloud" },
  { id: "onedrive",  name: "OneDrive · Camera Roll",       count: "14,880",  status: "synced",  sub: "Last scan 18m ago", kind: "cloud" },
  { id: "icloud",    name: "iCloud · Shared Library",      count: "8,312",   status: "paused",  sub: "Paused by user", kind: "cloud" },
  { id: "nas",       name: "NAS · \\\\synology\\photos",   count: "402,009", status: "synced",  sub: "Incremental · nightly", kind: "nas" },
  { id: "sd",        name: "SD card · Sony A7 IV",         count: "214",     status: "ready",   sub: "Ready to import", kind: "card" },
  { id: "iphone",    name: "iPhone 15 Pro · USB",          count: "3,214",   status: "ready",   sub: "Ari's iPhone · 284 new since last sync", kind: "iphone" },
  { id: "pixel",     name: "Pixel 8 · USB (MTP)",          count: "1,902",   status: "idle",    sub: "Plug in to import", kind: "android" },
  { id: "dropbox",   name: "Dropbox · Archive",            count: "62,140", status: "synced",  sub: "Last scan 1d ago", kind: "cloud" },
];

// People (for facets)
const PEOPLE = [
  { name: "Ari", count: 1243, face: 6 },
  { name: "Leo", count: 982, face: 23 },
  { name: "Maya", count: 611, face: 1 },
  { name: "Rishi", count: 587, face: 5 },
  { name: "Priya", count: 244, face: 16 },
  { name: "Milo", count: 711, face: 11 },
];

const SEARCH_SUGGESTIONS = [
  "Ari laughing at the beach",
  "Candid moments from Maya's wedding",
  "All photos of Milo in snow",
  "Sunset portraits on 35mm",
  "Food shots with warm light",
  "Tokyo night · neon",
  "Group photos where everyone smiles",
  "Photos Leo took himself",
];

Object.assign(window, {
  PHOTOS, ALBUMS, CULL_PAIRS, PRESETS, SOURCES, PEOPLE, SEARCH_SUGGESTIONS
});
