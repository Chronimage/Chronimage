# Operations runbook

> What to do when things break after shipping. Written for the
> maintainer; contributors should read the [`SECURITY.md`](../SECURITY.md)
> + [`CONTRIBUTING.md`](../CONTRIBUTING.md) flows first.

This runbook covers the operational edges of Chronimage's Phase 5
release infrastructure. When you're paging yourself at 2am, come here
first.

---

## Signed MSI fails SmartScreen on install

**Symptom**: users report "Windows protected your PC" dialogs on a
fresh install of a stable release.

**Most likely cause**: SmartScreen reputation hasn't built up for this
specific signer / file hash yet. Usually resolves itself within days
as more users download the signed artifact.

**Playbook**:

1. Confirm the MSI is actually signed: run `signtool verify /pa
   path\to\Chronimage-x.y.z.msi` on Windows. Expect "Successfully
   verified". If it fails, the signing step in
   `release-stable.yml` broke — check the workflow log for
   `WINDOWS_SIGN_CERT_PFX_B64` / `WINDOWS_SIGN_CERT_PWD` errors.
2. If signing worked but SmartScreen still warns:
   - Submit the MSI to Microsoft for review at
     `https://www.microsoft.com/en-us/wdsi/filesubmission`. Choose
     "Not a threat". Response is usually 24–72 hours.
   - Post a pinned GitHub discussion pointing users at the
     workaround: More info → Run anyway.
3. Medium-term: buy an EV cert (we currently ship with OV). EV gets
   instant SmartScreen reputation. Cost is ~$400 for 3 years. See
   `docs/manual-setup.md` for the purchase workflow.

---

## Auto-updater returns 404

**Symptom**: in-app updater logs `Error: HTTP 404` when checking for
a new release. Users stop getting updates.

**Most likely cause**: the Cloudflare Pages deploy that publishes
`releases.chronimage.app/{channel}.json` didn't run, or the manifest
path changed.

**Playbook**:

1. Hit the manifest URL directly in a browser:
   `https://releases.chronimage.app/stable.json`. Should return JSON
   with `version`, `platforms`, `signature`.
2. If it's a Cloudflare 404: go to Cloudflare Pages → `chronimage-releases`
   project → check latest deployment. Re-run if it failed.
3. If the deploy ran but the manifest is stale: Pages cache. Purge the
   zone: Cloudflare dashboard → `chronimage.app` zone → Caching →
   Purge Everything. Propagates globally in ~30 seconds.
4. If the manifest is missing entirely: the
   `scripts/publish-updater-manifest.cjs` step in `release-stable.yml`
   didn't run. Check the workflow log for `CF_API_TOKEN` /
   `CF_ACCOUNT_ID` errors.

---

## Sentry flooded / rate-limited

**Symptom**: Sentry emails "event quota exceeded" or the project
dashboard shows a flood of duplicate errors.

**Playbook**:

1. Open the flooding event. If it's the same stack from many users,
   it's a real bug — triage it, open an issue, ship a hotfix.
2. If it's a single user looping: add them to the project's
   **Inbound Filters** → **Clients** list (IP-based). Reach out via
   the email on their opt-in consent record.
3. If it's an environment-specific storm (nightly channel, specific
   OS), scope the inbound filter to that `environment` tag.
4. Long-term: tune the scrubber's sample rate in
   `src-tauri/src/telemetry.rs`. Default is 1.0 (100%); drop to 0.2
   for stable once volume is stable.

---

## Tauri updater key compromise

**Symptom**: Signing key material is exposed (committed to git,
leaked via CI logs, etc.).

**This is urgent.** A bad actor can sign a malicious updater manifest
and push it to every installed Chronimage.

**Playbook**:

1. Revoke immediately:
   - Rotate `TAURI_UPDATER_PRIVATE_KEY` + `TAURI_UPDATER_PWD` secrets in
     GitHub Actions.
   - Generate a new keypair: `pnpm exec tauri signer generate`.
   - Replace `pubkey` in `tauri.conf.json` + the three channel configs.
2. Bump the major version (`v2.0.0`). Old installs will refuse to
   auto-update to the new key; users must manually install the v2 MSI.
3. Publish the old public key to
   `releases.chronimage.app/pubkeys/revoked/{date}.txt` so audit
   tooling can detect manifests signed with it.
4. Post a security advisory at
   `github.com/Chronimage/chronimage/security/advisories`. Reference
   the rotation in the release notes for the new major.

---

## License signing key compromise (Insider channel)

**Symptom**: `INSIDER_PUBKEY_BYTES` or its private counterpart leaked.

**Playbook**:

1. Rotate the Insider Ed25519 keypair; update
   `src-tauri/src/license/mod.rs::INSIDER_PUBKEY_BYTES` to the new
   public bytes.
2. Ship a patch release. Existing Insider licences continue to verify
   locally until they expire; the new app build rejects anything
   signed by the old key on next launch.
3. Re-issue licences for all active Insiders (they drop the new
   `license.json` in place of the old one).
4. Publish the rotation in the next release notes with the rotation
   date so auditors can date-correlate.

---

## CI is red on `develop` for > 1 hour

**Symptom**: every PR that lands on `develop` fails CI, but the diff
looks unrelated.

**Most likely cause**: a dependency bump (Dependabot PR) got merged
that breaks the build, or a flaky test wormed past.

**Playbook**:

1. Check the CI logs for the first red run. If it's a dep bump, revert
   the merge commit — don't try to patch forward under time pressure.
2. If it's a specific test: mark it `#[ignore]` with a `// TODO:
   <issue>` comment pointing at a fresh GitHub issue. Unblock `develop`
   first, then fix the test.
3. Never disable CI gates to land a fix. If the fix itself is urgent,
   branch off `main` as a hotfix and merge to `main` directly (with an
   `[hotfix]` subject line — allowed by commitlint for the `hotfix/*`
   branch prefix).

---

## A user accidentally deleted from the source (cloud cleanup gone wrong)

**Symptom**: user reports photos missing from Google Photos / OneDrive
after using Chronimage's source-cleanup dashboard.

**Playbook**:

1. Ask the user for their **cleanup audit log**: Settings → Sources →
   {provider} → "Download audit log". JSONL; one row per action.
2. Cross-reference the row's `verified_sha256` field with the
   `source_copies` table — we only delete when the local sha matches
   what was in cloud. If the hash matches, the photo is recoverable
   from their local catalog.
3. For Google Photos: photos deleted via our cleanup go to the user's
   **Recently deleted** folder for 60 days. Recoverable in-product.
4. For OneDrive: same — 30-day Recycle Bin. Recoverable.
5. If the user emptied those too: Chronimage can't recover. Apologise
   and walk them through restoring from their local catalog.
6. Post-mortem: read the audit log for the session and confirm every
   two-step gate fired. If any gate was skipped, file a bug (it
   shouldn't be possible).

---

## When in doubt

- Check Sentry for recent error volume.
- Check Grafana dashboards for release-pipeline metrics.
- Check the #chronimage-ops private Slack (if it exists yet).
- Post a status update on `status.chronimage.app` before anything else
  so users know you're aware.
