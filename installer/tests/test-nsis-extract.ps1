#!/usr/bin/env pwsh
# test-nsis-extract.ps1 — installer-level smoke for the NSIS .exe produced by
# package-nsis.yml + installer/yt-dlp-ui.nsi.
#
# Verifies the embedded files using 7-Zip's listing of the NSIS .exe (NSIS
# installers are valid 7z archives at the byte level).
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

# Use 7z's structured long-format listing (-slt) instead of the human-
# readable table. -slt emits "Path = ..." / "Size = ..." records per file,
# which is robust against NSIS prefixing entries with $PLUGINSDIR\, mixing
# slashes, or tab-padding columns. The default `7z l` output failed to
# capture sizes for entries like `yt-dlp` / `deno` because the regex didn't
# tolerate the actual column layout produced by NSIS archives.
$slt = & 7z l -slt $InstallerExe 2>&1 | Out-String

$sizes = @{}
$currentPath = $null
foreach ($line in ($slt -split "`r?`n")) {
    if ($line -match '^Path = (.+)$') {
        $currentPath = $Matches[1].Trim()
    } elseif ($line -match '^Size = (\d+)\s*$' -and $null -ne $currentPath) {
        # Key the dictionary by basename — NSIS paths may carry a $PLUGINSDIR
        # prefix or use forward/back slashes. Split-Path -Leaf normalises both.
        $leaf = Split-Path -Leaf $currentPath
        $sizes[$leaf] = [int64]$Matches[1]
        $currentPath = $null
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
        Write-Host "ok: $name present in installer"
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
    exit 1
}
Write-Host ""
Write-Host 'all checks passed'
exit 0
