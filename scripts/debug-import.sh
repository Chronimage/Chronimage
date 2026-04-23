#!/usr/bin/env bash
# debug-import — drive the real import pipeline against a folder and pull
# stage-by-stage timing from Loki.
#
# Usage:
#   ./scripts/debug-import.sh <path-to-folder>
#
# Prereqs:
#   - `pnpm tauri dev` is NOT running (it holds chronimage-app.exe open).
#   - Loki is up on :3101 (the dev compose file starts it).
#   - The real ONNX model files exist under the app's model dir.
#
# Writes a summary to stdout and a JSON report to `debug-import-report.json`.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <path-to-folder>" >&2
  exit 1
fi

SRC="$1"
if [[ ! -d "$SRC" ]]; then
  echo "error: '$SRC' is not a directory" >&2
  exit 1
fi

# Check the dev server isn't hogging the binary.
if tasklist.exe 2>/dev/null | grep -q "chronimage-app.exe"; then
  echo "error: chronimage-app.exe is running — stop 'pnpm tauri dev' first" >&2
  exit 2
fi

# Check Loki.
if ! curl -sS --max-time 2 "http://localhost:3101/ready" > /dev/null 2>&1; then
  echo "warning: Loki not reachable at :3101 — stage timings will only appear on stdout" >&2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "▶ Running pipeline against: $SRC"
START_NS=$(( $(date +%s) * 1000000000 ))

CHRONIMAGE_DEBUG_IMPORT_SRC="$SRC" \
  cargo test --manifest-path src-tauri/Cargo.toml --test debug_import \
    -- --ignored --nocapture debug_import_timing 2>&1 | tee debug-import.log

END_NS=$(( $(date +%s) * 1000000000 ))

echo ""
echo "▶ Pulling stage summary from Loki ($START_NS → $END_NS)"

# Pull stage-level summary lines.
curl -sS -G "http://localhost:3101/loki/api/v1/query_range" \
  --data-urlencode 'query={app="chronimage",run="debug-import"} |= "import pipeline:"' \
  --data-urlencode 'limit=200' \
  --data-urlencode "start=$START_NS" \
  --data-urlencode "end=$END_NS" \
  > debug-import-stages.json

python -c "
import json
with open('debug-import-stages.json') as f:
    d = json.load(f)
lines = []
for stream in d.get('data', {}).get('result', []):
    for ts, line in stream['values']:
        lines.append((ts, line))
lines.sort()
for _, line in lines:
    print(line)
"

echo ""
echo "▶ Per-photo distribution (stage 2 + stage 4)"

python <<'PY'
import json, statistics, re

def load(path):
    with open(path) as f:
        return json.load(f)

def pull(query):
    import subprocess
    p = subprocess.run(
        ["curl", "-sS", "-G", "http://localhost:3101/loki/api/v1/query_range",
         "--data-urlencode", f"query={query}",
         "--data-urlencode", "limit=5000",
         "--data-urlencode", f"start={__import__('os').environ.get('START_NS', '0')}",
         "--data-urlencode", f"end={__import__('os').environ.get('END_NS', '9999999999999999999')}"],
        capture_output=True, text=True,
    )
    return json.loads(p.stdout)

def digest(label, query, keys):
    d = pull(query)
    rows = []
    for stream in d.get('data', {}).get('result', []):
        s = stream.get('stream', {})
        row = {}
        for k in keys:
            v = s.get(k)
            if v is None:
                continue
            try:
                row[k] = int(v)
            except ValueError:
                row[k] = v
        if row:
            rows.append(row)
    if not rows:
        print(f"{label}: no samples")
        return
    print(f"{label}: {len(rows)} samples")
    for k in keys:
        vals = [r[k] for r in rows if isinstance(r.get(k), int)]
        if not vals:
            continue
        vals.sort()
        p50 = vals[len(vals)//2]
        p95 = vals[int(len(vals)*0.95)]
        print(f"  {k:12s}  p50={p50:>5} ms   p95={p95:>5} ms   max={max(vals):>5} ms")

import os
os.environ['START_NS'] = os.environ.get('START_NS', str(int(__import__('time').time() * 1_000_000_000) - 3600*1_000_000_000))
os.environ['END_NS'] = os.environ.get('END_NS', str(int(__import__('time').time() * 1_000_000_000)))

digest("stage-2 per-photo",
       '{app="chronimage",run="debug-import"} |= "stage-2 per-photo timing"',
       ["hash_ms", "meta_ms", "thumb_ms", "total_ms"])
digest("stage-4 per-photo",
       '{app="chronimage",run="debug-import"} |= "stage-4 per-photo timing"',
       ["nima_ms", "siglip_ms", "total_ms"])
PY

echo ""
echo "✓ Full log written to debug-import.log"
echo "✓ Raw Loki stage dump at debug-import-stages.json"
