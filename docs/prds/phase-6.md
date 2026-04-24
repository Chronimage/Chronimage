# Phase 6 · Launch infrastructure + distribution

> Sign, ship, publish. The credential-gated half of release hardening: acquire the EV cert, generate real Tauri updater keys, provision `chronimage.app` + `docs.chronimage.app` + `releases.chronimage.app` on Cloudflare Pages, wire Sentry, stand up the Insider signup form, submit to the Microsoft Store. The phase where v1.0.0 leaves the local branch and lands on real users' machines.

## Context

[Phase 5](./phase-5.md) shipped the in-code release-hardening scaffolding: Ed25519 license verification, telemetry opt-in, dual MIT/Apache-2.0 licence, governance docs, operational runbook. Everything from Phase 5 that's gated on an external account — buying a cert, registering domains, configuring Sentry, publishing a marketing site — lives here instead. Split like this because the two halves have wildly different velocities: Phase 5 iterates in hours, Phase 6 in days-to-weeks (EV validation alone is 1–2 weeks of turnaround).

No code is blocking Phase 6; all CI/CD workflows from Phase 0 are wired and dry-run-green. Phase 6 is mostly configuration, secret management, and the external-facing web surfaces that turn a working desktop app into a shipping product.

## Personas & stories

- **Jay (shipping v1.0.0)**
  - As Jay, I can push `git tag v1.0.0` on `main` and have a signed MSI + updater manifest + press-ready marketing page live within 15 minutes.
  - As Jay, I can invite a friend to the Insider channel by pasting their email into a signup form, hitting approve, and the app self-validates the signed `license.json` they receive via email.
  - As Jay, a production crash sends an opt-in PII-scrubbed Sentry event so I can fix it without pestering the user.

- **First-wave users**
  - As a user, I download from chronimage.app, install a signed MSI with no SmartScreen scare, and the app auto-updates quietly on next launch.
  - As a user, I read clear docs at docs.chronimage.app about import / cull / develop / source-cleanup flows.
  - As a user, I can file a bug at github.com/Chronimage/chronimage/issues and the template asks me the right questions.

- **Insider testers**
  - As an Insider, I receive a signed `license.json` via email, drop it into `%APPDATA%\Chronimage`, and the app flips to the Insider update channel on next launch.

## Must-have deliverables

### 1. Code signing
- [ ] Acquire EV code-signing cert (DigiCert / Sectigo / SSL.com — one-time ~$400 / 3 years for personal)
- [ ] Store as GitHub Actions encrypted secrets `WINDOWS_SIGN_CERT_PFX_B64` + `WINDOWS_SIGN_CERT_PWD`
- [ ] Self-signed dev cert for nightly: SmartScreen warns but internal testers accept
- [ ] `release-stable.yml` + `release-beta.yml` + `insider.yml` use EV cert; `nightly.yml` uses dev cert
- [ ] First smoke test: push `v0.99.0-rc.1` on a test branch → verify signed MSI loads without warnings on a clean Windows 11 VM
- [ ] Signing step runs in < 60 s so the tag-to-MSI clock still beats 15 min

### 2. Tauri updater key rotation + manifest hosting
- [ ] `pnpm exec tauri signer generate` for three keypairs (stable+beta, nightly, insider)
- [ ] Public keys replace `REPLACE_WITH_TAURI_UPDATER_PUBKEY` in `tauri.conf.json` + the three channel configs
- [ ] Private keys live as `TAURI_UPDATER_PRIVATE_KEY` + `TAURI_UPDATER_PWD` (with `_INSIDER` suffix for the Insider key)
- [ ] Rotation policy documented: rotate per major version; public-key history at `releases.chronimage.app/pubkeys/history.txt`
- [ ] Cloudflare Pages project `chronimage-releases` with custom domain `releases.chronimage.app`
- [ ] `scripts/publish-updater-manifest.cjs` wired to `CF_API_TOKEN` + `CF_ACCOUNT_ID` + `CF_PAGES_PROJECT` secrets
- [ ] Four manifests live: `stable.json`, `beta.json`, `nightly.json`, `insider.json`
- [ ] Insider manifest consumed only by app instances whose `license_state` has `plan = 'insider'` + valid signature (plumbing is in Phase 5; this wires it end-to-end)

### 3. Sentry crash reporting (opt-in)
- [ ] Sentry project + DSN; DSN stored as encrypted secret
- [ ] Flip `src-tauri/src/telemetry.rs::event()` from no-op to Sentry-backed, gated by the `telemetry.enabled` setting (already default-off from Phase 5)
- [ ] First-run consent dialog: "Help improve Chronimage by sending anonymous crash reports?" (wired into Settings → Privacy)
- [ ] PII scrubber: strip file paths, photo filenames + sha256s, camera EXIF (serials), GPS coordinates — anything that could deanonymise
- [ ] Release tag = git tag; commit hash propagated so Sentry deep-links to source
- [ ] `diagnostic_report` command — user-initiated crash-report bundle (runs locally, lets the user inspect + confirm before sending)
- [ ] Sample rate: 1.0 for nightly; 0.2 for stable once volume is known

### 4. Documentation site (`docs.chronimage.app`)
- [ ] mdBook project at `docs-site/` with sections:
  - **Getting started** — install, first import, naming faces
  - **Source connectors** — Google Photos, iCloud, iPhone, local, NAS
  - **Source cleanup** — safe-delete dashboard, audit log, per-provider instructions
  - **Culling** — Compare/Grid/Swipe, shortcuts, Cull Bin
  - **Develop** — sliders, curves, masks (Phase 3+), presets, copy-paste-sync
  - **Prompt editing** — Flux vs SDXL, sidecar config, constraints, SAM2 masks
  - **Map + rediscovery** — trip clustering, "on this day", reverse geocoder
  - **Privacy & data** — on-device models, telemetry, encrypted face DB, no cloud sync
  - **Troubleshooting** — model downloads, GPU minimums, sync folder paths
  - **Release notes** — auto-generated from `CHANGELOG.md`
- [ ] Cloudflare Pages deploy at `docs.chronimage.app`
- [ ] Search: mdBook built-in FTS; target < 200 ms response

### 5. Marketing landing (`chronimage.app`)
- [ ] Static site at `website/` (Astro preferred; plain HTML fallback)
- [ ] Sections: hero + pitch, "the problem" (fragmented libraries), "how it works" (screenshots), "on-device AI" (privacy story), download (stable MSI + checksum), feature overview, pricing ("free today; optional paid tiers later"), FAQ, press + contact
- [ ] Newsletter signup (Buttondown — founder-friendly, markdown-native, ~$9/mo)
- [ ] Analytics: Plausible (cookie-less, GDPR-safe). No GA.
- [ ] Cloudflare Pages deploy at `chronimage.app`; Lighthouse performance ≥ 95
- [ ] Accessibility: axe-core clean; keyboard-navigable

### 6. Insider program
- [ ] Signup form at `chronimage.app/insider` — collects email only (name optional); writes to Airtable / Notion / a small Cloudflare Worker KV
- [ ] Manual approval workflow: maintainer reviews signup, runs `scripts/issue-insider-license.cjs <email>` which produces a signed `license.json`
- [ ] Approval email (templated via Buttondown transactional): license.json attached, install instructions linked to `docs.chronimage.app/insider`
- [ ] Ed25519 signing key pair stored in 1Password / Bitwarden + `INSIDER_SIGNING_KEY` secret for CI scripts
- [ ] Public key replaces the placeholder `INSIDER_PUBKEY_BYTES` in `src-tauri/src/license/mod.rs`
- [ ] Insider builds emit opt-in diagnostic telemetry (with a separate consent toggle)
- [ ] First 100 approved manually; after that review the flow + decide whether to auto-approve (lean manual indefinitely — signal-to-noise matters more than scale)

### 7. Branch protection + merge discipline
- [ ] `main`: require PR + 1 approval + all CI green + linear history (squash merges only) + signed commits
- [ ] `develop`: require PR + all CI green + linear history (squash merges only)
- [ ] Delete branch on merge
- [ ] Auto-cancel superseded CI runs (already wired in `ci.yml` concurrency group)
- [ ] `CODEOWNERS` populated once contributors + team form; one-person-repo for the first cohort

### 8. Distribution channels beyond GitHub Releases
- [ ] Microsoft Store submission (one-time ~$99 developer fee + review time)
  - Same MSI + updater manifest pipeline
  - Store builds disable `tauri-plugin-updater` (Store handles updates)
- [ ] WinGet manifest PR to `microsoft/winget-pkgs` (auto-triggered per stable release via `scripts/publish-winget.cjs`)
- [ ] Chocolatey + Scoop manifests (community-maintained; open the PR templates so others can pick them up)

## Non-goals

- macOS / Linux ports (Phase 10+ speculative; depends on demand)
- Mobile companion app (Phase 8 speculative; absorbs the post-v1 responsive pass from ADR 0007)
- Paid tier UI / pricing pages (entitlement scaffolding exists from Phase 5; the actual "add to cart" flow is a business decision, not a release deliverable)
- User accounts / cloud sync (user has explicitly rejected this)

## Non-functional requirements

- **Release pipeline**: tag push → signed MSI + updater manifest live on `releases.chronimage.app` ≤ 15 minutes
- **Auto-update**: detect + download + restart ≤ 3 minutes on a 50 Mbps connection
- **SmartScreen**: signed with EV cert = no warning on a clean Windows 11 install; if we ship OV first, the runbook covers the "wait for reputation" fallback
- **Sentry scrubbing**: sampled events contain zero file paths + zero photo metadata
- **Docs search**: < 200 ms response time
- **Marketing site**: Lighthouse performance ≥ 95; axe-core a11y clean
- **Updater verification**: manifest signature must verify against the embedded public key before the MSI is downloaded; bad signatures emit a `chronimage://update-rejected` event and abort

## Schema changes

None. Phase 5's migration 20261201 already added `license_state` + the `telemetry.enabled` setting, which is everything the launch pass needs from the catalog.

## Entitlements

Phase 5 shipped the Ed25519 verification machinery. Phase 6 finally swaps `INSIDER_PUBKEY_BYTES` from the placeholder `[0u8; 32]` to the real key and flips `Entitlements::current()` from all-true-community to:

```rust
pub fn current() -> &'static Entitlements {
    CURRENT.get_or_init(|| match load_license() {
        Ok(lic) if lic.is_valid_insider() => Entitlements { insider_updates: true, ..defaults() },
        Ok(lic) if lic.plan == "pro" => Entitlements { ..all_true() },  // Phase 7+
        _ => Entitlements { insider_updates: false, ..community_defaults() },
    })
}
```

`community_defaults()` equals `all_true_except_insider` for v1.0.0 so no feature hides behind a paywall at launch. The machinery is in place to flip any feature to Pro-only in a patch release without code changes elsewhere.

## Exit criteria (test-bound)

- [ ] `tests/e2e/phase-6-tag-to-msi.spec.ts` (CI-only) — push `v0.99.0-rc.1` on a test branch; poll GitHub API + Cloudflare; signed MSI + updater manifest live within 15 min
- [ ] `tests/e2e/phase-6-updater-flow.spec.ts` — simulated newer manifest → app downloads + restarts + reports new version on next launch
- [ ] `tests/e2e/phase-6-insider-license.spec.ts` — signed `license.json` produced by `issue-insider-license.cjs` verifies against the app's embedded public key; tampered version rejected
- [ ] `tests/docs/phase-6-docs-deploy.spec.ts` — mdBook builds; all internal links resolve; search index populated
- [ ] `tests/e2e/phase-6-sentry-scrub.spec.ts` — test event from a staging build appears in Sentry with zero file paths / photo metadata
- [ ] Manual: install signed MSI on clean Windows 11 VM → no SmartScreen warning
- [ ] Manual: download from `chronimage.app/download` → byte-identical match with GitHub Releases checksum
- [ ] Manual: marketing site Lighthouse performance ≥ 95 + axe-core a11y clean

## Open questions

- **OV first, EV later, or EV from day one?** OV is ~$80/yr and fails SmartScreen until reputation builds (usually 1–4 weeks). EV is $400/3yr and has instant reputation. Lean **EV from day one** — friction on install is the single biggest drop-off point for non-technical users.
- **Newsletter service**: ✅ **Buttondown** (~$9/mo, founder-friendly, markdown-native, no tracking pixels by default). Plausible Mail is tempting but still young.
- **Microsoft Store submission**: ship-in-parallel-with-stable or ship-after-first-stable? Lean **after** — Store review is 1–2 weeks and we don't want to gate the GitHub release on it. First Store build is Phase 6.1.
- **Insider keypair storage**: 1Password private vault vs Bitwarden vs a Cloudflare secret? Lean **1Password** (Jay's primary) with the private key also pulled into `INSIDER_SIGNING_KEY` GitHub Actions secret for the issue-license script; never commit, never log.
- **PGP key for security disclosures**: generate now or wait for the first report? Lean **now** — `SECURITY.md` links to a placeholder that becomes embarrassing fast. Publish the fingerprint at `chronimage.app/security/pgp.txt` during the docs-site deploy.

## TODO log

### Credentials + accounts (lead time 1–4 weeks)
- [ ] Register `chronimage.app` domain (Cloudflare Registrar; ~$10/yr)
- [ ] EV code-signing cert (DigiCert — 1–2 week validation)
- [ ] Cloudflare account + 3 Pages projects (`chronimage-app`, `chronimage-docs`, `chronimage-releases`)
- [ ] Sentry project + DSN + team member invite
- [ ] Buttondown account + newsletter setup
- [ ] Microsoft Partner Center developer account + MSA
- [ ] PGP keypair for `security@chronimage.app`

### Code + config
- [ ] `scripts/publish-updater-manifest.cjs` (currently stub) wired to CF API
- [ ] `scripts/issue-insider-license.cjs` — signs a `license.json` via `ed25519-dalek` + the Insider private key
- [ ] `scripts/publish-winget.cjs` — opens PRs on `microsoft/winget-pkgs` per stable release
- [ ] Swap `INSIDER_PUBKEY_BYTES` placeholder for the real public key
- [ ] Swap placeholder `REPLACE_WITH_TAURI_UPDATER_PUBKEY` in `tauri.conf.json` × 4 channels
- [ ] Sentry backend in `telemetry.rs` + PII scrubber config
- [ ] First-run consent dialog in Settings → Privacy
- [ ] mdBook scaffold at `docs-site/` + content drafts for the 10 sections
- [ ] Astro scaffold at `website/` + section drafts
- [ ] GitHub branch-protection rules on `main` + `develop`
- [ ] `CODEOWNERS` (placeholder — review quarterly)
- [ ] PGP fingerprint + key URL added to `SECURITY.md`
- [ ] 5 exit-criterion test files (3 e2e + 1 docs + 1 manual checklist)

### Go-live
- [ ] Tag `v1.0.0-rc.1` → smoke-test the pipeline end-to-end
- [ ] Tag `v1.0.0` → marketing site goes live, Buttondown announcement, HackerNews post
- [ ] Open the first 10 Insider invites
- [ ] 2-week soak with Insider channel before the Microsoft Store submission

---

## After Phase 6 — what's next?

v1.0.0 shipped. Chronimage is a complete product. Further phases are demand-driven:

- **Phase 7 (speculative): cloud backup**, user-BYO (S3/R2/B2) — opt-in; encrypted at rest with a user-held key.
- **Phase 8 (speculative): mobile companion** — iOS/Android, shoots directly to Chronimage over LAN/Tailscale. Absorbs the post-v1 mobile responsive pass from ADR 0007.
- **Phase 9 (speculative): shared albums** — LAN-based URL sharing, no cloud middleman.
- **Phase 10 (speculative): macOS + Linux ports** — once demand materialises.
- **Phase 11 (speculative): collaboration / team libraries** — if the user base asks.
