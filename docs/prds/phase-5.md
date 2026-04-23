# Phase 5 · Release hardening + public launch

> Sign it, ship it, support it. Flip every Phase 0 CI/CD switch that was stubbed, acquire the EV cert, publish updater manifests, stand up a marketing site + docs, open-source the repo (or don't, user decision), and start the insider program.

## Context

Phase 0 built the CI/CD scaffolding (7 workflows, 4 release channels, lefthook, conventional commits, signing infrastructure) but everything was tested in dry-run mode without real artifacts. Phases 1–4 shipped the product. Phase 5 is the final mile: sign the binaries with a real cert, point a real domain at Cloudflare Pages, publish to the Microsoft Store (optional), open the repo (optional), and ship v1.0.0 stable to actual users.

This is also where the entitlement scaffolding becomes real: the `Entitlements` module still returns all-true, but `license.json` infrastructure gets a real signing key, and the Insider channel gets its first invitees.

## Personas & stories

- **Jay (launching v1.0.0 to early adopters)**
  - As Jay, I can push `git tag v1.0.0` on `main` and have a signed MSI on GitHub Releases + the stable updater manifest live within 15 minutes, no manual steps.
  - As Jay, I can invite a friend to the Insider channel by sending them a signed `license.json` file; the app self-validates it.
  - As Jay, a crash in production sends an opt-in Sentry event so I can fix it without pestering the user.

- **First wave of users (the friend Jay invited + Hacker News commenters)**
  - As a user, I download from chronimage.app, install a signed MSI with no SmartScreen scare, and the app auto-updates quietly.
  - As a user, I read clear docs on chronimage.app/docs about how import + cull + develop + source-cleanup work.
  - As a user, I can file a bug at github.com/Chronimage/chronimage/issues with a template that asks the right questions.

- **Insider testers (private)**
  - As an Insider, I opt into beta features that aren't yet in stable (e.g., Phase 4 prompt-edit if it's still maturing), and my app validates my license before accepting `insider.json` manifests.

## Must-have deliverables

### 1. Code signing
- [ ] Acquire an EV code-signing certificate (DigiCert / Sectigo / SSL.com — one-time ~$400 for 3 years for personal)
- [ ] Store as GitHub Actions encrypted secret `WINDOWS_SIGN_CERT_PFX_B64` + `WINDOWS_SIGN_CERT_PWD`
- [ ] Non-EV dev cert for nightly: self-signed, warns SmartScreen (acceptable for internal testers)
- [ ] `release-stable.yml` + `release-beta.yml` + `insider.yml` all use EV cert; `nightly.yml` uses dev cert
- [ ] First test: push `v1.0.0-rc.1` on a test branch → verify signed MSI loads without warnings on a clean Windows VM

### 2. Tauri updater key rotation
- [ ] `pnpm exec tauri signer generate` produces a real public/private key pair
- [ ] Public key replaces `REPLACE_WITH_TAURI_UPDATER_PUBKEY` in `tauri.conf.json` + the three channel configs
- [ ] Private key stored as `TAURI_UPDATER_PRIVATE_KEY` + `TAURI_UPDATER_PWD` secrets
- [ ] Separate key pair for Insider channel: `TAURI_UPDATER_PRIVATE_KEY_INSIDER` + `TAURI_UPDATER_PWD_INSIDER`
- [ ] Document rotation policy: rotate per major version; publish public-key history at `releases.chronimage.app/pubkeys.txt`

### 3. Updater manifest hosting
- [ ] Cloudflare Pages project `chronimage-releases` with custom domain `releases.chronimage.app`
- [ ] Deploy `scripts/publish-updater-manifest.cjs` wired to `CF_API_TOKEN` + `CF_ACCOUNT_ID` + `CF_PAGES_PROJECT` secrets
- [ ] Four manifests live: `stable.json`, `beta.json`, `nightly.json`, `insider.json`
- [ ] Insider manifest requires a valid license check in-app before it's consumed (see Insider program below)

### 4. Sentry crash reporting (opt-in)
- [ ] `src-tauri/src/telemetry.rs` — flip `event()` no-op to Sentry-backed impl, gated by `telemetry.enabled` setting (default false)
- [ ] First-run dialog: "Help improve Chronimage by sending anonymous crash reports? (toggle any time in Settings)"
- [ ] Sentry DSN stored as secret; only accessed if user opts in
- [ ] PII scrubber: strip file paths, photo metadata, anything that could identify a user
- [ ] Release tag = git tag; commit hash propagated so Sentry links to source

### 5. Documentation site (`docs.chronimage.app`)
- [ ] mdBook at `docs-site/` with sections:
  - **Getting started** — install, first import, naming faces
  - **Source connectors** — how to connect each of Google Photos / iCloud / iPhone / local / NAS
  - **Source cleanup** — the safe-delete dashboard, per-source instructions, audit log
  - **Culling** — Compare/Grid/Swipe, keyboard shortcuts, Cull Bin, restore/empty
  - **Develop** — sliders, curves, masks, presets, copy-paste-sync
  - **Prompt editing** — constraints, Flux vs SDXL, cost/time
  - **Map + rediscovery** — how trips cluster, "on this day"
  - **Privacy & data** — on-device models, no telemetry in v1, encrypted face DB
  - **Troubleshooting** — sync folder paths, model download stalls, GPU driver minimums
  - **Release notes** — auto-generated from `CHANGELOG.md`
- [ ] Deployed to Cloudflare Pages at `docs.chronimage.app`
- [ ] Searchable (mdbook built-in FTS)

### 6. Marketing landing (`chronimage.app`)
- [ ] Static site (Astro or plain HTML) at `website/`
- [ ] Sections: hero + pitch, "the problem" (fragmented libraries), "how it works" (screenshots), "on-device AI" (privacy story), download (stable MSI), features overview, pricing ("free today; optional paid tiers later"), FAQ, press / contact
- [ ] Newsletter signup (Buttondown / Plausible Mail — privacy-friendly)
- [ ] Deployed to Cloudflare Pages at `chronimage.app`
- [ ] GA replaced with Plausible (cookie-less, GDPR-safe)

### 7. Insider program
- [ ] Signup form on `chronimage.app/insider` → collects email → manually approves → emails back a signed `license.json`
- [ ] License format: `{ plan: "insider", email, issued_at, expires_at, signature }` — Ed25519-signed via the insider private key
- [ ] App reads `license.json` from `{app_data}/license.json` on launch; verifies signature; if valid and `plan == "insider"`, enables `Entitlements::insider_updates = true` and consumes `insider.json` manifest
- [ ] Insider build emits opt-in crash reports + additional diagnostic telemetry (with user consent)

### 8. Repository hygiene
- [ ] Decide: open-source with MIT/Apache-2.0/AGPL? Or source-available (BSL)? User-level decision.
- [ ] Fill `LICENSE` accordingly
- [ ] Write `CONTRIBUTING.md` with PR process, branch strategy, style guide, how to run the test suite
- [ ] Write `SECURITY.md` with disclosure email + PGP key
- [ ] Issue templates: bug report, feature request, security (encrypted)
- [ ] PR template: summary, test plan, migration notes, screenshots
- [ ] Populate `.github/CODEOWNERS` with real reviewers once team forms

### 9. Branch protection + merge discipline
- [ ] `main`: require PR + 1 approval + all CI green + linear history (squash merges only) + signed commits
- [ ] `develop`: require PR + all CI green + linear history (squash merges only)
- [ ] Delete branch on merge
- [ ] Auto-cancel superseded CI runs (already in `ci.yml` concurrency group)

### 10. Distribution channels beyond GitHub
- [ ] Microsoft Store submission (optional; quotes ~$99 one-time developer fee + review time)
  - Uses the same MSI + updater manifest pipeline
  - Store version has its own updater endpoint (disable `tauri-plugin-updater` for Store builds; Store handles updates)
- [ ] WinGet manifest PR to `microsoft/winget-pkgs`
- [ ] Chocolatey / Scoop manifests (community maintainers welcome)

### 11. Operations runbook
- [ ] `docs/runbook.md` — what to do when:
  - Signed MSI fails SmartScreen (usually: wait for reputation to build, or submit to MS for review)
  - Auto-updater returns 404 (usually: Cloudflare Pages stale cache; purge zone)
  - Sentry flooded (throttle rules in Sentry project)
  - Key compromise (rotate Tauri updater key, push new `pubkey.txt`, major version bump)

## Non-goals

- macOS / Linux ports (deferred; track user demand first)
- Mobile companion app (separate Phase 6+)
- Paid tier UI (entitlement scaffolding exists; actual pricing page is a business decision)
- User accounts / cloud sync (user explicitly rejects this)

## Non-functional requirements

- Release pipeline: tag push → signed MSI + updater manifest deployed → visible on chronimage.app ≤ 15 minutes
- Auto-update detection + download + restart ≤ 3 minutes on a 50 Mbps connection
- SmartScreen: signed with EV cert = no warning on clean Windows install
- Sentry scrubbing: zero file paths / photo metadata present in sampled events
- Docs site search: < 200 ms response time
- Marketing site: Lighthouse performance ≥ 95

## Schema changes

Minimal. One field for license state:

```sql
-- src-tauri/migrations/20261201000000_phase5_release.sql

CREATE TABLE IF NOT EXISTS license_state (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  plan            TEXT    NOT NULL DEFAULT 'community',
  email           TEXT,
  issued_at       TEXT,
  expires_at      TEXT,
  signature       TEXT,
  verified_at     TEXT,
  last_checked_at TEXT
);

INSERT OR IGNORE INTO license_state(id, plan) VALUES (1, 'community');

INSERT OR REPLACE INTO settings(key, value, updated_at)
VALUES ('schema_version', '6', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
```

## API surface (new commands)

- `async fn license_load() -> Result<LicenseState>`
- `async fn license_import(path: PathBuf) -> Result<LicenseState>`  — user drag-drops `license.json`
- `async fn license_clear() -> Result<()>`
- `async fn telemetry_opt_in(enabled: bool) -> Result<()>`
- `async fn diagnostic_report() -> Result<DiagnosticReport>` — user-initiated crash-report bundle (runs locally, offers to send)

## Entitlements

This phase is where `Entitlements::current()` stops being all-true:

```rust
pub fn current() -> &'static Entitlements {
    CURRENT.get_or_init(|| match load_license() {
        Ok(lic) if lic.plan == "insider" => Entitlements { insider_updates: true, ..defaults() },
        Ok(lic) if lic.plan == "pro" => Entitlements { ..all_true() },
        _ => Entitlements { insider_updates: false, ..community_defaults() },
    })
}
```

v1.0.0 still ships with `community_defaults() == all_true_except_insider` — but the machinery is in place to flip any feature behind a paywall in a patch release without code changes elsewhere.

## Exit criteria (test-bound)

- [ ] `tests/e2e/phase-5-tag-to-msi.spec.ts` (CI-only) — push `v0.99.0-rc.1` on a test branch, poll GitHub API, confirm signed MSI + updater manifest live within 15 min
- [ ] `src-tauri/tests/phase_5_license_verify.rs` — valid signed license loads; tampered signature rejected; expired license rejected; `community` plan without a license works
- [ ] `src-tauri/tests/phase_5_telemetry_opt_out_default.rs` — fresh install has telemetry.enabled = false; `telemetry::event()` is no-op
- [ ] `tests/e2e/phase-5-updater-flow.spec.ts` — simulated manifest with newer version → app downloads + restarts + reports new version on next launch
- [ ] `tests/docs/phase-5-docs-deploy.spec.ts` — mdBook builds without errors; all internal links resolve; search index populated
- [ ] Manual: install signed MSI on clean Windows 11 VM → no SmartScreen warning
- [ ] Manual: download from chronimage.app/download → file matches GitHub Releases checksum exactly
- [ ] Manual: Sentry test event from staging build → appears in Sentry dashboard with PII scrubbed

## Open questions

- **License model**: strict (per-seat, activation server) or lax (signed JWT, offline verification)? Lean lax for v1 — offline verification, no server required, trust-but-auditable.
- **Open-source vs source-available**: full open-source earns goodwill + contributors but makes monetization harder. BSL (Business Source License) lets us ship source while reserving commercial rights. User-level decision; lean open MIT/Apache-2.0 with a clear "please don't rebrand and resell" norm.
- **Store vs direct**: Microsoft Store simplifies install for non-technical users but 30% revenue cut if we ever go paid. Ship both.
- **Newsletter service**: Buttondown ($9/mo), Plausible Mail (free, basic), MailerLite ($0 for <1k)? Lean Buttondown — founder-friendly, markdown-native.
- **Changelog automation trust**: can we fully trust git-cliff for release notes, or does every release get a human-written "Highlights" section on top? Lean Highlights-on-top for stable; auto-only for beta/nightly.
- **Insider vetting**: approve every Insider signup manually, or auto-approve anyone with a GitHub account? Lean manual for first 100, then automate.

## TODO log

- [ ] Migration `20261201000000_phase5_release.sql`
- [ ] Acquire EV cert (lead time 1–2 weeks after validation)
- [ ] Rotate Tauri updater keys; publish pubkey history
- [ ] Cloudflare Pages project + domain DNS
- [ ] Sentry project + DSN + PII scrubber config
- [ ] mdBook docs site scaffold
- [ ] Marketing site scaffold (Astro preferred; vanilla HTML fallback)
- [ ] LICENSE decision + fill
- [ ] CONTRIBUTING.md + SECURITY.md + issue/PR templates
- [ ] Insider signup form + manual approval workflow
- [ ] Microsoft Store submission (optional)
- [ ] WinGet manifest PR
- [ ] Runbook doc
- [ ] 8 exit-criterion items (5 automated + 3 manual)
- [ ] **Post-v1: mobile / tablet responsive pass** (filed from Phase 2 rehaul · ADR 0007). The Phase 2 rehaul shipped desktop-responsive breakpoints (720–3440 px window widths) but deliberately skipped touch-first UX. After v1 stable:
  - Touch-target sizing (≥ 44 px hit areas on all controls)
  - Sheet-based detail view (swipe-down to dismiss, like iOS Photos)
  - Camera-roll-style gestures (pinch-zoom the masonry for density, long-press for selection)
  - Tauri 2 mobile target (iOS + Android) — currently experimental; track upstream stability before committing
  - Re-validate the masonry packer on sub-720 px widths (may need a 2-column minimum fallback)

---

## After Phase 5 — what's next?

Not this document's scope, but worth noting so planning horizons line up:

- **Phase 6 (speculative): cloud backup**, user-BYO (S3/R2/B2) — opt-in; encrypted at rest with user-held key.
- **Phase 7 (speculative): mobile companion** — iOS/Android, shoots directly to Chronimage over LAN/Tailscale.
- **Phase 8 (speculative): shared albums** — LAN-based URL sharing, no cloud middleman.
- **Phase 9 (speculative): macOS + Linux ports** — once Windows is stable and user demand exists.
- **Phase 10 (speculative): collaboration / team libraries** — if the user base asks for it.

These are not commitments; they're parking spots. The v1.0.0 Chronimage shipped after Phase 5 is already a complete product. Further phases exist because users will ask for them, not because we promised them.
