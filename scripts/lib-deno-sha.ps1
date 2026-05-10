#!/usr/bin/env pwsh
# lib-deno-sha.ps1 — shared PowerShell helper for parsing deno's `.sha256sum`
# files. Dot-sourced by `fetch-deno.ps1` and the `test-fetch-ps1.ps1` test
# harness.
#
# Deno v2.x emits two different `.sha256sum` shapes depending on the build
# runner used to produce the asset:
#
#   * Unix triples (Linux + macOS): GNU coreutils format
#       "<64-hex-hash>  <asset-filename>\n"
#   * Windows triples: PowerShell `Get-FileHash | Format-List` output
#       "Algorithm : SHA256\nHash      : <64-hex-hash>\nPath      : ...\n"
#
# The parser rule is intentionally robust to both:
#   "first match of [0-9A-Fa-f]{64} anywhere in the file content".
# It tolerates `\r\n` / BOM / leading whitespace and is the lockstep partner
# of the rule used in `lib-deno-sha.sh` — see scripts/README.md
# § "Dual-script discipline".
#
# This file has NO top-level side effects (no Set-StrictMode,
# no $ErrorActionPreference, no module exports) so it is safe to dot-source
# from any caller without disturbing the caller's environment.

function Get-DenoExpectedSha {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    $content = Get-Content -Raw -LiteralPath $Path
    if ([string]::IsNullOrEmpty($content)) {
        [Console]::Error.WriteLine("could not parse $Path")
        throw "could not parse $Path"
    }
    $match = [regex]::Match($content, '[0-9A-Fa-f]{64}')
    if (-not $match.Success) {
        [Console]::Error.WriteLine("could not parse $Path")
        throw "could not parse $Path"
    }
    return $match.Value.ToLower()
}
