#!/usr/bin/env bash
# fetch-bundled-models.sh
#
# Download the four Phase-1 bundled ONNX models into src-tauri/models/bundled/.
# Used by CI on Linux packaging runners and by devs on non-Windows machines.
#
# buffalo_l.zip (InsightFace) is downloaded once and both SCRFD + ArcFace files
# are extracted before the zip is deleted.
#
# Usage:
#   bash scripts/fetch-bundled-models.sh
#
# Output:
#   [ok] <filename> <size>
#   [fail] <reason>
# Exits non-zero if any model fails.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLED_DIR="$REPO_ROOT/src-tauri/models/bundled"
mkdir -p "$BUNDLED_DIR"

# ---------------------------------------------------------------------------
# Model definitions — keep in sync with src-tauri/src/ai/download.rs KNOWN_MODELS
# ---------------------------------------------------------------------------
# Fields: filename|url|sha256|zip_entry (empty string = direct download)
MODELS=(
  "siglip2-b16-image.onnx|https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/vision_model.onnx|c0573e3f4140c3a7c4e9cc5912bd6b26a033b46a6a8e8af26cbea262b163bcad|"
  "siglip2-b16-text.onnx|https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/text_model_quantized.onnx|tbd|"
  "siglip2-b16-tokenizer.json|https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/tokenizer.json|tbd|"
  "nima.onnx|https://huggingface.co/cromsc/nima-mobilenet-aesthetic/resolve/main/nima_mobilenet_aesthetic.onnx|c58b0c39b5b8f752b1b0ebf10e07e48406780ce3bf9d4647f8c43898748fe69c|"
  "det_10g.onnx|https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip|5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91|buffalo_l/det_10g.onnx"
  "w600k_r50.onnx|https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip|4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43|buffalo_l/w600k_r50.onnx"
  "sam2.1_hiera_large.encoder.onnx|https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_large_20260221.zip|tbd|sam2.1_hiera_large.encoder.onnx"
  "sam2.1_hiera_large.decoder.onnx|https://huggingface.co/vietanhdev/segment-anything-2.1-onnx-models/resolve/main/sam2.1_hiera_large_20260221.zip|tbd|sam2.1_hiera_large.decoder.onnx"
)

BUFFALO_ZIP_URL="https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip"
BUFFALO_ZIP_PATH="$BUNDLED_DIR/buffalo_l.zip.tmp"
FAILURES=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "[fail] no sha256 tool found (need sha256sum or shasum)" >&2
    exit 1
  fi
}

format_bytes() {
  local b=$1
  if [ "$b" -ge 1048576 ]; then
    awk "BEGIN { printf \"%.1f MB\", $b/1048576 }"
  else
    awk "BEGIN { printf \"%.0f KB\", $b/1024 }"
  fi
}

download_file() {
  local url="$1" dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --tries=3 -O "$dest" "$url"
  else
    echo "[fail] no download tool found (need curl or wget)" >&2
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Download buffalo_l.zip once if either InsightFace zip entry is missing.
# ---------------------------------------------------------------------------
needs_buffalo=false
for model_def in "${MODELS[@]}"; do
  IFS='|' read -r filename url _sha zip_entry <<< "$model_def"
  if [ -n "$zip_entry" ] && [ ! -f "$BUNDLED_DIR/$filename" ]; then
    if [ "$url" = "$BUFFALO_ZIP_URL" ]; then
      needs_buffalo=true
      break
    fi
  fi
done

if [ "$needs_buffalo" = true ]; then
  echo "Downloading buffalo_l.zip (288 MB)..."
  if ! download_file "$BUFFALO_ZIP_URL" "$BUFFALO_ZIP_PATH"; then
    echo "[fail] buffalo_l.zip download failed"
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# Process each model
# ---------------------------------------------------------------------------
for model_def in "${MODELS[@]}"; do
  IFS='|' read -r filename url expected_sha zip_entry <<< "$model_def"
  dest="$BUNDLED_DIR/$filename"

  # Skip if already present and hash matches (or hash is still "tbd").
  if [ -f "$dest" ]; then
    if [ "$expected_sha" = "tbd" ]; then
      size="$(wc -c < "$dest" | tr -d ' ')"
      echo "[ok] $filename $(format_bytes "$size") (cached; sha256 tbd)"
      continue
    fi
    actual_sha="$(sha256_file "$dest")"
    if [ "$actual_sha" = "$expected_sha" ]; then
      size="$(wc -c < "$dest" | tr -d ' ')"
      echo "[ok] $filename $(format_bytes "$size") (cached)"
      continue
    fi
    echo "  Hash mismatch on cached $filename — re-fetching"
    rm -f "$dest"
  fi

  if [ -n "$zip_entry" ]; then
    # Extract from an archive-backed model bundle.
    if ! command -v unzip >/dev/null 2>&1; then
      echo "[fail] $filename: unzip not found — install unzip and retry"
      FAILURES=$((FAILURES + 1))
      continue
    fi
    archive_path="$BUNDLED_DIR/$(basename "${url%%\\?*}").tmp"
    if [ "$url" = "$BUFFALO_ZIP_URL" ]; then
      archive_path="$BUFFALO_ZIP_PATH"
    elif [ ! -f "$archive_path" ]; then
      echo "Downloading $(basename "${url%%\\?*}")..."
      if ! download_file "$url" "$archive_path"; then
        echo "[fail] $filename: archive download failed"
        FAILURES=$((FAILURES + 1))
        continue
      fi
    fi
    tmp="${dest}.extract.tmp"
    if ! unzip -p "$archive_path" "$zip_entry" > "$tmp" 2>/dev/null; then
      echo "[fail] $filename: '$zip_entry' not found in $(basename "$archive_path")"
      rm -f "$tmp"
      FAILURES=$((FAILURES + 1))
      continue
    fi
    mv "$tmp" "$dest"
  else
    # Direct download.
    tmp="${dest}.download.tmp"
    if ! download_file "$url" "$tmp"; then
      echo "[fail] $filename: download failed"
      rm -f "$tmp"
      FAILURES=$((FAILURES + 1))
      continue
    fi
    mv "$tmp" "$dest"
  fi

  # Verify hash (skip when still "tbd" — pending first-run lock).
  if [ "$expected_sha" != "tbd" ]; then
    actual_sha="$(sha256_file "$dest")"
    if [ "$actual_sha" != "$expected_sha" ]; then
      echo "[fail] $filename: SHA256 mismatch (got $actual_sha, want $expected_sha)"
      rm -f "$dest"
      FAILURES=$((FAILURES + 1))
      continue
    fi
  fi

  size="$(wc -c < "$dest" | tr -d ' ')"
  echo "[ok] $filename $(format_bytes "$size")"
done

# Clean up zip after extraction.
if [ -f "$BUFFALO_ZIP_PATH" ]; then
  rm -f "$BUFFALO_ZIP_PATH"
fi
rm -f "$BUNDLED_DIR"/*.zip.tmp

if [ "$FAILURES" -gt 0 ]; then
  echo "$FAILURES model(s) failed — see above"
  exit 1
fi

echo "All bundled models ready in $BUNDLED_DIR"
exit 0
