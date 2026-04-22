#Requires -Version 5.1
<#
.SYNOPSIS
    Download the four Phase-1 bundled ONNX models into src-tauri/models/bundled/.

.DESCRIPTION
    Fetches siglip2-b16-image.onnx, nima.onnx, det_10g.onnx, and w600k_r50.onnx
    from their upstream URLs and verifies SHA256 hashes before writing to the
    destination directory.

    buffalo_l.zip (InsightFace) is downloaded once and both SCRFD + ArcFace files
    are extracted before the zip is deleted.

    CI invokes this script in the packaging job before `tauri build`.
    Developers run it once after cloning: `pwsh scripts/fetch-bundled-models.ps1`

.OUTPUTS
    One status line per model:
      [ok] <filename> <size>
      [fail] <reason>
    Exits non-zero if any model fails.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$BundledDir = Join-Path $RepoRoot 'src-tauri' 'models' 'bundled'

if (-not (Test-Path $BundledDir)) {
    New-Item -ItemType Directory -Path $BundledDir | Out-Null
}

# ---------------------------------------------------------------------------
# Model definitions — keep in sync with src-tauri/src/ai/download.rs KNOWN_MODELS
# ---------------------------------------------------------------------------
$Models = @(
    [PSCustomObject]@{
        Filename  = 'siglip2-b16-image.onnx'
        Url       = 'https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/vision_model.onnx'
        Sha256    = 'c0573e3f4140c3a7c4e9cc5912bd6b26a033b46a6a8e8af26cbea262b163bcad'
        ZipEntry  = $null
    },
    [PSCustomObject]@{
        # int8-quantized build (283 MB vs. 1.13 GB for fp32). Keeps f32
        # input/output tensor boundaries so the Rust ort loader in
        # `siglip.rs` needs no precision-casting changes. Swap back to
        # `text_model.onnx` (or `text_model_fp16.onnx`) from the Settings
        # picker if a power user wants higher retrieval quality.
        Filename  = 'siglip2-b16-text.onnx'
        Url       = 'https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/onnx/text_model_quantized.onnx'
        Sha256    = 'tbd'
        ZipEntry  = $null
    },
    [PSCustomObject]@{
        Filename  = 'siglip2-b16-tokenizer.json'
        Url       = 'https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/main/tokenizer.json'
        Sha256    = 'tbd'
        ZipEntry  = $null
    },
    [PSCustomObject]@{
        Filename  = 'nima.onnx'
        Url       = 'https://huggingface.co/cromsc/nima-mobilenet-aesthetic/resolve/main/nima_mobilenet_aesthetic.onnx'
        Sha256    = 'c58b0c39b5b8f752b1b0ebf10e07e48406780ce3bf9d4647f8c43898748fe69c'
        ZipEntry  = $null
    },
    [PSCustomObject]@{
        Filename  = 'det_10g.onnx'
        Url       = 'https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip'
        Sha256    = '5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91'
        ZipEntry  = 'buffalo_l/det_10g.onnx'
    },
    [PSCustomObject]@{
        Filename  = 'w600k_r50.onnx'
        Url       = 'https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip'
        Sha256    = '4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43'
        ZipEntry  = 'buffalo_l/w600k_r50.onnx'
    }
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Get-FileSha256 {
    param([string]$Path)
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLower()
    return $hash
}

function Format-Bytes {
    param([long]$Bytes)
    if ($Bytes -ge 1MB) { return '{0:F1} MB' -f ($Bytes / 1MB) }
    return '{0:F0} KB' -f ($Bytes / 1KB)
}

# ---------------------------------------------------------------------------
# Download buffalo_l.zip once (covers both SCRFD + ArcFace)
# ---------------------------------------------------------------------------
$BuffaloZipUrl = 'https://github.com/deepinsight/insightface/releases/download/v0.7/buffalo_l.zip'
$BuffaloZipPath = Join-Path $BundledDir 'buffalo_l.zip.tmp'
$NeedBuffalo = $Models | Where-Object { $_.ZipEntry -ne $null } |
    Where-Object { -not (Test-Path (Join-Path $BundledDir $_.Filename)) }

if ($NeedBuffalo) {
    Write-Host "Downloading buffalo_l.zip (288 MB)..."
    try {
        $wc = [System.Net.WebClient]::new()
        $wc.DownloadFile($BuffaloZipUrl, $BuffaloZipPath)
        $wc.Dispose()
    } catch {
        Write-Host "[fail] buffalo_l.zip download failed: $_"
        exit 1
    }
}

# ---------------------------------------------------------------------------
# Process each model
# ---------------------------------------------------------------------------
$Failures = 0

foreach ($m in $Models) {
    $Dest = Join-Path $BundledDir $m.Filename

    # Skip if already present and hash matches (or hash is still "tbd").
    if (Test-Path $Dest) {
        if ($m.Sha256 -eq 'tbd') {
            $size = (Get-Item $Dest).Length
            Write-Host "[ok] $($m.Filename) $(Format-Bytes $size) (cached; sha256 tbd)"
            continue
        }
        $existing = Get-FileSha256 -Path $Dest
        if ($existing -eq $m.Sha256) {
            $size = (Get-Item $Dest).Length
            Write-Host "[ok] $($m.Filename) $(Format-Bytes $size) (cached)"
            continue
        }
        Write-Host "  Hash mismatch on cached $($m.Filename) — re-fetching"
        Remove-Item $Dest -Force
    }

    if ($m.ZipEntry -ne $null) {
        # Extract from the already-downloaded buffalo_l.zip.
        try {
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $zip = [System.IO.Compression.ZipFile]::OpenRead($BuffaloZipPath)
            # Match by basename rather than full path — InsightFace's zip
            # layout has shifted across v0.7 asset uploads (sometimes
            # prefixed with `buffalo_l/`, sometimes root-level). Fall back
            # to any entry whose filename matches.
            $expectedBase = Split-Path -Leaf $m.ZipEntry
            $entry = $zip.Entries | Where-Object { $_.FullName -ieq $m.ZipEntry } | Select-Object -First 1
            if (-not $entry) {
                $entry = $zip.Entries |
                    Where-Object { (Split-Path -Leaf $_.FullName) -ieq $expectedBase } |
                    Select-Object -First 1
            }
            if (-not $entry) {
                $available = ($zip.Entries | ForEach-Object { $_.FullName }) -join ', '
                $zip.Dispose()
                throw "Entry matching '$expectedBase' not found in buffalo_l.zip (available: $available)"
            }
            $stream = $entry.Open()
            $tmp = $Dest + '.extract.tmp'
            $fs = [System.IO.File]::OpenWrite($tmp)
            $stream.CopyTo($fs)
            $fs.Close()
            $stream.Close()
            $zip.Dispose()
            Move-Item -Path $tmp -Destination $Dest -Force
        } catch {
            Write-Host "[fail] $($m.Filename): $_"
            $Failures++
            continue
        }
    } else {
        # Direct download.
        $tmp = $Dest + '.download.tmp'
        try {
            $wc = [System.Net.WebClient]::new()
            $wc.DownloadFile($m.Url, $tmp)
            $wc.Dispose()
            Move-Item -Path $tmp -Destination $Dest -Force
        } catch {
            Write-Host "[fail] $($m.Filename): $_"
            if (Test-Path $tmp) { Remove-Item $tmp -Force }
            $Failures++
            continue
        }
    }

    # Verify hash (skip when still "tbd" — pending first-run lock).
    if ($m.Sha256 -ne 'tbd') {
        $actual = Get-FileSha256 -Path $Dest
        if ($actual -ne $m.Sha256) {
            Write-Host "[fail] $($m.Filename): SHA256 mismatch (got $actual, want $($m.Sha256))"
            Remove-Item $Dest -Force
            $Failures++
            continue
        }
    }

    $size = (Get-Item $Dest).Length
    Write-Host "[ok] $($m.Filename) $(Format-Bytes $size)"
}

# Clean up the zip after both entries are extracted.
if (Test-Path $BuffaloZipPath) {
    Remove-Item $BuffaloZipPath -Force
}

if ($Failures -gt 0) {
    Write-Host "$Failures model(s) failed — see above"
    exit 1
}

Write-Host "All bundled models ready in $BundledDir"
exit 0
