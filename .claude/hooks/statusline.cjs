#!/usr/bin/env node
// Statusline: shows current phase, branch, uncommitted count, TODO(cc) count.
// Output a single line.

const { execSync } = require('node:child_process');
const fs = require('node:fs');

function safe(fn, fallback = '') {
  try {
    return fn();
  } catch {
    return fallback;
  }
}

const phase = safe(() => {
  const m = fs.readFileSync('CLAUDE.md', 'utf8').match(/^\*\*Current phase:\*\*\s*(.+)$/m);
  if (!m) return 'phase?';
  // Shorten "Phase 0 — Foundation & quality backbone (week 1)" → "P0"
  const pm = m[1].match(/Phase\s+(\d+)/i);
  return pm ? `P${pm[1]}` : m[1].slice(0, 20);
}, '?');

const branch = safe(() => execSync('git rev-parse --abbrev-ref HEAD', { encoding: 'utf8' }).trim(), '?');
const changes = safe(() => {
  const s = execSync('git status --porcelain', { encoding: 'utf8' });
  const n = s.trim().split('\n').filter(Boolean).length;
  return n;
}, 0);

// Find TODO(cc) counts across tracked files (cheap approximation)
const todoCount = safe(() => {
  const out = execSync('git grep -l "TODO(cc)" -- "*.ts" "*.tsx" "*.rs" 2>/dev/null | wc -l', {
    encoding: 'utf8',
    shell: true,
  }).trim();
  return Number.parseInt(out, 10) || 0;
}, 0);

const changesStr = changes === 0 ? '✓' : `${changes}Δ`;
const todoStr = todoCount === 0 ? '' : ` · ${todoCount} TODO(cc)`;

process.stdout.write(`[${phase}] ${branch} · ${changesStr}${todoStr}`);
process.exit(0);
