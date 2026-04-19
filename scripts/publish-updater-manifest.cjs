#!/usr/bin/env node
/*
 * Publish the Tauri updater manifest for a given channel.
 *
 * Usage: node scripts/publish-updater-manifest.js <channel> <tag>
 *
 * Reads the just-built MSI + its .sig file out of
 * src-tauri/target/release/bundle/msi/ and emits a JSON manifest of the shape
 * Tauri's updater expects, then uploads it to Cloudflare Pages / R2.
 *
 * In CI, expects:
 *   CF_API_TOKEN            — Cloudflare API token with Pages:Edit scope
 *   CF_ACCOUNT_ID           — Cloudflare account id
 *   CF_PAGES_PROJECT        — Cloudflare Pages project name (e.g. "chronimage-releases")
 *   GITHUB_REPOSITORY       — owner/repo (from GitHub Actions)
 *
 * Locally, omit the env and the script prints the manifest to stdout.
 */

const { execSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const [, , channel, tag] = process.argv;
if (!channel || !tag) {
  console.error('Usage: node scripts/publish-updater-manifest.js <channel> <tag>');
  process.exit(1);
}
if (!['stable', 'beta', 'nightly', 'insider'].includes(channel)) {
  console.error(`Invalid channel: ${channel}`);
  process.exit(1);
}

const version = tag.replace(/^v/, '');
const bundleDir = path.join('src-tauri', 'target', 'release', 'bundle', 'msi');

function findFirst(pattern) {
  if (!fs.existsSync(bundleDir)) return null;
  const m = fs.readdirSync(bundleDir).find((f) => pattern.test(f));
  return m ? path.join(bundleDir, m) : null;
}

const msi = findFirst(/\.msi$/);
const sigFile = findFirst(/\.msi\.sig$/);
if (!msi || !sigFile) {
  console.error(`Could not find MSI or .sig in ${bundleDir}`);
  process.exit(1);
}

const signature = fs.readFileSync(sigFile, 'utf8').trim();
const repo = process.env.GITHUB_REPOSITORY || 'jamirineni-onedosh/chronimage';
const msiFilename = path.basename(msi);
const url = `https://github.com/${repo}/releases/download/${tag}/${msiFilename}`;

const manifest = {
  version,
  notes: `Chronimage ${version} (${channel} channel).`,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url,
    },
  },
};

const outPath = path.join('releases-manifest', `${channel}.json`);
fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(manifest, null, 2));
console.log(`Wrote ${outPath}`);
console.log(JSON.stringify(manifest, null, 2));

// Publish to Cloudflare Pages when creds are set.
if (process.env.CF_API_TOKEN && process.env.CF_ACCOUNT_ID && process.env.CF_PAGES_PROJECT) {
  const project = process.env.CF_PAGES_PROJECT;
  console.log(`Deploying manifests to Cloudflare Pages project ${project}...`);
  try {
    execSync(`npx -y wrangler@3 pages deploy releases-manifest --project-name=${project} --branch=main`, {
      stdio: 'inherit',
      env: { ...process.env, CLOUDFLARE_API_TOKEN: process.env.CF_API_TOKEN },
    });
  } catch (err) {
    console.error('Cloudflare Pages deploy failed:', err?.message ?? err);
    process.exit(1);
  }
} else {
  console.log('No CF creds set — skipping remote publish. Manifest printed above.');
}
