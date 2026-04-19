#!/usr/bin/env node
// Pre-Bash guard: blocks destructive git flags and hook-bypass attempts.
// Invoked by PreToolUse(Bash). Exit 2 to block with a message.

const fs = require('node:fs');

let data;
try {
  data = JSON.parse(fs.readFileSync(0, 'utf8'));
} catch {
  process.exit(0);
}

const cmd = (data?.tool_input?.command || '').trim();

const blocks = [
  {
    re: /\bgit\s+(commit|push|rebase|merge|tag)\s+[^\n]*--no-verify\b/,
    msg: 'Skipping hooks (--no-verify) is not allowed. Fix the underlying issue instead.',
  },
  {
    re: /\bgit\s+push\s+[^\n]*--force\b/,
    msg: 'git push --force is blocked. Use --force-with-lease only if strictly necessary, and ask the user first.',
  },
  {
    re: /\bgit\s+reset\s+--hard\b/,
    msg: 'git reset --hard is destructive. Confirm with the user; prefer git stash or a revert commit.',
  },
  {
    re: /\bcargo\s+tauri\s+build\b/,
    msg: 'cargo tauri build produces release artifacts and should run in CI, not locally. Push a tag to release.',
  },
  {
    re: /\brm\s+-rf\s+\//,
    msg: 'Absolute-path rm -rf blocked.',
  },
];

for (const b of blocks) {
  if (b.re.test(cmd)) {
    console.error(`[pre-bash-guard] ${b.msg}`);
    process.exit(2);
  }
}

process.exit(0);
