#!/usr/bin/env bash
# fetch-deno.sh — release-time hook to fetch the upstream deno binary, verify
# its SHA256, and place it next to the bundled yt-dlp binary.
#
# Argv:
#   $1 — target triple (e.g. "aarch64-apple-darwin", "x86_64-pc-windows-msvc",
#                       "x86_64-unknown-linux-gnu")
#   $2 — output directory (the bundled-binary location used by paths.rs)
#
# Env:
#   DENO_VERSION  — required, e.g. "2.7.14". The pinned deno release version.
#                   v2-only: the `.sha256sum` parser used here only handles
#                   the v2.x file shapes (GNU coreutils on Unix runners,
#                   `Get-FileHash | Format-List` on Windows runners). Re-pinning
#                   to a v1.x release will break parsing — see scripts/README.md
#                   § "Bump procedure" for the v2-only assumption.
#   DENO_BASE_URL — optional override for the GitHub release base URL. Used by
#                   the bats test harness to point fetches at a stub-curl
#                   serving local fixtures. Defaults to upstream's release URL.
#
# Exit codes:
#   64 — wrong argv count
#   65 — DENO_VERSION env var missing
#   70 — neither sha256sum nor shasum available on PATH
#   71 — unzipped archive missing the expected binary
#   72 — could not parse the upstream `.sha256sum` file (no 64-hex match)
#   73 — SHA256 mismatch between fetched archive and expected hash
#
# Behavior:
#   1. Map the target triple to deno's release-asset name.
#   2. Fetch the .zip and the matching .sha256sum from the GitHub release.
#   3. Parse the expected SHA from the .sha256sum file via lib-deno-sha.sh
#      (handles both deno v2.x file shapes; see that lib for the rule).
#   4. Compute the actual SHA via sha256sum (Linux) or shasum -a 256 (macOS),
#      compare case-insensitively.
#   5. Unzip to a temp dir, move the binary into <output-dir>, and chmod +x
#      on Unix.
#
# This script is callable in isolation (unit-testable). It is NOT yet wired
# into the GHA release workflow — that wiring is a separate UC.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <target-triple> <output-dir>" >&2
    exit 64
fi

TARGET_TRIPLE="$1"
OUTPUT_DIR="$2"

if [[ -z "${DENO_VERSION:-}" ]]; then
    echo "error: DENO_VERSION env var is required" >&2
    exit 65
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-deno-sha.sh
source "${SCRIPT_DIR}/lib-deno-sha.sh"

# Deno's release-asset naming: deno-<target-triple>.zip plus a matching
# <asset>.sha256sum. Linux musl is exposed as -unknown-linux-gnu in our
# target-triple input but uses the same asset name as glibc on deno.
ASSET="deno-${TARGET_TRIPLE}.zip"
SHA_ASSET="${ASSET}.sha256sum"
BASE_URL="${DENO_BASE_URL:-https://github.com/denoland/deno/releases/download/v${DENO_VERSION}}"

WORK_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t fetch-deno)"
trap 'rm -rf "${WORK_DIR}"' EXIT

echo "fetching ${BASE_URL}/${ASSET}"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${ASSET}" \
    "${BASE_URL}/${ASSET}"

echo "fetching ${BASE_URL}/${SHA_ASSET}"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${SHA_ASSET}" \
    "${BASE_URL}/${SHA_ASSET}"

echo "verifying SHA256"
EXPECTED_SHA="$(parse_deno_sha256sum_file "${WORK_DIR}/${SHA_ASSET}")" || exit 72

if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_SHA="$(sha256sum "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_SHA="$(shasum -a 256 "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
else
    echo "error: neither sha256sum nor shasum available; cannot verify" >&2
    exit 70
fi

EXPECTED_SHA_LC="$(echo "${EXPECTED_SHA}" | tr 'A-F' 'a-f')"
ACTUAL_SHA_LC="$(echo "${ACTUAL_SHA}" | tr 'A-F' 'a-f')"
if [[ "${EXPECTED_SHA_LC}" != "${ACTUAL_SHA_LC}" ]]; then
    echo "error: sha256 mismatch for ${ASSET} (expected ${EXPECTED_SHA_LC}, actual ${ACTUAL_SHA_LC})" >&2
    exit 73
fi

echo "unzipping"
unzip -q "${WORK_DIR}/${ASSET}" -d "${WORK_DIR}/extract"

mkdir -p "${OUTPUT_DIR}"
# The archive contains `deno.exe` on Windows and `deno` elsewhere.
case "${TARGET_TRIPLE}" in
    *windows*)
        SRC_BIN_NAME="deno.exe"
        ;;
    *)
        SRC_BIN_NAME="deno"
        ;;
esac

if [[ ! -f "${WORK_DIR}/extract/${SRC_BIN_NAME}" ]]; then
    echo "error: extracted archive missing ${SRC_BIN_NAME}" >&2
    exit 71
fi

# Canonical destination name on every OS — see Smoke 1 outcome of UC 06.
# cargo-dist's `include` is a single global list with no per-target pruning
# and fails on missing entries, so we ship `deno` (no extension) everywhere;
# paths.rs probes `deno.exe` first on Windows and falls back to `deno`.
DEST="${OUTPUT_DIR}/deno"
mv "${WORK_DIR}/extract/${SRC_BIN_NAME}" "${DEST}"

case "${TARGET_TRIPLE}" in
    *windows*) ;;
    *) chmod +x "${DEST}" ;;
esac

echo "placed ${DEST} (deno ${DENO_VERSION}, ${TARGET_TRIPLE})"
