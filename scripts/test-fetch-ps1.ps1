#!/usr/bin/env pwsh
# test-fetch-ps1.ps1 — argv smoke for fetch-yt-dlp.ps1 and fetch-deno.ps1.
#
# Runs on Windows GHA runners. Per AC #11 PowerShell carve-out, full
# SHA/GPG paths are not unit-tested in PS — they're covered by the
# `dist build --artifacts=local` smoke at AC #7. This file proves the
# argv contract: bad invocations exit non-zero with a clear message.
#
# Usage: pwsh scripts/test-fetch-ps1.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $PSCommandPath
$YtDlpScript = Join-Path $ScriptDir 'fetch-yt-dlp.ps1'
$DenoScript = Join-Path $ScriptDir 'fetch-deno.ps1'

if (-not (Test-Path -LiteralPath $YtDlpScript -PathType Leaf)) {
    throw "fetch-yt-dlp.ps1 not found at $YtDlpScript"
}
if (-not (Test-Path -LiteralPath $DenoScript -PathType Leaf)) {
    throw "fetch-deno.ps1 not found at $DenoScript"
}

$failures = 0
$total = 0

function Invoke-Case {
    param(
        [string] $Name,
        [scriptblock] $Block,
        [int] $ExpectedExit
    )
    $script:total++
    try {
        & $Block
        $actual = $LASTEXITCODE
        if ($actual -ne $ExpectedExit) {
            Write-Host "FAIL: $Name (expected exit $ExpectedExit, got $actual)"
            $script:failures++
        } else {
            Write-Host "ok: $Name"
        }
    } catch {
        Write-Host "FAIL: $Name (threw: $_)"
        $script:failures++
    }
}

# fetch-yt-dlp.ps1: missing required env → 65.
Invoke-Case 'fetch-yt-dlp.ps1 missing YT_DLP_VERSION → 65' {
    $env:YT_DLP_VERSION = $null
    & pwsh -NoProfile -File $YtDlpScript x86_64-pc-windows-msvc (Join-Path ([System.IO.Path]::GetTempPath()) 'out') 2>&1 | Out-Null
} 65

# fetch-yt-dlp.ps1: unknown target triple → 64.
Invoke-Case 'fetch-yt-dlp.ps1 unknown target triple → 64' {
    $env:YT_DLP_VERSION = '0.0.0-test'
    $tmp = New-TemporaryFile; Remove-Item $tmp; $tmpDir = New-Item -ItemType Directory -Path $tmp.FullName
    $env:REPO_ROOT = $tmpDir.FullName
    # No yt-dlp.asc → would fail with 75; create a placeholder so we hit the
    # target-triple branch first.
    New-Item -ItemType Directory -Force -Path (Join-Path $tmpDir 'scripts/keys') | Out-Null
    Set-Content -LiteralPath (Join-Path $tmpDir 'scripts/keys/yt-dlp.asc') -Value 'placeholder'
    & pwsh -NoProfile -File $YtDlpScript potato-unknown-triple (Join-Path $tmpDir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $tmpDir.FullName -ErrorAction SilentlyContinue
} 64

# fetch-yt-dlp.ps1: missing yt-dlp.asc → 75.
Invoke-Case 'fetch-yt-dlp.ps1 missing yt-dlp.asc → 75' {
    $env:YT_DLP_VERSION = '0.0.0-test'
    $tmp = New-TemporaryFile; Remove-Item $tmp; $tmpDir = New-Item -ItemType Directory -Path $tmp.FullName
    $env:REPO_ROOT = $tmpDir.FullName  # exists, but no scripts/keys/yt-dlp.asc inside
    & pwsh -NoProfile -File $YtDlpScript x86_64-pc-windows-msvc (Join-Path $tmpDir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $tmpDir.FullName -ErrorAction SilentlyContinue
} 75

# fetch-deno.ps1: missing DENO_VERSION → 65.
Invoke-Case 'fetch-deno.ps1 missing DENO_VERSION → 65' {
    $env:DENO_VERSION = $null
    & pwsh -NoProfile -File $DenoScript x86_64-pc-windows-msvc (Join-Path ([System.IO.Path]::GetTempPath()) 'out') 2>&1 | Out-Null
} 65

if ($failures -gt 0) {
    Write-Host ""
    Write-Host "$failures of $total tests failed"
    exit 1
}

Write-Host ""
Write-Host "all $total tests passed"
exit 0
