#!/usr/bin/env pwsh
# fetch-yt-dlp.ps1 — PowerShell parallel of fetch-yt-dlp.sh for Windows GHA
# runners. Same argv contract: <target-triple> <output-dir>.
#
# Required env: $env:YT_DLP_VERSION (e.g. "2026.04.21").
# Optional env: $env:REPO_ROOT (defaults to one level up from this script).
#
# Verifies the upstream yt-dlp binary via SHA256 + GPG against the upstream
# key in $REPO_ROOT/scripts/keys/yt-dlp.asc. Places the binary at <output-dir>/yt-dlp
# (canonical name on every OS — see Smoke 1 of UC 06; the Windows branch of
# paths.rs probes yt-dlp.exe first and falls back to yt-dlp).
#
# Exit codes mirror fetch-yt-dlp.sh: 64 usage, 65 missing env, 72 asset not
# in SHA2-256SUMS, 73 SHA mismatch, 74 GPG fail, 75 yt-dlp.asc missing.
#
# This script is paired with fetch-yt-dlp.sh — every fix in one MUST land in
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

if (-not $env:YT_DLP_VERSION) {
    [Console]::Error.WriteLine('YT_DLP_VERSION env var is required')
    exit 65
}

if (-not $env:REPO_ROOT) {
    $env:REPO_ROOT = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
}

$keyPath = Join-Path $env:REPO_ROOT 'scripts/keys/yt-dlp.asc'
if (-not (Test-Path -LiteralPath $keyPath -PathType Leaf)) {
    [Console]::Error.WriteLine("yt-dlp.asc not found at $keyPath")
    exit 75
}

# Asset-name map — must stay in sync with fetch-yt-dlp.sh.
$asset = switch ($TargetTriple) {
    'x86_64-pc-windows-msvc'    { 'yt-dlp.exe' }
    'x86_64-unknown-linux-gnu'  { 'yt-dlp_linux' }
    'aarch64-unknown-linux-gnu' { 'yt-dlp_linux_aarch64' }
    'x86_64-apple-darwin'       { 'yt-dlp_macos' }
    'aarch64-apple-darwin'      { 'yt-dlp_macos' }
    default {
        [Console]::Error.WriteLine("unknown target triple: $TargetTriple")
        exit 64
    }
}

$baseUrl = if ($env:YT_DLP_BASE_URL) {
    $env:YT_DLP_BASE_URL
} else {
    "https://github.com/yt-dlp/yt-dlp/releases/download/$($env:YT_DLP_VERSION)"
}

$workDir = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("fetch-yt-dlp-" + [System.IO.Path]::GetRandomFileName()))
$gnupgDir = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("fetch-yt-dlp-gpg-" + [System.IO.Path]::GetRandomFileName()))

try {
    Write-Host "fetching $baseUrl/$asset"
    Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile (Join-Path $workDir $asset) -UseBasicParsing

    Write-Host "fetching $baseUrl/SHA2-256SUMS"
    Invoke-WebRequest -Uri "$baseUrl/SHA2-256SUMS" -OutFile (Join-Path $workDir 'SHA2-256SUMS') -UseBasicParsing

    Write-Host "fetching $baseUrl/SHA2-256SUMS.sig"
    Invoke-WebRequest -Uri "$baseUrl/SHA2-256SUMS.sig" -OutFile (Join-Path $workDir 'SHA2-256SUMS.sig') -UseBasicParsing

    Write-Host 'verifying GPG signature on SHA2-256SUMS'
    # MSYS-built gpg shipped with Git for Windows (`C:\Program Files\Git\
    # usr\bin\gpg.exe`) does not accept Windows-style backslash paths from
    # native pwsh — `gpg --import 'D:\a\...\yt-dlp.asc'` fails with
    # "error reading ...: General error" because the MSYS path translator
    # mangles the drive-prefix + backslashes. Two mitigations applied:
    #   1. Set GNUPGHOME using forward slashes (still a valid Windows path
    #      but parses cleanly through MSYS).
    #   2. Pipe the key body into `gpg --import` via stdin so we don't pass
    #      a path at all. The verify step uses forward-slash paths for the
    #      same reason.
    # On Linux/macOS the same forward-slash form is already canonical, so
    # this is a no-op there.
    $env:GNUPGHOME = ($gnupgDir.FullName -replace '\\', '/')
    Write-Host "gpg path: $((Get-Command gpg).Source)"
    Write-Host "GNUPGHOME: $env:GNUPGHOME"
    Write-Host "key path: $keyPath"

    # Pipe key content via stdin to bypass any path-translation issue.
    # The yt-dlp signing key is ASCII-armored, so text mode is safe.
    $keyText = Get-Content -Raw -LiteralPath $keyPath
    $importOut = $keyText | & gpg --batch --import 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "--- gpg --import output ---"
        $importOut | ForEach-Object { Write-Host $_ }
        Write-Host "--- end ---"
        [Console]::Error.WriteLine('gpg --import failed')
        exit 74
    }

    $sigPath  = (Join-Path $workDir 'SHA2-256SUMS.sig') -replace '\\', '/'
    $sumsPath = (Join-Path $workDir 'SHA2-256SUMS')     -replace '\\', '/'
    $verifyOut = & gpg --batch --verify $sigPath $sumsPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "--- gpg --verify output ---"
        $verifyOut | ForEach-Object { Write-Host $_ }
        Write-Host "--- end ---"
        [Console]::Error.WriteLine('GPG verification failed for SHA2-256SUMS')
        exit 74
    }

    Write-Host "verifying SHA256 for $asset"
    $sha2sumsPath = Join-Path $workDir 'SHA2-256SUMS'
    $expectedSha = (Get-Content -LiteralPath $sha2sumsPath |
        Where-Object { $_ -match (' ' + [regex]::Escape($asset) + '$') } |
        Select-Object -First 1) -split '\s+' |
        Select-Object -First 1
    if (-not $expectedSha) {
        [Console]::Error.WriteLine("$asset not listed in SHA2-256SUMS")
        exit 72
    }
    $actualSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $workDir $asset)).Hash.ToLower()
    if ($expectedSha.ToLower() -ne $actualSha) {
        [Console]::Error.WriteLine("sha256 mismatch for ${asset}: expected $expectedSha, got $actualSha")
        exit 73
    }

    if (-not (Test-Path -LiteralPath $OutputDir -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    }

    # Canonical name on every OS — see Smoke 1 outcome of UC 06.
    $dest = Join-Path $OutputDir 'yt-dlp'
    Move-Item -Force -LiteralPath (Join-Path $workDir $asset) -Destination $dest

    Write-Host "placed $dest (yt-dlp $($env:YT_DLP_VERSION), $TargetTriple)"
}
finally {
    Remove-Item -Recurse -Force -LiteralPath $workDir.FullName -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force -LiteralPath $gnupgDir.FullName -ErrorAction SilentlyContinue
    $env:GNUPGHOME = $null
}
