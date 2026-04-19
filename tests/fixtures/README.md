# Test fixtures

Binary photo fixtures are stored via Git LFS. See `.gitattributes` at repo root for the tracked patterns.

## Setup

Install Git LFS (Windows):
```powershell
winget install -e --id GitHub.GitLFS
# or via git-for-windows bundled
git lfs install
```

After cloning:
```bash
git lfs pull
```

## Layout

```
tests/fixtures/
├── photos/                  # binary RAW/HEIC/JPG fixtures (LFS)
│   ├── sony-a7iv/           # ARW + JPG pairs (primary user's camera)
│   ├── iphone/              # HEIC + Live Photos
│   ├── mixed/               # CR3, NEF, RAF, DNG, ORF samples (1 each)
│   └── synthetic/           # PNG/JPG test images we generate on demand (NOT LFS)
├── manifests/               # JSON manifests describing expected import outcomes
├── catalogs/                # SQLite dump files used by integration tests
└── tmp/                     # GITIGNORED — scratch space for test runs
```

## Minimum required for Phase 1 tests

- 5000 RAW+JPG pairs (any camera, synthetic OK) for pair-detection precision test
- 500 photos of a single labeled person across different scenes for face-clustering F1 test
- 100 Google-Photos-style JSON exports for source-cleanup dry-run test
- 100k synthetic JPGs for import-throughput test (generate on-the-fly; do NOT commit)

## Licensing

Only commit fixtures you own or that are CC0 / unambiguously public-domain. Do NOT commit photos of other people unless explicit written consent is filed.
