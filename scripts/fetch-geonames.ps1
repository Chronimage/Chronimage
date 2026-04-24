# Download the GeoNames cities15000 dataset and drop it next to the app
# data directory so the offline reverse-geocoder can load it at runtime.
# Chronimage falls back to a bundled ~120 city table when this file
# isn't present, so running this script is optional.
#
# License: https://creativecommons.org/licenses/by/4.0/ (GeoNames).

$ErrorActionPreference = "Stop"

$dest = Join-Path $env:LOCALAPPDATA "app.chronimage.desktop\geonames"
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$zip = Join-Path $dest "cities15000.zip"
$txt = Join-Path $dest "cities15000.txt"

if (Test-Path $txt) {
    Write-Host "cities15000.txt already present at $txt"
    Write-Host "Delete the file and re-run to refresh."
    exit 0
}

Write-Host "Downloading cities15000.zip from download.geonames.org..."
Invoke-WebRequest -Uri "https://download.geonames.org/export/dump/cities15000.zip" -OutFile $zip -UserAgent "Chronimage-geonames-fetch/0.1"

Write-Host "Extracting..."
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip

Write-Host "Installed: $txt"
Write-Host "Restart Chronimage; the Map view will pick up the expanded city list."
