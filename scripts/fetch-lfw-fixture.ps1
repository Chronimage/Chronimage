<#
.SYNOPSIS
  Build a face-cluster test fixture from LFW (Labeled Faces in the Wild).

  Downloads the LFW tarball (~170 MB, 13 233 images, 5 749 identities),
  picks the top-N identities with the most photos, flattens them into
  `tests/fixtures/face-clusters/photos/` and emits `labels.json` mapping
  each filename to a cluster id (one cluster per identity, noise id = -1).

  LFW is pre-cropped faces, so this fixture measures HDBSCAN + ArcFace F1
  in isolation — it does NOT exercise SCRFD detection. For the full
  detection-then-clustering pipeline, use the manual labeler flow
  (scripts/scan-raw-jpg-pairs.ps1 companion, deferred to end of Phase 1).

  LFW terms of use: https://vis-www.cs.umass.edu/lfw/ — publicly available
  for academic / research use with attribution. Fine for test fixtures.

.PARAMETER Destination
  Output dir. Defaults to `tests/fixtures/face-clusters/` relative to repo
  root (gitignored by the rule added in feature/phase1-week4-plumbing).

.PARAMETER TopN
  Number of identities (clusters) to keep. Default 10 — the top-10 LFW
  identities have 25-530 photos each, plenty for HDBSCAN signal.

.PARAMETER PerIdentityCap
  Limit per identity to keep total photos manageable. Default 20.
#>
param(
  [string]$Destination = 'tests\fixtures\face-clusters',
  [int]$TopN = 10,
  [int]$PerIdentityCap = 20
)

$ErrorActionPreference = 'Stop'

$lfwUrl = 'https://vis-www.cs.umass.edu/lfw/lfw.tgz'
# Primary mirror goes through a self-signed HTTPS that's flaky on Windows.
# Fall back to the academictorrents mirror if the first 404s / TLS-errors.
$lfwMirror = 'https://conradsanderson.id.au/lfwcrop/lfwcrop_color.zip'

if (-not (Test-Path $Destination)) {
  New-Item -ItemType Directory -Path $Destination -Force | Out-Null
}
$photosDir = Join-Path $Destination 'photos'
$labelsPath = Join-Path $Destination 'labels.json'
$cacheTgz = Join-Path $env:TEMP 'lfw.tgz'
$cacheExtract = Join-Path $env:TEMP 'lfw-extract'

if (-not (Test-Path $cacheTgz) -or (Get-Item $cacheTgz).Length -lt 100000000) {
  Write-Host "downloading LFW tarball (~170 MB) to $cacheTgz…" -ForegroundColor Cyan
  try {
    Invoke-WebRequest -Uri $lfwUrl -OutFile $cacheTgz -UseBasicParsing
  } catch {
    Write-Warning "primary LFW url failed ($($_.Exception.Message)); trying mirror"
    Invoke-WebRequest -Uri $lfwMirror -OutFile $cacheTgz -UseBasicParsing
  }
}

if (-not (Test-Path $cacheExtract)) {
  Write-Host "extracting…" -ForegroundColor Cyan
  New-Item -ItemType Directory -Path $cacheExtract -Force | Out-Null
  # tar.exe ships with Windows 10+; if missing, fall back to 7z/Expand-Archive.
  & tar -xzf $cacheTgz -C $cacheExtract
  if ($LASTEXITCODE -ne 0) {
    Write-Error "tar -xzf failed (exit $LASTEXITCODE). Install Git Bash or 7-Zip."
    exit 1
  }
}

# Two possible layouts after extraction:
#   (a) LFW proper: <cacheExtract>/lfw/<Person_Name>/<Person_Name>_NNNN.jpg
#   (b) conradsanderson lfwcrop_color: flat `faces/<Person_Name>_NNNN.ppm`
# We probe for the richer nested layout first; fall back to flat + filename-parse.
$lfwRoot = Join-Path $cacheExtract 'lfw'
$flatFacesDir = $null
if (-not (Test-Path $lfwRoot)) {
  $lfwcropDir = Join-Path $cacheExtract 'lfwcrop_color\faces'
  if (Test-Path $lfwcropDir) {
    $flatFacesDir = $lfwcropDir
  } else {
    $lfwRoot = $cacheExtract  # last-ditch: tarball extracted straight into root
  }
}

Write-Host "scanning identities…" -ForegroundColor Cyan
if ($flatFacesDir) {
  Write-Host "  (flat layout: $flatFacesDir)"
  # Parse identity from filename stem: drop trailing `_NNNN`.
  $byIdent = @{}
  Get-ChildItem -LiteralPath $flatFacesDir -File | ForEach-Object {
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
    if ($stem -match '^(.+)_\d{4}$') {
      $name = $matches[1]
      if (-not $byIdent.ContainsKey($name)) { $byIdent[$name] = New-Object System.Collections.Generic.List[object] }
      $byIdent[$name].Add($_)
    }
  }
  $identities = $byIdent.GetEnumerator() |
    ForEach-Object { [PSCustomObject]@{ Name = $_.Key; Count = $_.Value.Count; Photos = $_.Value } } |
    Where-Object { $_.Count -ge 5 } |
    Sort-Object -Property Count -Descending
} else {
  $identities = Get-ChildItem -LiteralPath $lfwRoot -Directory |
    ForEach-Object {
      $photos = Get-ChildItem -LiteralPath $_.FullName -Filter '*.jpg' -File
      [PSCustomObject]@{ Name = $_.Name; Count = $photos.Count; Photos = $photos }
    } |
    Where-Object { $_.Count -ge 5 } |
    Sort-Object -Property Count -Descending
}

Write-Host ("  {0} identities with >= 5 photos" -f $identities.Count)
$top = $identities | Select-Object -First $TopN

# Flatten: copy chosen photos into Destination\photos\, build labels.json.
if (Test-Path $photosDir) { Remove-Item -LiteralPath $photosDir -Recurse -Force }
New-Item -ItemType Directory -Path $photosDir -Force | Out-Null

$labels = New-Object System.Collections.Generic.List[object]
$clusterId = 0
foreach ($ident in $top) {
  $keep = $ident.Photos | Select-Object -First $PerIdentityCap
  foreach ($p in $keep) {
    $srcExt = [System.IO.Path]::GetExtension($p.Name).ToLowerInvariant()
    # Convert PPM → JPG via ImageMagick's `magick` if available, else keep PPM
    # (image crate decodes PPM natively; SCRFD preprocess accepts either).
    $dstName = [System.IO.Path]::GetFileNameWithoutExtension($p.Name) + $srcExt
    $dst = Join-Path $photosDir $dstName
    Copy-Item -LiteralPath $p.FullName -Destination $dst -Force
    # Paths in labels.json are relative to `face-clusters/` so the test can
    # `dir.join(label.file)` to reach the file under `photos/`.
    $labels.Add([PSCustomObject]@{ file = "photos/$dstName"; cluster = $clusterId })
  }
  Write-Host ("  cluster {0}: {1} ({2} photos kept)" -f $clusterId, $ident.Name, $keep.Count)
  $clusterId++
}

# Write labels.json (UTF-8, no BOM) — matches the test parser.
$labelsJson = $labels | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($labelsPath, $labelsJson, (New-Object System.Text.UTF8Encoding $false))

Write-Host ("`n{0} photos, {1} clusters" -f $labels.Count, $clusterId) -ForegroundColor Green
Write-Host "labels: $labelsPath"
Write-Host "photos: $photosDir"
Write-Host "`nrun: cargo test --manifest-path src-tauri/Cargo.toml --test phase_1_face_clustering -- --ignored --nocapture"
