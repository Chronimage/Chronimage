<#
.SYNOPSIS
  Walk a directory tree, find every RAW+JPG pair (same stem + same dir +
  timestamps within 1 s), and copy both files of each pair into a flat
  output directory. Also writes `manifest.json` listing every pair.

  Used to build the `phase_1_raw_jpg_pair.rs` exit-criterion fixture
  (PRD docs/prds/phase-1.md § Exit criteria — 5k-pair precision ≥ 99.5%).

.PARAMETER Source
  Root of the photo library to scan (recursive).
.PARAMETER Destination
  Output directory. Created if missing. Pairs are copied flat — if two
  sub-directories have the same stem, the second copy gets a dir-prefix
  appended to its name to avoid collision.
.PARAMETER MaxPairs
  Stop after copying this many pairs. 0 = unlimited. Default 5000 to match
  the PRD fixture target.
.PARAMETER TimestampTolerance
  Max seconds between RAW and JPG mtime to consider them a pair. Matches
  `src-tauri/src/import/pair.rs` default. Default 1.
#>
param(
  [Parameter(Mandatory)][string]$Source,
  [Parameter(Mandatory)][string]$Destination,
  [int]$MaxPairs = 5000,
  [int]$TimestampTolerance = 1
)

$ErrorActionPreference = 'Stop'

$rawExts = @('.arw', '.cr2', '.cr3', '.nef', '.raf', '.orf', '.rw2', '.dng', '.pef', '.srw')
$jpgExts = @('.jpg', '.jpeg')

if (-not (Test-Path $Source)) {
  Write-Error "Source path does not exist: $Source"
  exit 1
}
if (-not (Test-Path $Destination)) {
  New-Item -ItemType Directory -Path $Destination | Out-Null
}

Write-Host "Scanning $Source ..." -ForegroundColor Cyan
$scanStart = Get-Date

# Walk the tree once; record every image with its stem + parent dir + ext + mtime.
$allImages = Get-ChildItem -Path $Source -Recurse -File -Force -ErrorAction SilentlyContinue |
  Where-Object {
    $ext = $_.Extension.ToLowerInvariant()
    ($rawExts -contains $ext) -or ($jpgExts -contains $ext)
  } |
  ForEach-Object {
    [PSCustomObject]@{
      Path      = $_.FullName
      Stem      = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
      Ext       = $_.Extension.ToLowerInvariant()
      Dir       = $_.DirectoryName
      Mtime     = $_.LastWriteTime
      SizeBytes = $_.Length
    }
  }

$scanDur = (Get-Date) - $scanStart
Write-Host ("  {0} images in {1:N1}s" -f $allImages.Count, $scanDur.TotalSeconds)

# Group by (dir + stem). A pair = one RAW + one JPG in the same group with
# compatible mtimes. Multi-raw or multi-jpg groups skipped (ambiguous).
$groups = $allImages | Group-Object -Property { "$($_.Dir)::$($_.Stem)" }

$pairs = @()
$skippedMulti = 0
$skippedTimestamp = 0
$skippedUnpaired = 0

foreach ($g in $groups) {
  $raws = $g.Group | Where-Object { $rawExts -contains $_.Ext }
  $jpgs = $g.Group | Where-Object { $jpgExts -contains $_.Ext }

  if ($raws.Count -eq 0 -or $jpgs.Count -eq 0) { $skippedUnpaired++; continue }
  if ($raws.Count -gt 1 -or $jpgs.Count -gt 1) { $skippedMulti++; continue }

  $raw = $raws[0]
  $jpg = $jpgs[0]
  $delta = [Math]::Abs(($raw.Mtime - $jpg.Mtime).TotalSeconds)
  if ($delta -gt $TimestampTolerance) { $skippedTimestamp++; continue }

  $pairs += [PSCustomObject]@{
    RawPath  = $raw.Path
    JpgPath  = $jpg.Path
    Stem     = $raw.Stem
    Dir      = $raw.Dir
    MtimeGap = $delta
  }

  if ($MaxPairs -gt 0 -and $pairs.Count -ge $MaxPairs) { break }
}

Write-Host "Pair summary:" -ForegroundColor Cyan
Write-Host "  paired       : $($pairs.Count)"
Write-Host "  unpaired     : $skippedUnpaired"
Write-Host "  multi-match  : $skippedMulti  (ambiguous; skipped)"
Write-Host "  timestamp gap: $skippedTimestamp  (> ${TimestampTolerance}s; skipped)"

if ($pairs.Count -eq 0) {
  Write-Warning "No pairs found under $Source"
  exit 0
}

# Copy to destination. Collision-safe: dir-prefix the stem on second occurrence.
Write-Host "`nCopying to $Destination ..." -ForegroundColor Cyan
$seen = @{}
$copied = 0
$bytesTotal = 0
$copyStart = Get-Date
$manifest = New-Object System.Collections.Generic.List[object]

foreach ($p in $pairs) {
  $outStem = $p.Stem
  if ($seen.ContainsKey($outStem)) {
    # Collision — prefix with parent dir name, then hash the full path if still colliding.
    $parent = Split-Path -Leaf $p.Dir
    $outStem = "${parent}_${outStem}"
    $i = 2
    while ($seen.ContainsKey($outStem)) { $outStem = "${parent}_${i}_$($p.Stem)"; $i++ }
  }
  $seen[$outStem] = $true

  $rawDst = Join-Path $Destination ($outStem + (Get-Item $p.RawPath).Extension.ToLowerInvariant())
  $jpgDst = Join-Path $Destination ($outStem + (Get-Item $p.JpgPath).Extension.ToLowerInvariant())

  # Defender/Search-Indexer briefly locks freshly-written files. Retry with backoff,
  # and skip if a non-empty copy of the same size already exists (idempotent resume).
  function Copy-WithRetry($src, $dst) {
    $srcSize = (Get-Item -LiteralPath $src).Length
    if ((Test-Path -LiteralPath $dst) -and ((Get-Item -LiteralPath $dst).Length -eq $srcSize)) {
      return
    }
    $attempts = 0
    while ($true) {
      try {
        Copy-Item -LiteralPath $src -Destination $dst -Force -ErrorAction Stop
        return
      } catch {
        $attempts++
        if ($attempts -ge 6) { throw }
        Start-Sleep -Milliseconds (250 * [Math]::Pow(2, $attempts))
      }
    }
  }
  Copy-WithRetry $p.RawPath $rawDst
  Copy-WithRetry $p.JpgPath $jpgDst

  $rawSize = (Get-Item $rawDst).Length
  $jpgSize = (Get-Item $jpgDst).Length
  $bytesTotal += $rawSize + $jpgSize
  $copied++

  $manifest.Add([PSCustomObject]@{
    stem          = $outStem
    raw           = Split-Path -Leaf $rawDst
    jpg           = Split-Path -Leaf $jpgDst
    source_raw    = $p.RawPath
    source_jpg    = $p.JpgPath
    mtime_gap_sec = [Math]::Round($p.MtimeGap, 3)
  })

  if ($copied % 100 -eq 0) {
    $elapsed = (Get-Date) - $copyStart
    $rate = if ($elapsed.TotalSeconds -gt 0) { $copied / $elapsed.TotalSeconds } else { 0 }
    Write-Host ("  {0}/{1} copied ({2:N1}/s, {3:N2} GB)" -f $copied, $pairs.Count, $rate, ($bytesTotal / 1GB))
  }
}

$copyDur = (Get-Date) - $copyStart
Write-Host ("`n  {0} pairs copied in {1:N1}s · {2:N2} GB total" -f $copied, $copyDur.TotalSeconds, ($bytesTotal / 1GB))

# Manifest for the Rust test to read.
$manifestPath = Join-Path $Destination 'manifest.json'
$manifestBody = [PSCustomObject]@{
  source              = $Source
  generated_at        = (Get-Date).ToString('s')
  expected_pairs      = $copied
  timestamp_tolerance = $TimestampTolerance
  pairs               = $manifest
}
$manifestJson = $manifestBody | ConvertTo-Json -Depth 6
# Write UTF-8 without BOM — serde_json (and most JSON parsers) choke on BOM.
[System.IO.File]::WriteAllText($manifestPath, $manifestJson, (New-Object System.Text.UTF8Encoding $false))
Write-Host "manifest: $manifestPath" -ForegroundColor Green
