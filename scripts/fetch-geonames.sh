#!/usr/bin/env bash
# Download the GeoNames cities15000 dataset and drop it next to the app
# data directory so the offline reverse-geocoder can load it at runtime.
# Chronimage falls back to a bundled ~120 city table when this file
# isn't present, so running this script is optional.
#
# License: https://creativecommons.org/licenses/by/4.0/ (GeoNames).

set -euo pipefail

case "$(uname -s)" in
    Linux*)  base="${XDG_DATA_HOME:-$HOME/.local/share}/app.chronimage.desktop" ;;
    Darwin*) base="$HOME/Library/Application Support/app.chronimage.desktop" ;;
    *)       base="${LOCALAPPDATA:-$HOME/AppData/Local}/app.chronimage.desktop" ;;
esac

dest="$base/geonames"
mkdir -p "$dest"

txt="$dest/cities15000.txt"
if [[ -f "$txt" ]]; then
    echo "cities15000.txt already present at $txt"
    echo "Delete the file and re-run to refresh."
    exit 0
fi

echo "Downloading cities15000.zip from download.geonames.org..."
zip="$dest/cities15000.zip"
curl -L --user-agent "Chronimage-geonames-fetch/0.1" \
    -o "$zip" \
    "https://download.geonames.org/export/dump/cities15000.zip"

echo "Extracting..."
(cd "$dest" && unzip -o "$zip" && rm "$zip")

echo "Installed: $txt"
echo "Restart Chronimage; the Map view will pick up the expanded city list."
