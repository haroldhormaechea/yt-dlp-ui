#!/usr/bin/env pwsh
# fetch-deno.ps1 — PowerShell parallel of fetch-deno.sh for Windows GHA runners.
# Same argv contract: <target-triple> <output-dir>.
#
# Required env: $env:DENO_VERSION (e.g. "1.47.2").
#
# SHA-only verification (deno does not publish GPG signatures; THREATS.md
# § T11 documents this asymmetry vs. yt-dlp's SHA+GPG posture).
#
# Places the binary at <output-dir>/deno (canonical name on every OS — see
# Smoke 1 of UC 06; the Windows branch of paths.rs probes deno.exe first
# and falls back to deno).
#
# This script is paired with fetch-deno.sh — every fix in one MUST land in
# the other (see scripts/README.md § Dual-script discipline).

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $TargetTriple,

    [Parameter(Mandatory = $true, Position = 1)]
    [string] $OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $env:DENO_VERSION) {
    [Console]::Error.WriteLine('DENO_VERSION env var is required')
    exit 65
}

$asset = "deno-$TargetTriple.zip"
$shaAsset = "$asset.sha256sum"
$baseUrl = if ($env:DENO_BASE_URL) {
    $env:DENO_BASE_URL
} else {
    "https://github.com/denoland/deno/releases/download/v$($env:DENO_VERSION)"
}

$workDir = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("fetch-deno-" + [System.IO.Path]::GetRandomFileName()))

try {
    Write-Host "fetching $baseUrl/$asset"
    Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile (Join-Path $workDir $asset) -UseBasicParsing

    Write-Host "fetching $baseUrl/$shaAsset"
    Invoke-WebRequest -Uri "$baseUrl/$shaAsset" -OutFile (Join-Path $workDir $shaAsset) -UseBasicParsing

    Write-Host 'verifying SHA256'
    # The .sha256sum file has format: "<hash>  <asset>"
    $expectedSha = (Get-Content -LiteralPath (Join-Path $workDir $shaAsset) | Select-Object -First 1) -split '\s+' | Select-Object -First 1
    if (-not $expectedSha) {
        [Console]::Error.WriteLine("could not parse $shaAsset")
        exit 72
    }
    $actualSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $workDir $asset)).Hash.ToLower()
    if ($expectedSha.ToLower() -ne $actualSha) {
        [Console]::Error.WriteLine("sha256 mismatch for ${asset}: expected $expectedSha, got $actualSha")
        exit 73
    }

    Write-Host 'unzipping'
    $extractDir = Join-Path $workDir 'extract'
    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    Expand-Archive -LiteralPath (Join-Path $workDir $asset) -DestinationPath $extractDir -Force

    # Archive contains deno.exe on Windows targets, deno otherwise.
    $srcBin = if ($TargetTriple -like '*windows*') { 'deno.exe' } else { 'deno' }
    $srcPath = Join-Path $extractDir $srcBin
    if (-not (Test-Path -LiteralPath $srcPath -PathType Leaf)) {
        [Console]::Error.WriteLine("extracted archive missing $srcBin")
        exit 71
    }

    if (-not (Test-Path -LiteralPath $OutputDir -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    }

    # Canonical destination name — see Smoke 1 outcome of UC 06.
    $dest = Join-Path $OutputDir 'deno'
    Move-Item -Force -LiteralPath $srcPath -Destination $dest

    Write-Host "placed $dest (deno $($env:DENO_VERSION), $TargetTriple)"
}
finally {
    Remove-Item -Recurse -Force -LiteralPath $workDir.FullName -ErrorAction SilentlyContinue
}
