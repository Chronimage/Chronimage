#!/usr/bin/env node
/*
 * Emit a JSON bundle-size baseline to stdout. Used by release-stable.yml:
 *   node scripts/record-size-baseline.js v1.2.3 > docs/release/bundle-size-baseline.json
 */

const fs = require('node:fs');
const path = require('node:path');

const release = process.argv[2];
if (!release) {
  console.error('Usage: node scripts/record-size-baseline.js <release-tag>');
  process.exit(1);
}

const bundleDir = path.join('src-tauri', 'target', 'release', 'bundle', 'msi');
const msi = fs.existsSync(bundleDir) ? fs.readdirSync(bundleDir).find((f) => f.endsWith('.msi')) : null;

if (!msi) {
  console.error('No MSI found in', bundleDir);
  process.exit(1);
}

const full = path.join(bundleDir, msi);
const bytes = fs.statSync(full).size;
const sizeMb = Number((bytes / 1024 / 1024).toFixed(2));

const out = {
  release,
  msi: path.basename(full),
  size_bytes: bytes,
  size_mb: sizeMb,
  recorded_at: new Date().toISOString(),
};

console.log(JSON.stringify(out, null, 2));
