# Phase 5 · Release hardening (in-code) · **closed 2026-04-24**

> Ed25519 license verification + telemetry opt-in plumbing + governance docs + runbook. The in-code half of the release-hardening workstream. The account-provisioning half (EV cert, Cloudflare Pages, Sentry DSN, domain DNS, Microsoft Store) moves to [Phase 6 · Launch infrastructure](./phase-6.md) where it belongs with the rest of the credential-gated work.

## Context

Phase 0 built the CI/CD scaffolding (7 workflows, 4 release channels, lefthook, conventional commits, signing infrastructure) but everything was tested in dry-run mode without real artifacts. Phases 1–4 shipped the product. Phase 5 was originally scoped as "sign it, ship it" — but the two halves have wildly different velocities: in-code work iterates in hours, credential work in days-to-weeks (EV cert validation lead time alone is 1–2 weeks). Splitting them into 5 (in-code) and 6 (launch) lets the code land + soak on `develop` while the external accounts get provisioned in parallel.

Phase 5 shipped the machinery: `license_state` + Ed25519 verification, `telemetry.enabled` default-off, dual MIT/Apache-2.0 license with full texts, CONTRIBUTING / SECURITY / issue + PR templates, and a 7-playbook operational runbook. The `Entitlements` module still returns all-true for v1; Phase 6 flips the Insider gate once the signing key + Insider signup form are live.

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

## Must-have deliverables (shipped)

### 4. Telemetry opt-in — **shipped**
- [x] `telemetry.enabled` setting written to `'0'` by migration `20261201000000_phase5_release.sql`; fresh installs default off
- [x] `telemetry_get` + `telemetry_opt_in` commands + TS wrappers (a first-run dialog in Settings is Phase 6 surface work)
- [x] `src-tauri/tests/phase_5_telemetry_opt_out_default.rs` — 2 integration tests (default off + opt-in persists)
- [ ] Sentry-backed `event()` implementation + PII scrubber — **moved to Phase 6 §3** (needs DSN)
- [ ] First-run consent dialog — **moved to Phase 6 §3** (ships alongside the live Sentry backend)

### 7. Insider license verification — **shipped (client half)**
- [x] `license_state` single-row table (migration `20261201`, schema v8)
- [x] `src-tauri/src/license/mod.rs` — Ed25519 signature verification against the embedded `INSIDER_PUBKEY_BYTES` placeholder; typed `LicenseError` (Parse / UnknownPlan / BadSignatureEncoding / BadSignatureLength / SignatureMismatch / Expired / BadTimestamp)
- [x] `license_load` + `license_import` + `license_clear` commands
- [x] Canonical signable message `{plan}|{email}|{issued_at}|{expires_at}` so any tool (minisign, curl) can issue licences
- [x] 7 unit tests + `phase_5_license_verify.rs` (5 integration tests covering tamper, expiry, unknown plan, community default, round-trip, clear)
- [ ] Real Ed25519 signing keypair + signup form + manual-approval email flow — **moved to Phase 6 §5** (needs keypair gen + a public signup URL)

### 8. Repository hygiene — **shipped**
- [x] `LICENSE` flipped to dual `MIT OR Apache-2.0`; `LICENSE-MIT` + `LICENSE-APACHE` hold the full texts (Rust-community convention)
- [x] `CONTRIBUTING.md` with branch strategy, style rules, gauntlet, PR checklist
- [x] `SECURITY.md` with private disclosure flow (email placeholder until Phase 6 publishes the PGP fingerprint)
- [x] `.github/ISSUE_TEMPLATE/{bug_report,feature_request,config}.yml` + `.github/pull_request_template.md`
- [ ] `.github/CODEOWNERS` real reviewers — **deferred** until a team forms; one-person-repo for now

### 11. Operations runbook — **shipped**
- [x] `docs/runbook.md` — 7 playbooks: SmartScreen, updater 404, Sentry flood, Tauri key rotation, Insider license key rotation, CI red > 1 hour, source-cleanup regression

## Moved to Phase 6 · Launch infrastructure

Everything in Phase 5 that needs an external account / credential / domain has moved to [Phase 6](./phase-6.md) so the in-code work here could close cleanly. The short list, with the § in the new PRD:

- §1 EV code-signing cert + CI secret wiring → **Phase 6 §1**
- §2 Real Tauri updater keypair + rotation policy → **Phase 6 §2**
- §3 Updater manifest hosting on Cloudflare Pages → **Phase 6 §2**
- §4 Sentry DSN + backend + first-run consent dialog → **Phase 6 §3**
- §5 mdBook documentation site + `docs.chronimage.app` deploy → **Phase 6 §4**
- §6 Marketing landing + `chronimage.app` deploy → **Phase 6 §5**
- §7 Insider signup form + manual approval + live signing key → **Phase 6 §6**
- §9 Branch protection + merge discipline (needs GitHub admin config) → **Phase 6 §7**
- §10 Microsoft Store + WinGet + Chocolatey submissions → **Phase 6 §8**

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
```

> Note: sqlx migration metadata is the schema source of truth; the old `settings.schema_version` breadcrumb was removed during the reset cleanup.

## API surface (shipped commands)

- `async fn license_load() -> Result<LicenseState>` ✅
- `async fn license_import(path: String) -> Result<LicenseState>` ✅ — user drag-drops or picks `license.json`
- `async fn license_clear() -> Result<()>` ✅
- `async fn telemetry_get() -> Result<bool>` ✅
- `async fn telemetry_opt_in(enabled: bool) -> Result<()>` ✅
- `async fn diagnostic_report() -> Result<DiagnosticReport>` — **moved to Phase 6 §3** (meaningful only once Sentry is live)

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

Shipped in Phase 5:

- [x] `src-tauri/tests/phase_5_license_verify.rs` — valid signed license loads; tampered signature rejected; expired license rejected; `community` plan without a license works (5 tests green)
- [x] `src-tauri/tests/phase_5_telemetry_opt_out_default.rs` — fresh install has `telemetry.enabled = '0'`; opt-in persists (2 tests green)

Moved to **Phase 6** exit criteria (credential-gated):

- [ ] `tests/e2e/phase-6-tag-to-msi.spec.ts` (CI-only) — push `v0.99.0-rc.1`, confirm signed MSI + updater manifest live within 15 min
- [ ] `tests/e2e/phase-6-updater-flow.spec.ts` — simulated manifest with newer version → app downloads + restarts
- [ ] `tests/docs/phase-6-docs-deploy.spec.ts` — mdBook builds; internal links resolve; search index populated
- [ ] Manual: install signed MSI on clean Windows 11 VM → no SmartScreen warning
- [ ] Manual: download from chronimage.app/download matches GitHub Releases checksum
- [ ] Manual: Sentry test event from staging build appears with PII scrubbed

## Open questions (resolved)

- **License model**: ✅ **lax** — offline Ed25519 verification, no server required. Shipped.
- **Open-source vs source-available**: ✅ **MIT OR Apache-2.0** dual-licensed (Rust convention). Shipped.
- **Store vs direct**: **both** — direct is v1.0.0; Store is Phase 6 §8.
- **Newsletter service**: deferred to Phase 6 §5 (marketing-site decision; signup form lives there).
- **Changelog automation trust**: deferred to Phase 6 §1 (actual release-cutting lives there).
- **Insider vetting**: deferred to Phase 6 §6 (signup form lives there).

## Phase 5 exit summary (2026-04-24)

Phase 5 closes with the in-code release-hardening scaffolding in place:

- **Ed25519 license verification** ships against a placeholder public key; Phase 6 §6 swaps in the real one.
- **Telemetry opt-in plumbing** ships default-off; Phase 6 §3 wires the Sentry backend behind the same flag.
- **Governance docs** (LICENSE × 3, CONTRIBUTING, SECURITY, issue + PR templates, runbook) cover the disclosure + review loop so contributors have a clear path when the repo opens.
- **Schema at v8**: `license_state` single-row table + `telemetry.enabled` seed value in `settings`.

Total Phase 5 diff: 21 files · ~1.4 k lines added. Tests: 377 rust lib + 7 new integration + 111 vitest. Zero clippy / deny / biome errors.

---

## After Phase 5 — what's next?

- **[Phase 6 · Launch infrastructure](./phase-6.md)** — the credential-gated half of release hardening: EV cert, Tauri updater keys, Cloudflare Pages × 3 domains, Sentry DSN + scrubber, mdBook docs, marketing landing, Insider signup form, branch protection, Microsoft Store / WinGet / Chocolatey submissions. This is where v1.0.0 actually ships.
- **Phase 7 (speculative): cloud backup**, user-BYO (S3/R2/B2) — opt-in; encrypted at rest with user-held key.
- **Phase 8 (speculative): mobile companion** — iOS/Android, shoots directly to Chronimage over LAN/Tailscale. Also absorbs the **mobile / tablet responsive pass** originally filed from the Phase 2 rehaul (ADR 0007): touch-target sizing ≥ 44 px, sheet-based detail view, camera-roll-style gestures, Tauri 2 mobile target once stable, masonry packer re-validated sub-720 px.
- **Phase 9 (speculative): shared albums** — LAN-based URL sharing, no cloud middleman.
- **Phase 10 (speculative): macOS + Linux ports** — once Windows is stable and user demand exists.
- **Phase 11 (speculative): collaboration / team libraries** — if the user base asks for it.

These are parking spots, not commitments. v1.0.0 Chronimage (shipped at the end of Phase 6) is already a complete product.
