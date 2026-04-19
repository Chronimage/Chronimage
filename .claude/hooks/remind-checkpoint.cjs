#!/usr/bin/env node
// On session Stop, remind the agent to run /phase-checkpoint if there's been meaningful work.
// We don't see tool counts directly; use git status as a proxy for "unsaved progress".

const { execSync } = require('node:child_process');

try {
  const status = execSync('git status --porcelain', { encoding: 'utf8' });
  const lines = status.trim().split('\n').filter(Boolean);
  if (lines.length >= 3) {
    console.log(
      `[checkpoint] ${lines.length} uncommitted changes. Consider running /phase-checkpoint to update docs/checkpoints/latest.md before ending.`,
    );
  }
} catch {
  // Not a git repo or git missing — silent.
}

process.exit(0);
