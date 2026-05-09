#!/usr/bin/env bash
# lib-deno-sha.sh — shared bash helper for parsing deno's `.sha256sum`
# files. Sourced by `fetch-deno.sh` and the `bats` test harness.
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
# of the rule used in `lib-deno-sha.ps1` — see scripts/README.md
# § "Dual-script discipline".
#
# This file has NO top-level side effects so it is safe to `source`.

# parse_deno_sha256sum_file <path>
#   Echoes the lower-cased 64-hex-character SHA found in <path> on stdout.
#   On no match (empty file, malformed, no 64-hex run): writes
#   "error: could not parse <path>" to stderr and returns 1.
parse_deno_sha256sum_file() {
    local path="$1"
    local hash
    hash="$(grep -oE '[0-9a-fA-F]{64}' "${path}" | head -n 1 | tr 'A-F' 'a-f')"
    if [[ -z "${hash}" ]]; then
        echo "error: could not parse ${path}" >&2
        return 1
    fi
    echo "${hash}"
}
