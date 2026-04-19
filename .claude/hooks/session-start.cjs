#!/usr/bin/env node
// SessionStart hook: print the current phase, last checkpoint, and git state.
// Output is shown in the session once at the top.

const { execSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

function safe(fn, fallback = '') {
  try {
    return fn();
  } catch {
    return fallback;
  }
}

const phase = safe(() => {
  const claudeMd = fs.readFileSync('CLAUDE.md', 'utf8');
  const m = claudeMd.match(/^\*\*Current phase:\*\*\s*(.+)$/m);
  return m ? m[1].trim() : '(phase marker missing in CLAUDE.md)';
});

const lastCheckpoint = safe(() => {
  const p = path.join('docs', 'checkpoints', 'latest.md');
  if (!fs.existsSync(p)) return '(no checkpoint yet — run /phase-checkpoint)';
  const stat = fs.statSync(p);
  const head = fs.readFileSync(p, 'utf8').split('\n').slice(0, 3).join(' | ');
  const ageHours = ((Date.now() - stat.mtimeMs) / 3600000).toFixed(1);
  return `${head} (${ageHours}h ago)`;
});

const gitBranch = safe(
  () => execSync('git rev-parse --abbrev-ref HEAD', { encoding: 'utf8' }).trim(),
  '(no git)',
);
const gitStatus = safe(() => {
  const s = execSync('git status --porcelain', { encoding: 'utf8' });
  const n = s.trim().split('\n').filter(Boolean).length;
  return n === 0 ? 'clean' : `${n} changes`;
}, '?');

console.log(`[chronimage] phase=${phase} · branch=${gitBranch} · ${gitStatus}`);
console.log(`[chronimage] last checkpoint: ${lastCheckpoint}`);
console.log(
  '[chronimage] resume protocol → read CLAUDE.md → read docs/checkpoints/latest.md → read active PRD.',
);

process.exit(0);
