#!/usr/bin/env node
// Post-edit formatter: runs biome on TS/TSX, cargo fmt on Rust. Silent on success.
// Invoked by PostToolUse(Edit|Write). Reads tool invocation JSON from stdin.

const { execSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

let payload = '';
try {
  payload = fs.readFileSync(0, 'utf8');
} catch {
  process.exit(0);
}

let data;
try {
  data = JSON.parse(payload);
} catch {
  process.exit(0);
}

const filePath = data?.tool_input?.file_path;
if (!filePath) process.exit(0);

const rel = path.relative(process.cwd(), filePath).replace(/\\/g, '/');

// Skip non-source and design-handoff
if (
  rel.startsWith('design-handoff/') ||
  rel.startsWith('src-tauri/target/') ||
  rel.startsWith('node_modules/') ||
  rel.startsWith('dist/') ||
  rel.startsWith('models/')
) {
  process.exit(0);
}

const ext = path.extname(filePath).toLowerCase();

try {
  if (['.ts', '.tsx', '.js', '.jsx', '.json', '.css'].includes(ext)) {
    // Biome write+check
    execSync(`pnpm exec biome check --write --no-errors-on-unmatched "${filePath}"`, {
      stdio: 'ignore',
    });
  } else if (ext === '.rs') {
    execSync(`cargo fmt --manifest-path src-tauri/Cargo.toml -- "${filePath}"`, {
      stdio: 'ignore',
    });
  }
} catch {
  // Formatter missing or file not yet parseable; fail silently.
}

process.exit(0);
