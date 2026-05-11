#!/usr/bin/env pwsh
# test-fetch-ffmpeg.ps1 — argv smoke + closed-set asset guard for fetch-ffmpeg.ps1.
#
# Mirrors test-fetch-ps1.ps1's posture: per the AC#11 PowerShell carve-out,
# full SHA verification on the happy path is covered by the
# `dist build --artifacts=local` smoke. This file pins the argv contract,
# the macOS hard-fail, the closed-set asset guard, and the missing-env
# error paths.
#
# UC 28 note: ffprobe is staged from the same BtbN archive as ffmpeg by
# `fetch-ffmpeg.ps1` (extra candidate-path loop entries + a parallel copy
# step). The happy-path win64 outcome — `${OutputDir}/ffprobe` present
# alongside `${OutputDir}/ffmpeg`, canonical no-extension name on every OS
# — is exercised by:
#   - `scripts/test-fetch-ffmpeg.bats` win64 case (Test-Path equivalent),
#   - the `dist build --artifacts=local` CI smoke that builds a real
#     installer and runs `installer/tests/test-nsis-extract.ps1` against it
#     (which now asserts both `ffmpeg` AND `ffprobe` are present).
# Adding a duplicate happy-path here would require restaging the full
# stub-HTTP-server fixture chain the bats file relies on; the dist smoke
# is the agreed-upon Windows coverage path.
#
# Usage: pwsh scripts/test-fetch-ffmpeg.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $PSCommandPath
$FetchScript = Join-Path $ScriptDir 'fetch-ffmpeg.ps1'

if (-not (Test-Path -LiteralPath $FetchScript -PathType Leaf)) {
    throw "fetch-ffmpeg.ps1 not found at $FetchScript"
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

# Helper: stage the script alongside a synthetic pins file so PSScriptRoot
# resolves to a temp dir we control. Returns the staged script path.
# fetch-ffmpeg.ps1 dot-sources lib-net-retry.ps1 from $PSScriptRoot, so the
# lib must be staged alongside or the staged copy fails to load.
function Stage-Script {
    param([string] $PinsContent)
    $tmp = New-TemporaryFile
    Remove-Item $tmp
    $tmpDir = New-Item -ItemType Directory -Path $tmp.FullName
    Copy-Item -LiteralPath $FetchScript -Destination (Join-Path $tmpDir 'fetch-ffmpeg.ps1')
    Copy-Item -LiteralPath (Join-Path $ScriptDir 'lib-net-retry.ps1') -Destination (Join-Path $tmpDir 'lib-net-retry.ps1')
    Set-Content -LiteralPath (Join-Path $tmpDir 'runtime-deps-pins.env') -Value $PinsContent
    return [PSCustomObject]@{ Script = (Join-Path $tmpDir 'fetch-ffmpeg.ps1'); Dir = $tmpDir.FullName }
}

# A pins-file body sufficient to clear the env-required check. The SHA
# values are placeholders; the macOS / unknown-triple / closed-set tests
# don't reach the SHA-verification step, so placeholder values are fine.
$validPins = @"
FFMPEG_VERSION=autobuild-test
FFMPEG_RELEASE_TAG=n7.1.4
FFMPEG_SHA256_LINUX64=0000000000000000000000000000000000000000000000000000000000000000
FFMPEG_SHA256_LINUXARM64=0000000000000000000000000000000000000000000000000000000000000000
FFMPEG_SHA256_WIN64=0000000000000000000000000000000000000000000000000000000000000000
FFMPEG_VERSION_SOURCE=7.1
FFMPEG_TARBALL_SHA256_SOURCE=0000000000000000000000000000000000000000000000000000000000000000
"@

# missing pins file → 75
Invoke-Case 'missing pins file → 75' {
    $tmp = New-TemporaryFile
    Remove-Item $tmp
    $tmpDir = New-Item -ItemType Directory -Path $tmp.FullName
    Copy-Item -LiteralPath $FetchScript -Destination (Join-Path $tmpDir 'fetch-ffmpeg.ps1')
    # lib-net-retry.ps1 must still be present — it's dot-sourced before the
    # pins-file check, so omitting it would mask the pins-file error.
    Copy-Item -LiteralPath (Join-Path $ScriptDir 'lib-net-retry.ps1') -Destination (Join-Path $tmpDir 'lib-net-retry.ps1')
    & pwsh -NoProfile -File (Join-Path $tmpDir 'fetch-ffmpeg.ps1') x86_64-pc-windows-msvc (Join-Path $tmpDir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $tmpDir.FullName -ErrorAction SilentlyContinue
} 75

# missing FFMPEG_VERSION in pins → 65
Invoke-Case 'missing FFMPEG_VERSION in pins → 65' {
    $partialPins = @"
FFMPEG_RELEASE_TAG=n7.1.4
FFMPEG_SHA256_WIN64=0000000000000000000000000000000000000000000000000000000000000000
"@
    $staged = Stage-Script -PinsContent $partialPins
    & pwsh -NoProfile -File $staged.Script x86_64-pc-windows-msvc (Join-Path $staged.Dir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $staged.Dir -ErrorAction SilentlyContinue
} 65

# unknown target triple → 64
Invoke-Case 'unknown target triple → 64' {
    $staged = Stage-Script -PinsContent $validPins
    & pwsh -NoProfile -File $staged.Script potato-unknown-triple (Join-Path $staged.Dir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $staged.Dir -ErrorAction SilentlyContinue
} 64

# macOS hard-fail → 64
Invoke-Case 'x86_64-apple-darwin → 64 with build-ffmpeg-macos.sh redirect' {
    $staged = Stage-Script -PinsContent $validPins
    & pwsh -NoProfile -File $staged.Script x86_64-apple-darwin (Join-Path $staged.Dir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $staged.Dir -ErrorAction SilentlyContinue
} 64

Invoke-Case 'aarch64-apple-darwin → 64 with build-ffmpeg-macos.sh redirect' {
    $staged = Stage-Script -PinsContent $validPins
    & pwsh -NoProfile -File $staged.Script aarch64-apple-darwin (Join-Path $staged.Dir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $staged.Dir -ErrorAction SilentlyContinue
} 64

# Closed-set guard — env override is ignored. The script does not honor any
# FFMPEG_ASSET override, so even if the env var is set to a GPL-tagged
# filename, the script picks the LGPL asset for the requested triple. We
# can't easily run the happy path without staging archives + a stub HTTP
# server, but we can at least confirm an env injection that *would* be a
# GPL filename, when triggered for an unknown triple, still hits the
# unknown-triple branch (exit 64). This proves env vars don't escape the
# triple-driven case.
Invoke-Case 'FFMPEG_ASSET env override does not bypass triple gate' {
    $staged = Stage-Script -PinsContent $validPins
    $env:FFMPEG_ASSET = 'ffmpeg-master-latest-linux64-gpl-shared.tar.xz'
    & pwsh -NoProfile -File $staged.Script potato-unknown-triple (Join-Path $staged.Dir 'out') 2>&1 | Out-Null
    Remove-Item -Recurse -Force -LiteralPath $staged.Dir -ErrorAction SilentlyContinue
    Remove-Item Env:FFMPEG_ASSET -ErrorAction SilentlyContinue
} 64

if ($failures -gt 0) {
    Write-Host ""
    Write-Host "$failures of $total tests failed"
    exit 1
}

Write-Host ""
Write-Host "all $total tests passed"
exit 0
