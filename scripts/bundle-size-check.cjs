#!/usr/bin/env node
// Compare the built Tauri MSI size against the last release in the same channel.
// Invoked in CI after `cargo tauri build`. Fails on >5 MB regression unless PR carries [size-ok] label.

const fs = require('node:fs');
const path = require('node:path');

const LIMIT_MB = 900;
const DELTA_LIMIT_MB = 5;

function findMsi() {
  const dir = path.join('src-tauri', 'target', 'release', 'bundle', 'msi');
  if (!fs.existsSync(dir)) {
    console.error(`No MSI directory found at ${dir}`);
    process.exit(1);
  }
  const msis = fs.readdirSync(dir).filter((f) => f.endsWith('.msi'));
  if (msis.length === 0) {
    console.error('No .msi file produced.');
    process.exit(1);
  }
  return path.join(dir, msis[0]);
}

function mb(bytes) {
  return (bytes / 1024 / 1024).toFixed(2);
}

const msi = findMsi();
const size = fs.statSync(msi).size;
const sizeMb = Number.parseFloat(mb(size));

console.log(`MSI: ${msi}`);
console.log(`Size: ${sizeMb} MB`);

if (sizeMb > LIMIT_MB) {
  console.error(`❌ Installer exceeds hard limit of ${LIMIT_MB} MB.`);
  process.exit(1);
}

// Compare against last known good size if we recorded one
const baselinePath = path.join('docs', 'release', 'bundle-size-baseline.json');
if (fs.existsSync(baselinePath)) {
  const baseline = JSON.parse(fs.readFileSync(baselinePath, 'utf8'));
  const delta = sizeMb - baseline.size_mb;
  console.log(`Baseline: ${baseline.size_mb} MB (release ${baseline.release}) · Δ ${delta.toFixed(2)} MB`);
  const labelSkips = (process.env.PR_LABELS || '').split(',').includes('size-ok');
  if (delta > DELTA_LIMIT_MB && !labelSkips) {
    console.error(
      `❌ Installer grew by ${delta.toFixed(2)} MB (> ${DELTA_LIMIT_MB} MB). Add [size-ok] label to override.`,
    );
    process.exit(1);
  }
}

console.log('✅ Bundle size check passed.');
process.exit(0);
