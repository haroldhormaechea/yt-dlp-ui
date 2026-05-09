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
$DenoShaLib = Join-Path $ScriptDir 'lib-deno-sha.ps1'
$FixturesDir = Join-Path $ScriptDir 'tests/fixtures'

if (-not (Test-Path -LiteralPath $YtDlpScript -PathType Leaf)) {
    throw "fetch-yt-dlp.ps1 not found at $YtDlpScript"
}
if (-not (Test-Path -LiteralPath $DenoScript -PathType Leaf)) {
    throw "fetch-deno.ps1 not found at $DenoScript"
}
if (-not (Test-Path -LiteralPath $DenoShaLib -PathType Leaf)) {
    throw "lib-deno-sha.ps1 not found at $DenoShaLib"
}
foreach ($fx in @(
        'deno-v2.7.14-unix.sha256sum',
        'deno-v2.7.14-windows.sha256sum',
        'malformed.sha256sum',
        'empty.sha256sum')) {
    $p = Join-Path $FixturesDir $fx
    if (-not (Test-Path -LiteralPath $p)) {
        throw "fixture missing: $p"
    }
}

# Upstream-published lower-case hashes for the captured v2.7.14 fixtures.
# Literal compare; if upstream republishes v2.7.14 with different bytes,
# regenerate the fixtures and update these constants.
$UPSTREAM_UNIX_SHA = '3287efef53606966469cb6a02781327be22b908959397f976e2996dc1b64ae0f'
$UPSTREAM_WINDOWS_SHA = '25f9871f5c1d9e999d60071f8069767134495fd601d2e2c7ce1e8c641487bda0'

# Dot-source the parser library directly (NOT fetch-deno.ps1 — its
# Mandatory-param block would block parser-only invocations).
. $DenoShaLib

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

# In-process parser test — calls Get-DenoExpectedSha directly (no subprocess)
# and compares against an expected literal value. Any throw is a failure.
function Invoke-ParserCase {
    param(
        [string] $Name,
        [string] $FixturePath,
        [string] $ExpectedHash
    )
    $script:total++
    try {
        $actual = Get-DenoExpectedSha -Path $FixturePath
        if ($actual -ne $ExpectedHash) {
            Write-Host "FAIL: $Name (expected '$ExpectedHash', got '$actual')"
            $script:failures++
        } else {
            Write-Host "ok: $Name"
        }
    } catch {
        Write-Host "FAIL: $Name (threw: $_)"
        $script:failures++
    }
}

# In-process parser failure test — expects Get-DenoExpectedSha to throw with
# a message containing 'could not parse'. Pinned error semantics:
# `[Console]::Error.WriteLine` followed by `throw "could not parse <path>"`.
function Invoke-ParserThrowCase {
    param(
        [string] $Name,
        [string] $FixturePath
    )
    $script:total++
    try {
        $null = Get-DenoExpectedSha -Path $FixturePath
        Write-Host "FAIL: $Name (expected throw, returned normally)"
        $script:failures++
    } catch {
        if ("$_" -like '*could not parse*') {
            Write-Host "ok: $Name"
        } else {
            Write-Host "FAIL: $Name (threw, but message was: $_)"
            $script:failures++
        }
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

# --- lib-deno-sha.ps1 parser shape tests (UC 21) ----------------------------

# Real upstream Unix-format fixture → upstream-published lower-case hash.
Invoke-ParserCase `
    'lib-deno-sha.ps1 unix-format fixture → upstream-published lowercase hash' `
    (Join-Path $FixturesDir 'deno-v2.7.14-unix.sha256sum') `
    $UPSTREAM_UNIX_SHA

# Real upstream Windows-format fixture → upstream-published lower-case hash.
Invoke-ParserCase `
    'lib-deno-sha.ps1 windows-format fixture → upstream-published lowercase hash' `
    (Join-Path $FixturesDir 'deno-v2.7.14-windows.sha256sum') `
    $UPSTREAM_WINDOWS_SHA

# Malformed → throw with "could not parse" message.
Invoke-ParserThrowCase `
    'lib-deno-sha.ps1 malformed fixture → throws "could not parse"' `
    (Join-Path $FixturesDir 'malformed.sha256sum')

# Empty → throw with "could not parse" message.
Invoke-ParserThrowCase `
    'lib-deno-sha.ps1 empty fixture → throws "could not parse"' `
    (Join-Path $FixturesDir 'empty.sha256sum')

if ($failures -gt 0) {
    Write-Host ""
    Write-Host "$failures of $total tests failed"
    exit 1
}

Write-Host ""
Write-Host "all $total tests passed"
exit 0
