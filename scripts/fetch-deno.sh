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
#   DENO_VERSION — required, e.g. "1.47.2". The pinned deno release version.
#                  Bumping this requires also updating the SHA256 verification
#                  step (deno provides per-asset .sha256sum files; this script
#                  fetches and uses them at runtime, so no in-tree digest).
#
# Behavior:
#   1. Map the target triple to deno's release-asset name.
#   2. Fetch the .zip and the matching .sha256sum from the GitHub release.
#   3. Verify SHA256 (sha256sum -c on Linux, shasum -a 256 -c on macOS).
#   4. Unzip to a temp dir, move the binary into <output-dir>, and chmod +x
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

# Deno's release-asset naming: deno-<target-triple>.zip plus a matching
# <asset>.sha256sum. Linux musl is exposed as -unknown-linux-gnu in our
# target-triple input but uses the same asset name as glibc on deno.
ASSET="deno-${TARGET_TRIPLE}.zip"
SHA_ASSET="${ASSET}.sha256sum"
BASE_URL="https://github.com/denoland/deno/releases/download/v${DENO_VERSION}"

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
cd "${WORK_DIR}"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "${SHA_ASSET}"
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "${SHA_ASSET}"
else
    echo "error: neither sha256sum nor shasum available; cannot verify" >&2
    exit 70
fi
cd - >/dev/null

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
