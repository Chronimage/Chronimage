#!/usr/bin/env bash
# debug-import — drive the real import pipeline against a folder and capture
# stage-by-stage timing logs.
#
# Usage:
#   ./scripts/debug-import.sh <path-to-folder>
#
# Prereqs:
#   - `pnpm tauri dev` is NOT running (it holds chronimage-app.exe open).
#   - The real ONNX model files exist under the app's model dir.
#
# Writes a summary to stdout and the full trace to `debug-import.log`.

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

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "▶ Running pipeline against: $SRC"

CHRONIMAGE_DEBUG_IMPORT_SRC="$SRC" \
  cargo test --manifest-path src-tauri/Cargo.toml --test debug_import \
    -- --ignored --nocapture debug_import_timing 2>&1 | tee debug-import.log

echo ""
echo "✓ Full log written to debug-import.log"
echo "  Search it for: import pipeline:, stage-2 per-photo timing, stage-4 per-photo timing"
