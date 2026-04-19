#!/usr/bin/env node
// Forbidden-patterns check. Runs in lefthook pre-commit + PostToolUse.
// Usage: node scripts/forbidden-patterns.js [file1] [file2] ...
// With no args, scans all tracked source files.

const { execSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const RULES = [
  // Rust
  {
    file: /\.rs$/,
    pattern: /(^|[^\w.])unwrap\s*\(\s*\)/,
    message: 'No `unwrap()` outside tests. Use `?` with a typed error.',
    // Skip tests: handled by file path check below.
  },
  {
    file: /\.rs$/,
    pattern: /(^|[^\w.])expect\s*\(/,
    message: 'No `expect()` outside tests. Use `.context("...")` with `anyhow`.',
  },
  {
    file: /\.rs$/,
    pattern: /\bdbg!\s*\(/,
    message: 'No `dbg!()` in committed code. Use `tracing::debug!` instead.',
  },
  {
    file: /\.rs$/,
    pattern: /(^|[^\w.])panic!\s*\(/,
    message: 'No `panic!()` outside tests. Return an error.',
  },
  {
    file: /\.rs$/,
    pattern: /(^|[^\w.])todo!\s*\(/,
    message: 'No `todo!()` macro on main-path code. Use `// TODO(cc): ...` comment + explicit error.',
  },
  // TypeScript
  {
    file: /\.(ts|tsx)$/,
    pattern: /(^|[^\w.])console\.log\s*\(/,
    message: 'No `console.log`. Use `debug()` from `src/util/log.ts`.',
  },
  {
    file: /\.(ts|tsx)$/,
    pattern: /\bdebugger\b/,
    message: 'No `debugger` statements.',
  },
  {
    file: /\.(ts|tsx)$/,
    pattern: /:\s*any(\b|$)/,
    message: 'No explicit `any` type. Use `unknown` + narrowing.',
    // Whitelist: comments
  },
  // Generic
  {
    file: /\.(ts|tsx|rs)$/,
    pattern: /TODO\(blocker\)/,
    message: 'TODO(blocker) markers must be resolved before commit.',
  },
];

function getStagedFiles() {
  try {
    const out = execSync('git diff --cached --name-only --diff-filter=ACM', { encoding: 'utf8' });
    return out.split('\n').filter(Boolean);
  } catch {
    return [];
  }
}

function isTestFile(filePath) {
  const p = filePath.replace(/\\/g, '/');
  return (
    /\/(tests|__tests__|benches)\//.test(p) ||
    /\.(test|spec)\.(ts|tsx|js|jsx|rs)$/.test(p) ||
    p.includes('src-tauri/tests/')
  );
}

function skipFile(filePath) {
  const p = filePath.replace(/\\/g, '/');
  return (
    p.startsWith('design-handoff/') ||
    p.startsWith('node_modules/') ||
    p.startsWith('src-tauri/target/') ||
    p.startsWith('dist/') ||
    p.startsWith('.claude/hooks/') ||
    p.startsWith('scripts/')
  );
}

const files = process.argv.slice(2).length ? process.argv.slice(2) : getStagedFiles();
let violations = 0;

for (const file of files) {
  if (skipFile(file)) continue;
  if (!fs.existsSync(file)) continue;

  const isTest = isTestFile(file);
  let content;
  try {
    content = fs.readFileSync(file, 'utf8');
  } catch {
    continue;
  }

  const lines = content.split('\n');
  for (const rule of RULES) {
    if (!rule.file.test(file)) continue;
    // Allow unwrap/expect/panic in tests
    if (isTest && /unwrap|expect|panic!|todo!/.test(rule.pattern.source)) continue;

    lines.forEach((line, i) => {
      // Skip lines that are line-comments
      if (/^\s*(\/\/|#)/.test(line)) return;
      if (rule.pattern.test(line)) {
        console.error(`${file}:${i + 1}  ${rule.message}`);
        console.error(`    ${line.trim()}`);
        violations++;
      }
    });
  }
}

if (violations > 0) {
  console.error(`\n${violations} forbidden-pattern violation(s) found.`);
  process.exit(1);
}

process.exit(0);
