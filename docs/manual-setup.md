# Chronimage — manual setup & credentials

> **Scope:** everything that a human must configure personally — API keys, OAuth client registrations, code-signing certs, CI secrets, dev-tool endpoints. Automation cannot create these; they require real accounts, card charges, or social-login approval.
>
> **Maintenance contract (per [CLAUDE.md](../CLAUDE.md)):** anytime code lands that depends on a new external credential, env var, or GitHub Actions secret, the agent making the change **must update this file in the same PR**. The truth lives here; `README.md` and per-ADR docs link to specific sections rather than re-listing credentials.

---

## Quick status

| Item | Status | Blocks |
|---|---|---|
| `.env.local` (GITHUB_TOKEN, GH_TOKEN, gphotos secret) | ✅ configured | — |
| Google Photos OAuth client | ✅ registered (default client id baked in) | — |
| Loki dev log endpoint | 🟡 optional (auto-falls-back to localhost:3101) | dev observability only |
| Microsoft Graph / OneDrive OAuth app | ⛔ not registered | Phase 2 §6 OneDrive upload adapter |
| GitHub repository secrets (release workflows) | 🟡 partial | nightly/beta/stable/insider releases |
| Windows EV code-signing cert | ⛔ not acquired | signed stable/beta MSIs (Phase 5 §4) |
| Cloudflare R2 for model mirror | 🟡 token ref-ed by workflows, bucket TBD | release-time model upload |

Legend: ✅ done · 🟡 partial · ⛔ not done · — not blocking.

---

## 1. Local development — `.env.local`

Loaded at app boot by [`src-tauri/src/main.rs`](../src-tauri/src/main.rs#L74) via `dotenvy::from_filename(".env.local")`. Gitignored. Also read by `gh` CLI for PR creation.

```env
# GitHub PAT — required for `gh pr create`, `gh pr merge`, etc. from this session.
# Scopes needed: repo, workflow. Generate at https://github.com/settings/tokens (classic).
GITHUB_TOKEN=ghp_your_token_here
GH_TOKEN=ghp_your_token_here   # alias; gh CLI reads either

# Google Photos OAuth — ONLY needed if you're self-hosting a custom OAuth client.
# The bundled default at src-tauri/src/sources/google_photos.rs:93 works for
# personal use; this env var lets you override with your own client secret.
CHRONIMAGE_GPHOTOS_CLIENT_SECRET=GOCSPX-your_secret
```

**Sister file:** `.env.local.example` is committed with placeholder values so a fresh clone can copy → edit → run.

## 2. Google Photos OAuth

- **App reg:** https://console.cloud.google.com/apis/credentials → existing client id `192197717334-m6hbu02dhhi3igdm9hi771op1tptdbft.apps.googleusercontent.com` (baked in as `DEFAULT_CLIENT_ID` at [`google_photos.rs:93`](../src-tauri/src/sources/google_photos.rs#L93))
- **Scopes:** `photospicker.mediaitems.readonly` + `openid profile` (for userinfo)
- **Redirect URI:** `http://127.0.0.1:<ephemeral-port>/callback` (the loopback flow picks a free port per session)
- **Token storage:** Windows Credential Manager service `chronimage.source.google_photos` via the `keyring` crate

**Secret handling:** The client *id* is public; the *secret* ships via `CHRONIMAGE_GPHOTOS_CLIENT_SECRET` in `.env.local`. Omitting it works in dev (Google accepts empty secret for PKCE loopback flows) but production builds should bundle it via the build-time env.

**Known limitation:** Google killed the `mediaItems.batchCreate` upload endpoint in 2025. Uploads from Chronimage → Google Photos are not possible via the current API; the app uses the Picker API for *downloads* only. See Phase 2 §6 deferred items.

## 3. Microsoft Graph / OneDrive OAuth — **NOT YET REGISTERED**

Phase 2 §6 wants OneDrive upload. Requires:

1. Register app at https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps
2. Redirect URI: `http://127.0.0.1:<ephemeral>/callback` (same loopback pattern as Google)
3. Delegated permissions: `Files.ReadWrite` + `offline_access` + `User.Read`
4. Add client id as a baked-in constant at `src-tauri/src/sources/onedrive.rs` (not yet created)
5. Add env var `CHRONIMAGE_ONEDRIVE_CLIENT_SECRET` to `.env.local`
6. Keyring service: `chronimage.source.onedrive`

**Status:** deferred until Phase 2 follow-up PR. Not registered.

## 4. GitHub repository secrets (Settings → Secrets and variables → Actions)

Referenced by workflows in [`.github/workflows/`](../.github/workflows/). Every entry here **must exist** in the repo's GitHub Actions secrets, or the referenced workflow fails.

| Secret | Used by | Purpose | Status |
|---|---|---|---|
| `GITHUB_TOKEN` | all | Auto-provided by GitHub Actions; no manual action needed | ✅ auto |
| `TAURI_UPDATER_PRIVATE_KEY` | nightly, release-beta, release-stable | ed25519 private key for auto-updater manifest signing. Generate with `tauri signer generate -w ~/.tauri/chronimage.key` | ⛔ TBD |
| `TAURI_UPDATER_PWD` | nightly, release-beta, release-stable | Password protecting the updater private key | ⛔ TBD |
| `TAURI_UPDATER_PRIVATE_KEY_INSIDER` | insider | Separate key for the insider channel so a leaked stable key doesn't compromise insider builds | ⛔ TBD |
| `TAURI_UPDATER_PWD_INSIDER` | insider | Password for insider key | ⛔ TBD |
| `WINDOWS_SIGN_CERT_PFX_B` | release-beta, release-stable | Base64-encoded `.pfx` of the EV code-signing certificate. `base64 -w 0 < cert.pfx` | ⛔ TBD — EV cert not yet purchased |
| `WINDOWS_SIGN_CERT_PWD` | release-beta, release-stable | `.pfx` file password | ⛔ TBD |
| `WINDOWS_SIGN_CERT_NIGHTLY_PFX_B` | nightly | Self-signed cert for nightly (acceptable since nightly users accept the SmartScreen warning) | ⛔ TBD |
| `WINDOWS_SIGN_CERT_NIGHTLY_PWD` | nightly | Password for nightly self-signed cert | ⛔ TBD |
| `CLOUDFLARE_API_TOKEN` | nightly, release-beta, release-stable, insider | Pushes release artifacts + updater manifest to Cloudflare R2 bucket `chronimage-releases`. Scopes: `Workers Scripts:Edit`, `Workers R2 Storage:Edit` | ⛔ TBD |
| `CODECOV_TOKEN` | CI | Optional — coverage upload to codecov.io. Safe to omit until coverage gating on PRs matters | 🟡 optional |

**Adding a secret:** Settings → Secrets and variables → Actions → New repository secret. **Never commit a secret file** even gitignored — base64-encode and paste into the GitHub UI.

## 5. Cloudflare R2 — model + release mirror

Planned architecture (not yet implemented):

- **Bucket:** `chronimage-releases` for MSI + updater manifest downloads
- **Bucket:** `chronimage-models` for optional first-run model downloads (SigLIP, NIMA, SCRFD, ArcFace)
- **API token** stored as `CLOUDFLARE_API_TOKEN` in repo secrets (see §4)
- **Custom domain:** TBD — recommend `releases.chronimage.app` once the project has a domain

**Status:** token referenced in four workflows but no actual `wrangler` or `aws s3 cp` invocation yet. Bucket + domain creation is a Phase 5 §2 task.

## 6. Windows code-signing certificate — **not acquired**

Required for stable + beta to install without SmartScreen warnings.

- **EV cert vendor:** SSL.com or Sectigo (~\$300–500/year). Must be EV (extended validation) for immediate SmartScreen reputation — OV certs need a few hundred installs to earn trust
- **Format:** `.pfx` with password, base64-encoded → `WINDOWS_SIGN_CERT_PFX_B` secret
- **Timestamp server:** `http://timestamp.sectigo.com` or `http://timestamp.digicert.com` (already configured in `tauri.conf.json`)
- **Nightly channel fallback:** self-signed cert is fine; users accept the warning for nightlies

**Status:** ⛔ — Phase 5 §4 budget item.

## 7. Optional dev-tool endpoints

| Env var | Default | Purpose |
|---|---|---|
| `LOKI_URL` | `http://localhost:3101` | Where the backend ships logs (Grafana Loki). Override to point at a remote or disable (`""`). Used by [`main.rs:35`](../src-tauri/src/main.rs#L35) |
| `CHRONIMAGE_MODELS_DIR` | platform data dir | Override AI model download location. Useful for CI or shared-drive dev |
| `CHRONIMAGE_BUNDLED_MODELS_DIR` | installer-resolved | Override bundled-model resolution (dev overrides for testing fallbacks) |
| `CHRONIMAGE_THUMBNAILS_DIR` | platform data dir | Override thumbnail cache location |
| `CHRONIMAGE_RELEASE_CHANNEL` | `dev` (option_env) | Baked in at compile time by release workflows. Never set locally |

All optional — the app runs without any of them on a fresh install.

## 8. One-time operator tasks (outside the repo)

Tracked here so new operators don't miss them. Move to 🟡 or ✅ as they get done.

- [ ] Register the repo's `develop` and `main` branches with required status checks (Settings → Branches → Branch protection rules). Required checks: `Rust clippy + test (Windows)`, `Unit tests (vitest)`, `Lint + typecheck (node)`, `cargo deny`, `Rust fmt`, `Commit message lint`, `Forbidden patterns + secrets`.
- [ ] Enable `Require signed commits` on `main` + `develop` (CLAUDE.md mandates this; not currently enforced on GitHub side).
- [ ] Set `develop` as default branch.
- [ ] Add `release-please` GitHub App to the repo so its perpetual release PR keeps updating.
- [ ] Purchase EV code-signing cert (see §6).
- [ ] Provision Cloudflare R2 buckets (see §5).
- [ ] Generate + upload Tauri updater ed25519 keypair (see §4).
- [ ] Register OneDrive OAuth app (see §3).

---

## How this doc stays honest

1. **When adding code that reads a new env var** — update §1 or §7 in the same PR as the code.
2. **When a workflow references a new `secrets.*`** — update §4 in the same PR.
3. **When a manual step is completed** — flip its status in §Quick status and §8 to ✅.
4. **When you're about to ship something blocked by a deferred item** — grep for the item in this doc first; if the checklist isn't done, decide: block the PR, or carve a clean fallback path and note it.

If you're ever unsure whether a manual step is required for what you're building, check here first — easier to add a line than to debug a CI failure at 2 AM because a secret wasn't set.
