# Security policy

## Supported versions

Chronimage follows semantic versioning. Security fixes land on the
latest minor release of the current major; older majors are not
supported once a new one ships.

| Version | Supported |
| ------- | --------- |
| `1.x`   | ✅ current |
| `< 1.0` | ❌ pre-release; upgrade to 1.x |

## Reporting a vulnerability

**Do not open a public GitHub issue.** Send a report privately:

- **Email**: `security@chronimage.app` (monitored by the maintainers)
- **PGP**: public key fingerprint `TBD` — first rotation lands in v1.0.0

Please include:

1. A short summary of the issue.
2. Steps to reproduce (or a PoC).
3. The version affected (check `Help → About` or
   `pnpm tauri info`).
4. Your name / handle if you want credit in the release notes.

We aim to:

- Acknowledge receipt within 72 hours.
- Triage + reproduce within 7 days.
- Ship a fix within 30 days for high-severity issues (RCE, auth bypass,
  data loss). Lower-severity fixes may roll into the next minor.

If the fix ships as a new release, we publish an advisory at
`github.com/Chronimage/chronimage/security/advisories` with a CVE when
applicable.

## Out of scope

- Vulnerabilities in third-party services the user configures
  themselves (their Flux/SDXL sidecar, their Google/OneDrive OAuth
  consent flow, their OpenStreetMap tile provider, etc.). Report those
  to the upstream maintainer.
- Theoretical attacks that require physical access to an already
  unlocked device. Chronimage stores data locally; if someone has your
  keyboard, your catalog is their catalog.
- Rate-limit bypasses on public-data endpoints (we don't host any).

## Safe-harbour

We won't pursue legal action against researchers who:

- Make a good-faith effort to follow this disclosure process.
- Don't exfiltrate real user data beyond what's needed to prove the
  bug.
- Give us reasonable time to patch before going public.
