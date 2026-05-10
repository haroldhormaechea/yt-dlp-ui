#!/usr/bin/env pwsh
# test-nsis-extract.ps1 — installer-level smoke for the NSIS .exe produced by
# package-nsis.yml + installer/yt-dlp-ui.nsi.
#
# Extracts the NSIS .exe to a temp directory via 7-Zip and verifies the
# expected files exist with reasonable sizes. Extraction is preferred over
# `7z l` parsing because:
#   - The default `7z l` table layout failed to capture sizes for entries
#     without an extension (yt-dlp, deno).
#   - `7z l -slt` does not emit Path/Size records for the embedded files of
#     an NSIS archive — only outer-archive metadata.
# Extracting and stat-ing the result on disk works regardless of how 7-Zip
# chooses to surface NSIS internals.
#
# Usage: pwsh installer/tests/test-nsis-extract.ps1 <path-to-installer.exe>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $InstallerExe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $InstallerExe -PathType Leaf)) {
    Write-Error "installer not found at $InstallerExe"
    exit 65
}

$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
if (-not $sevenZip) {
    Write-Error '7z not on PATH; required for NSIS extract test'
    exit 70
}

$extractRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$extractDir = Join-Path $extractRoot 'nsis-smoke-extract'
if (Test-Path -LiteralPath $extractDir) {
    Remove-Item -Recurse -Force -LiteralPath $extractDir
}
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

# `7z x -y` extracts everything quietly. NSIS archives may use forward or
# backward slashes internally; 7z translates them to native paths.
& 7z x $InstallerExe "-o$extractDir" -y *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Error "7z extract failed (exit $LASTEXITCODE)"
    exit 71
}

# Build a basename -> file size map across the entire extracted tree. The
# basename is what we test against, since NSIS may stash files under
# $PLUGINSDIR\ or $INSTDIR\ subdirectories that the test does not care about.
# If the same basename appears more than once (rare), take the largest —
# that's the application binary, not a plugin stub.
$sizes = @{}
Get-ChildItem -Recurse -File -LiteralPath $extractDir | ForEach-Object {
    $existing = if ($sizes.ContainsKey($_.Name)) { $sizes[$_.Name] } else { -1 }
    if ($_.Length -gt $existing) {
        $sizes[$_.Name] = $_.Length
    }
}

$expected = @(
    'yt-dlp-ui.exe',
    'ad-window.exe',
    'yt-dlp',
    'deno',
    'yt-dlp-LICENSE.txt',
    'LICENSE',
    'MicrosoftEdgeWebview2Setup.exe'
)

$failures = 0
foreach ($name in $expected) {
    if ($sizes.ContainsKey($name)) {
        Write-Host "ok: $name present in installer ($($sizes[$name]) bytes)"
    } else {
        Write-Host "FAIL: $name missing from installer"
        $failures++
    }
}

# Sanity size checks (yt-dlp > 5 MB, deno > 30 MB; loose lower bounds).
function Test-MinSize {
    param([string] $Name, [int64] $Min)
    if ($sizes.ContainsKey($Name) -and $sizes[$Name] -ge $Min) {
        Write-Host "ok: $Name size $($sizes[$Name]) >= $Min"
    } else {
        $actual = if ($sizes.ContainsKey($Name)) { $sizes[$Name] } else { '?' }
        Write-Host "FAIL: $Name size $actual not >= $Min"
        $script:failures++
    }
}
Test-MinSize 'yt-dlp' 5000000
Test-MinSize 'deno'   30000000

if ($failures -gt 0) {
    Write-Host ""
    Write-Host "$failures checks failed"
    Write-Host "Extracted contents (for debugging):"
    Get-ChildItem -Recurse -File -LiteralPath $extractDir | Select-Object FullName, Length | Format-Table -AutoSize | Out-String | Write-Host
    exit 1
}
Write-Host ""
Write-Host 'all checks passed'
exit 0
