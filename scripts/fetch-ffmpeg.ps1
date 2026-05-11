#!/usr/bin/env pwsh
# fetch-ffmpeg.ps1 — PowerShell parallel of fetch-ffmpeg.sh for Windows
# GHA runners. Same argv contract: <target-triple> <output-dir>.
#
# Pins live in scripts/runtime-deps-pins.env (shared with the .sh path).
# This script parses the .env file line-by-line and applies the same
# in-tree-pin + remote-checksum defense-in-depth posture.
#
# Exit codes mirror fetch-ffmpeg.sh: 64 usage, 65 missing env, 70 no SHA
# tool (Get-FileHash always present on PS 5.1+, so this is unreachable but
# kept for symmetry), 72 archive layout unexpected, 73 SHA mismatch,
# 75 pins file missing.
#
# This script is paired with fetch-ffmpeg.sh — every fix in one MUST land
# in the other (see scripts/README.md § Dual-script discipline).

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $TargetTriple,

    [Parameter(Mandatory = $true, Position = 1)]
    [string] $OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'lib-net-retry.ps1')

$pinsFile = Join-Path $PSScriptRoot 'runtime-deps-pins.env'
if (-not (Test-Path -LiteralPath $pinsFile -PathType Leaf)) {
    [Console]::Error.WriteLine("pins file not found at $pinsFile")
    exit 75
}

# Parse KEY=VALUE shell-style env into a hashtable. Comments + blank lines ignored.
$pins = @{}
foreach ($line in Get-Content -LiteralPath $pinsFile) {
    if ($line -match '^\s*#' -or $line -match '^\s*$') { continue }
    if ($line -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$') {
        $pins[$matches[1]] = $matches[2]
    }
}

foreach ($k in @('FFMPEG_VERSION', 'FFMPEG_RELEASE_TAG', 'FFMPEG_SHA256_WIN64')) {
    if (-not $pins.ContainsKey($k) -or -not $pins[$k]) {
        [Console]::Error.WriteLine("$k missing from pins file")
        exit 65
    }
}

if ($TargetTriple -like '*apple-darwin') {
    [Console]::Error.WriteLine('macOS uses build-ffmpeg-macos.sh, not fetch-ffmpeg.ps1')
    exit 64
}

$asset = switch ($TargetTriple) {
    'x86_64-pc-windows-msvc'    { "ffmpeg-$($pins.FFMPEG_RELEASE_TAG)-win64-lgpl-7.1.zip" }
    'x86_64-unknown-linux-gnu'  { "ffmpeg-$($pins.FFMPEG_RELEASE_TAG)-linux64-lgpl-7.1.tar.xz" }
    'aarch64-unknown-linux-gnu' { "ffmpeg-$($pins.FFMPEG_RELEASE_TAG)-linuxarm64-lgpl-7.1.tar.xz" }
    default {
        [Console]::Error.WriteLine("unknown target triple: $TargetTriple")
        exit 64
    }
}

$expectedSha = switch ($TargetTriple) {
    'x86_64-pc-windows-msvc'    { $pins.FFMPEG_SHA256_WIN64 }
    'x86_64-unknown-linux-gnu'  { $pins.FFMPEG_SHA256_LINUX64 }
    'aarch64-unknown-linux-gnu' { $pins.FFMPEG_SHA256_LINUXARM64 }
}

if ($asset -notlike '*-lgpl-*') {
    [Console]::Error.WriteLine("refusing to fetch non-LGPL asset: $asset")
    exit 64
}

$baseUrl = if ($env:FFMPEG_BASE_URL) {
    $env:FFMPEG_BASE_URL
} else {
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/$($pins.FFMPEG_VERSION)"
}

$workDir = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("fetch-ffmpeg-" + [System.IO.Path]::GetRandomFileName()))

try {
    Write-Host "fetching $baseUrl/$asset"
    Invoke-DownloadWithRetry -Uri "$baseUrl/$asset" -OutFile (Join-Path $workDir $asset)

    $remoteSha = $null
    try {
        # Retry the sidecar .sha256 download; fall back to in-tree pin if all
        # attempts fail (the file is optional — not all releases publish it).
        Invoke-DownloadWithRetry -Uri "$baseUrl/$asset.sha256" -OutFile (Join-Path $workDir "$asset.sha256")
        $first = (Get-Content -LiteralPath (Join-Path $workDir "$asset.sha256") | Select-Object -First 1)
        if ($first) {
            $remoteSha = ($first -split '\s+')[0]
        }
    } catch {
        Write-Warning "per-asset .sha256 not published; falling back to in-tree pin only"
    }

    $actualSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $workDir $asset)).Hash.ToLower()

    if ($expectedSha.ToLower() -ne $actualSha) {
        [Console]::Error.WriteLine("in-tree SHA256 pin mismatch for ${asset}: expected $expectedSha, got $actualSha")
        exit 73
    }

    if ($remoteSha -and ($remoteSha.ToLower() -ne $actualSha)) {
        [Console]::Error.WriteLine("remote checksums.sha256 mismatch for ${asset}: remote $remoteSha, actual $actualSha")
        exit 73
    }

    if (-not (Test-Path -LiteralPath $OutputDir -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    }

    # Extract.
    $extractedRoot = Join-Path $workDir 'extracted'
    New-Item -ItemType Directory -Force -Path $extractedRoot | Out-Null
    if ($asset -like '*.zip') {
        Expand-Archive -LiteralPath (Join-Path $workDir $asset) -DestinationPath $extractedRoot -Force
    } else {
        # tar -xJf for tar.xz; PS 7 ships tar.exe on Windows.
        & tar -xJf (Join-Path $workDir $asset) -C $extractedRoot
        if ($LASTEXITCODE -ne 0) {
            [Console]::Error.WriteLine("tar extraction failed with $LASTEXITCODE")
            exit 72
        }
    }

    $topDir = Get-ChildItem -LiteralPath $extractedRoot -Directory | Select-Object -First 1
    if (-not $topDir) {
        [Console]::Error.WriteLine('unexpected archive layout: no top-level directory')
        exit 72
    }

    $candidates = @(
        (Join-Path $topDir.FullName 'bin/ffmpeg.exe'),
        (Join-Path $topDir.FullName 'bin/ffmpeg'),
        (Join-Path $topDir.FullName 'ffmpeg.exe'),
        (Join-Path $topDir.FullName 'ffmpeg'),
        (Join-Path $topDir.FullName 'bin/ffprobe.exe'),
        (Join-Path $topDir.FullName 'bin/ffprobe'),
        (Join-Path $topDir.FullName 'ffprobe.exe'),
        (Join-Path $topDir.FullName 'ffprobe')
    )
    $ffmpegBin = $candidates | Where-Object {
        ($_ -match 'ffmpeg(\.exe)?$') -and (Test-Path -LiteralPath $_ -PathType Leaf)
    } | Select-Object -First 1
    if (-not $ffmpegBin) {
        [Console]::Error.WriteLine('ffmpeg binary not found inside extracted archive')
        exit 72
    }

    # UC 28: locate ffprobe alongside ffmpeg in the same BtbN archive.
    # Co-locating ffprobe with ffmpeg lets yt-dlp discover both via the
    # single `--ffmpeg-location <dir>` flag.
    $ffprobeBin = $candidates | Where-Object {
        ($_ -match 'ffprobe(\.exe)?$') -and (Test-Path -LiteralPath $_ -PathType Leaf)
    } | Select-Object -First 1
    if (-not $ffprobeBin) {
        [Console]::Error.WriteLine('ffprobe binary not found inside extracted archive')
        exit 72
    }

    # Canonical no-extension destination on every OS (mirrors fetch-yt-dlp.ps1).
    $dest = Join-Path $OutputDir 'ffmpeg'
    Copy-Item -Force -LiteralPath $ffmpegBin -Destination $dest

    # UC 28: ffprobe sits next to ffmpeg, canonical no-extension on every OS.
    $ffprobeDest = Join-Path $OutputDir 'ffprobe'
    Copy-Item -Force -LiteralPath $ffprobeBin -Destination $ffprobeDest

    $licenseDest = Join-Path $OutputDir 'ffmpeg-LICENSE.txt'
    $licenseSrc = Get-ChildItem -LiteralPath $topDir.FullName -Recurse -File `
        | Where-Object { $_.Name -in @('LICENSE.txt','LICENSE','COPYING.LGPLv2.1','COPYING.LGPLv3') } `
        | Select-Object -First 1
    if ($licenseSrc) {
        Copy-Item -Force -LiteralPath $licenseSrc.FullName -Destination $licenseDest
    } else {
        Set-Content -LiteralPath $licenseDest -Value @"
ffmpeg LGPL-only static build, sourced from https://github.com/BtbN/FFmpeg-Builds.
Bundled binaries are LGPL-2.1+ at minimum (configure flags exclude GPL- and
nonfree-licensed components — no x264, no x265, no fdk-aac).

Per LGPL terms, you may obtain the corresponding ffmpeg source code from
https://ffmpeg.org/download.html using FFMPEG_VERSION_SOURCE pinned in
scripts/runtime-deps-pins.env.

LICENSE.txt was not present inside the upstream archive; this stub stands in
until the next ffmpeg pin bump rotates a real LICENSE file in.
"@
    }

    Write-Host "placed $dest (ffmpeg $($pins.FFMPEG_RELEASE_TAG), $TargetTriple)"
    Write-Host "placed $ffprobeDest (ffprobe $($pins.FFMPEG_RELEASE_TAG), $TargetTriple)"
}
finally {
    Remove-Item -Recurse -Force -LiteralPath $workDir.FullName -ErrorAction SilentlyContinue
}
