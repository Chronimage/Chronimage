#!/usr/bin/env node
// Naive secret detector. Blocks common accidental key/token commits.
// Not a substitute for GitHub secret scanning; this is a fast local guard.

const fs = require('node:fs');

const PATTERNS = [
  { re: /-----BEGIN (RSA|OPENSSH|EC|DSA|PRIVATE) KEY-----/, label: 'private key block' },
  { re: /ghp_[A-Za-z0-9]{36,}/, label: 'GitHub personal access token' },
  { re: /gho_[A-Za-z0-9]{36,}/, label: 'GitHub OAuth token' },
  { re: /github_pat_[A-Za-z0-9_]{60,}/, label: 'GitHub fine-grained PAT' },
  { re: /xox[baprs]-[A-Za-z0-9-]{10,}/, label: 'Slack token' },
  { re: /AKIA[0-9A-Z]{16}/, label: 'AWS access key id' },
  { re: /AIza[0-9A-Za-z\-_]{35}/, label: 'Google API key' },
  { re: /sk-[A-Za-z0-9]{48}/, label: 'OpenAI-like secret key' },
  { re: /sk-ant-[A-Za-z0-9\-_]{20,}/, label: 'Anthropic API key' },
];

const files = process.argv.slice(2);
let violations = 0;

for (const file of files) {
  if (!fs.existsSync(file)) continue;
  const p = file.replace(/\\/g, '/');
  // Skip sample env files
  if (p.endsWith('.env.example') || p.endsWith('.gitignore')) continue;

  let content;
  try {
    content = fs.readFileSync(file, 'utf8');
  } catch {
    continue;
  }

  for (const { re, label } of PATTERNS) {
    const m = content.match(re);
    if (m) {
      console.error(`${file}: suspected ${label} — "${m[0].slice(0, 16)}…"`);
      violations++;
    }
  }
}

if (violations > 0) {
  console.error(
    `\n${violations} potential secret(s) in staged files. Remove and use env/secret store instead.`,
  );
  process.exit(1);
}

process.exit(0);
