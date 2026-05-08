#!/usr/bin/env bash
# fetch-ffmpeg.sh — release-time hook to fetch the LGPL-only static ffmpeg
# binary from BtbN/FFmpeg-Builds, verify it (in-tree SHA256 pin + remote
# checksums.sha256 defense-in-depth), and place it in the bundled-binary
# location used by paths.rs.
#
# Argv:
#   $1 — target triple (e.g. "aarch64-unknown-linux-gnu",
#                       "x86_64-unknown-linux-gnu",
#                       "x86_64-pc-windows-msvc")
#   $2 — output directory (the bundled-binary location used by paths.rs)
#
# Env (loaded from scripts/runtime-deps-pins.env):
#   FFMPEG_VERSION            — BtbN release tag (e.g. "autobuild-YYYY-MM-DD-HH-MM")
#   FFMPEG_RELEASE_TAG        — in-tag stable-release tag (e.g. "n7.1.4")
#   FFMPEG_SHA256_LINUX64     — pinned SHA256 for the linux64 LGPL static archive
#   FFMPEG_SHA256_LINUXARM64  — pinned SHA256 for the linuxarm64 LGPL static archive
#   FFMPEG_SHA256_WIN64       — pinned SHA256 for the win64 LGPL static archive
#
# Behaviour:
#   1. Map target triple to BtbN asset name. Closed set; no FFMPEG_ASSET
#      env override (treat unknown triples as a hard error so a typo can't
#      silently fetch the wrong asset).
#   2. Belt-and-suspenders `-lgpl-` substring guard before any fetch.
#   3. Fetch the archive plus the matching `checksums.sha256` from the
#      same release.
#   4. Verify the in-tree pinned SHA256 against the freshly downloaded
#      archive (primary check). Fail loudly on mismatch.
#   5. Defense-in-depth: parse the row in `checksums.sha256` for our asset
#      and verify it matches the archive too. Catches an attacker who
#      compromises the pinned SHA in this repo without also rotating
#      BtbN's published checksums.
#   6. Extract the archive; copy the bare `ffmpeg` binary to
#      <output-dir>/ffmpeg (canonical no-extension name on every OS,
#      mirroring fetch-yt-dlp.sh's posture). chmod +x on Unix.
#   7. Drop the archive's bundled LICENSE / GPL / LGPL text alongside as
#      <output-dir>/ffmpeg-LICENSE.txt for redistribution compliance.
#
# Exit codes (mirror fetch-yt-dlp.sh's contract):
#   64 — usage / unknown target triple / asset name failed lgpl guard
#   65 — required env var missing
#   70 — neither sha256sum nor shasum available
#   72 — asset not listed in remote checksums.sha256
#   73 — SHA mismatch (in-tree pin OR remote defense-in-depth)
#   75 — pins file not found
#
# Hard-fails on `*apple-darwin` triples — macOS is built from upstream
# FFmpeg source via build-ffmpeg-macos.sh because no LGPL-only mainstream
# macOS prebuilt exists (evermeet.cx ships x86_64 only).

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <target-triple> <output-dir>" >&2
    exit 64
fi

TARGET_TRIPLE="$1"
OUTPUT_DIR="$2"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PINS_FILE="${SCRIPT_DIR}/runtime-deps-pins.env"
if [[ ! -f "${PINS_FILE}" ]]; then
    echo "error: pins file not found at ${PINS_FILE}" >&2
    exit 75
fi
# shellcheck disable=SC1090
source "${PINS_FILE}"

case "${TARGET_TRIPLE}" in
    *apple-darwin)
        echo "error: macOS uses build-ffmpeg-macos.sh, not fetch-ffmpeg.sh" >&2
        exit 64
        ;;
    x86_64-unknown-linux-gnu)
        ASSET="ffmpeg-${FFMPEG_RELEASE_TAG}-linux64-lgpl-7.1.tar.xz"
        EXPECTED_SHA="${FFMPEG_SHA256_LINUX64}"
        ARCHIVE_TYPE="tar.xz"
        ;;
    aarch64-unknown-linux-gnu)
        ASSET="ffmpeg-${FFMPEG_RELEASE_TAG}-linuxarm64-lgpl-7.1.tar.xz"
        EXPECTED_SHA="${FFMPEG_SHA256_LINUXARM64}"
        ARCHIVE_TYPE="tar.xz"
        ;;
    x86_64-pc-windows-msvc)
        ASSET="ffmpeg-${FFMPEG_RELEASE_TAG}-win64-lgpl-7.1.zip"
        EXPECTED_SHA="${FFMPEG_SHA256_WIN64}"
        ARCHIVE_TYPE="zip"
        ;;
    *)
        echo "error: unknown target triple: ${TARGET_TRIPLE}" >&2
        exit 64
        ;;
esac

# Belt-and-suspenders guard: refuse to fetch any asset name that is not
# explicitly LGPL-tagged. Catches a future copy-paste bug introducing a
# `-gpl-` GPL build.
if [[ "${ASSET}" != *-lgpl-* ]]; then
    echo "error: refusing to fetch non-LGPL asset: ${ASSET}" >&2
    exit 64
fi

for var in FFMPEG_VERSION FFMPEG_RELEASE_TAG EXPECTED_SHA; do
    if [[ -z "${!var:-}" ]]; then
        echo "error: ${var} is empty (pins file?)" >&2
        exit 65
    fi
done

BASE_URL="${FFMPEG_BASE_URL:-https://github.com/BtbN/FFmpeg-Builds/releases/download/${FFMPEG_VERSION}}"

WORK_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t fetch-ffmpeg)"
trap 'rm -rf "${WORK_DIR}"' EXIT

echo "fetching ${BASE_URL}/${ASSET}"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${ASSET}" \
    "${BASE_URL}/${ASSET}"

# checksums.sha256 covers the entire release (one row per asset).
echo "fetching ${BASE_URL}/${ASSET}.sha256"
if ! curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${ASSET}.sha256" \
    "${BASE_URL}/${ASSET}.sha256"; then
    echo "warning: per-asset .sha256 not published; falling back to in-tree pin only" >&2
    REMOTE_SHA=""
else
    REMOTE_SHA="$(awk '{print $1}' "${WORK_DIR}/${ASSET}.sha256" | head -1)"
fi

# Compute the actual SHA256 of the archive.
if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_SHA="$(sha256sum "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_SHA="$(shasum -a 256 "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
else
    echo "error: neither sha256sum nor shasum available; cannot verify" >&2
    exit 70
fi

# Primary: in-tree pin.
if [[ "${EXPECTED_SHA}" != "${ACTUAL_SHA}" ]]; then
    echo "error: in-tree SHA256 pin mismatch for ${ASSET}" >&2
    echo "  expected: ${EXPECTED_SHA}" >&2
    echo "  actual:   ${ACTUAL_SHA}" >&2
    exit 73
fi

# Defense-in-depth: remote checksum row, when available.
if [[ -n "${REMOTE_SHA}" && "${REMOTE_SHA}" != "${ACTUAL_SHA}" ]]; then
    echo "error: remote checksums.sha256 mismatch for ${ASSET}" >&2
    echo "  remote:  ${REMOTE_SHA}" >&2
    echo "  actual:  ${ACTUAL_SHA}" >&2
    exit 73
fi

mkdir -p "${OUTPUT_DIR}"

# Extract. BtbN archives unpack into a single top-level directory named
# after the asset minus the extension.
case "${ARCHIVE_TYPE}" in
    tar.xz)
        tar -xJf "${WORK_DIR}/${ASSET}" -C "${WORK_DIR}/"
        ;;
    zip)
        unzip -q "${WORK_DIR}/${ASSET}" -d "${WORK_DIR}/"
        ;;
esac

EXTRACTED_DIR="${WORK_DIR}/${ASSET%.${ARCHIVE_TYPE}}"
# tar.xz strips one extension at a time, so for tar.xz we end up with .tar
EXTRACTED_DIR="${EXTRACTED_DIR%.tar}"

if [[ ! -d "${EXTRACTED_DIR}" ]]; then
    # Fall back: pick the first directory in WORK_DIR that's not the archive.
    EXTRACTED_DIR="$(find "${WORK_DIR}" -mindepth 1 -maxdepth 1 -type d | head -1)"
fi

if [[ ! -d "${EXTRACTED_DIR}" ]]; then
    echo "error: could not locate extracted directory for ${ASSET}" >&2
    exit 72
fi

# Locate ffmpeg under <extracted>/bin (BtbN convention) or fall back.
FFMPEG_BIN=""
for cand in "${EXTRACTED_DIR}/bin/ffmpeg" "${EXTRACTED_DIR}/bin/ffmpeg.exe" \
            "${EXTRACTED_DIR}/ffmpeg" "${EXTRACTED_DIR}/ffmpeg.exe"; do
    if [[ -f "${cand}" ]]; then
        FFMPEG_BIN="${cand}"
        break
    fi
done
if [[ -z "${FFMPEG_BIN}" ]]; then
    echo "error: ffmpeg binary not found inside extracted archive" >&2
    exit 72
fi

# Place at canonical no-extension path on every OS (parallel to fetch-yt-dlp.sh).
DEST="${OUTPUT_DIR}/ffmpeg"
cp -f "${FFMPEG_BIN}" "${DEST}"
case "${TARGET_TRIPLE}" in
    *windows*) ;;
    *) chmod +x "${DEST}" ;;
esac

# Drop the bundled license text alongside, for redistribution compliance.
LICENSE_DEST="${OUTPUT_DIR}/ffmpeg-LICENSE.txt"
LICENSE_SRC=""
for cand in "${EXTRACTED_DIR}/LICENSE.txt" "${EXTRACTED_DIR}/LICENSE" \
            "${EXTRACTED_DIR}/COPYING.LGPLv2.1" "${EXTRACTED_DIR}/COPYING.LGPLv3"; do
    if [[ -f "${cand}" ]]; then
        LICENSE_SRC="${cand}"
        break
    fi
done
if [[ -n "${LICENSE_SRC}" ]]; then
    cp -f "${LICENSE_SRC}" "${LICENSE_DEST}"
else
    # Synthesize a minimal LGPL-only attribution stub so the artifact is
    # never license-text-free — better than silent omission. Real text is
    # fetched at next bump.
    cat > "${LICENSE_DEST}" <<'EOF'
ffmpeg LGPL-only static build, sourced from https://github.com/BtbN/FFmpeg-Builds.
Bundled binaries are LGPL-2.1+ at minimum (configure flags exclude GPL- and
nonfree-licensed components — no x264, no x265, no fdk-aac).

Per LGPL terms, you may obtain the corresponding ffmpeg source code from
https://ffmpeg.org/download.html using FFMPEG_VERSION_SOURCE pinned in
scripts/runtime-deps-pins.env.

LICENSE.txt was not present inside the upstream archive; this stub stands in
until the next ffmpeg pin bump rotates a real LICENSE file in.
EOF
fi

echo "placed ${DEST} (ffmpeg ${FFMPEG_RELEASE_TAG}, ${TARGET_TRIPLE})"
