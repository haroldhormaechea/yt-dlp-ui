#!/usr/bin/env bash
# build-ffmpeg-macos.sh — builds an LGPL-only ffmpeg binary from upstream
# source on a macOS GHA runner (or a local M-series Mac during dev).
#
# Why source-build instead of fetching a prebuilt?
#   - BtbN/FFmpeg-Builds publishes Linux + Windows LGPL-only static builds
#     but no macOS variant.
#   - evermeet.cx ships x86_64-only builds; UC 06 mandates universal-binary
#     parity with the app, so we need both arches.
#   - Other macOS prebuilts (Homebrew, MacPorts) bundle GPL components.
#
# Argv:
#   $1 — output directory (placed: ffmpeg + ffmpeg-LICENSE.txt)
#
# Env:
#   FFMPEG_VERSION_SOURCE        — upstream tarball version (e.g. "7.1")
#   FFMPEG_TARBALL_SHA256_SOURCE — pinned SHA256 of the source tarball
#   MACOSX_DEPLOYMENT_TARGET     — defaults to "11.0" if unset
#   FFMPEG_BUILD_JOBS            — defaults to `sysctl -n hw.ncpu`
#
# Pins are loaded from scripts/runtime-deps-pins.env when the env vars
# above are not already set in the caller's environment.
#
# Configure flags (locked-in LGPL-only posture; UC 17 hard prohibitions):
#   --disable-gpl --disable-nonfree
#   --disable-libx264 --disable-libx265 --disable-libfdk-aac
#   --disable-libxvid --disable-libvpx --disable-libmp3lame
#   --enable-securetransport --enable-zlib
#   --enable-static --disable-shared
#   --disable-doc --disable-htmlpages --disable-manpages
#   --disable-podpages --disable-txtpages
#   --disable-debug --disable-ffplay
#
# `--enable-libopus` / `--enable-libvorbis` were removed from the locked
# set: they require external libraries at build time (`pkg-config` find
# of libopus / libvorbis), which would force every macOS dev host and
# the GHA runner to `brew install opus libvorbis`. yt-dlp's DASH-merge
# does not require them — ffmpeg's built-in opus/vorbis demuxers handle
# remuxing without re-encoding. If a future use case requires re-encode
# to opus/vorbis, add the brew install step in `package-dmg.yml` and
# the corresponding `--enable-libopus` flag in lock-step.
#
# Tail of script: configure-line lint on the built binary's `ffmpeg
# -version` output. Forbidden flags `--enable-libx264|libx265|libfdk-aac|
# gpl|nonfree` produce `exit 75` with the actual config line echoed.
#
# Exit codes:
#   64 — usage / required tool missing
#   65 — required env / pin missing
#   70 — source SHA mismatch
#   72 — extracted directory layout unexpected
#   73 — make / configure failed
#   75 — configure-line lint detected forbidden flag in built binary

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <output-dir>" >&2
    exit 64
fi

OUTPUT_DIR_RAW="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Resolve OUTPUT_DIR to absolute up front — the script `cd`s into the
# extracted source tree before invoking configure / make, so a relative
# OUTPUT_DIR would resolve against the wrong base when copying the
# binary out.
mkdir -p "${OUTPUT_DIR_RAW}"
OUTPUT_DIR="$(cd "${OUTPUT_DIR_RAW}" && pwd)"

# Load pins if env not preset.
PINS_FILE="${SCRIPT_DIR}/runtime-deps-pins.env"
if [[ -f "${PINS_FILE}" ]]; then
    # shellcheck disable=SC1090
    source "${PINS_FILE}"
fi

if [[ -z "${FFMPEG_VERSION_SOURCE:-}" ]]; then
    echo "error: FFMPEG_VERSION_SOURCE not set (pins file?)" >&2
    exit 65
fi
if [[ -z "${FFMPEG_TARBALL_SHA256_SOURCE:-}" ]]; then
    echo "error: FFMPEG_TARBALL_SHA256_SOURCE not set (pins file?)" >&2
    exit 65
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
JOBS="${FFMPEG_BUILD_JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 2)}"

for tool in curl tar make clang; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        echo "error: required tool ${tool} not on PATH" >&2
        exit 64
    fi
done

WORK_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t build-ffmpeg-macos)"
trap 'rm -rf "${WORK_DIR}"' EXIT

TARBALL="ffmpeg-${FFMPEG_VERSION_SOURCE}.tar.xz"
URL="https://ffmpeg.org/releases/${TARBALL}"

echo "fetching ${URL}"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${TARBALL}" \
    "${URL}"

if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_SHA="$(sha256sum "${WORK_DIR}/${TARBALL}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_SHA="$(shasum -a 256 "${WORK_DIR}/${TARBALL}" | awk '{print $1}')"
else
    echo "error: neither sha256sum nor shasum available" >&2
    exit 70
fi

if [[ "${FFMPEG_TARBALL_SHA256_SOURCE}" != "${ACTUAL_SHA}" ]]; then
    echo "error: source tarball SHA mismatch" >&2
    echo "  expected: ${FFMPEG_TARBALL_SHA256_SOURCE}" >&2
    echo "  actual:   ${ACTUAL_SHA}" >&2
    exit 70
fi

echo "extracting ${TARBALL}"
tar -xJf "${WORK_DIR}/${TARBALL}" -C "${WORK_DIR}/"
SRC_DIR="${WORK_DIR}/ffmpeg-${FFMPEG_VERSION_SOURCE}"
if [[ ! -d "${SRC_DIR}" ]]; then
    echo "error: extracted directory not at expected layout (${SRC_DIR})" >&2
    exit 72
fi

cd "${SRC_DIR}"

echo "configuring ffmpeg ${FFMPEG_VERSION_SOURCE} (LGPL-only)"
./configure \
    --prefix="${WORK_DIR}/install" \
    --disable-gpl \
    --disable-nonfree \
    --disable-libx264 \
    --disable-libx265 \
    --disable-libfdk-aac \
    --disable-libxvid \
    --disable-libvpx \
    --disable-libmp3lame \
    --enable-securetransport \
    --enable-zlib \
    --enable-static \
    --disable-shared \
    --disable-doc \
    --disable-htmlpages \
    --disable-manpages \
    --disable-podpages \
    --disable-txtpages \
    --disable-debug \
    --disable-ffplay \
    || { echo "error: configure failed" >&2; exit 73; }

echo "building (jobs=${JOBS})"
make -j "${JOBS}" || { echo "error: make failed" >&2; exit 73; }

# Copy the built binary out.
DEST="${OUTPUT_DIR}/ffmpeg"
cp -f "${SRC_DIR}/ffmpeg" "${DEST}"
chmod +x "${DEST}"

# Copy LGPL license text.
LICENSE_DEST="${OUTPUT_DIR}/ffmpeg-LICENSE.txt"
if [[ -f "${SRC_DIR}/COPYING.LGPLv2.1" ]]; then
    cp -f "${SRC_DIR}/COPYING.LGPLv2.1" "${LICENSE_DEST}"
elif [[ -f "${SRC_DIR}/COPYING.LGPLv3" ]]; then
    cp -f "${SRC_DIR}/COPYING.LGPLv3" "${LICENSE_DEST}"
elif [[ -f "${SRC_DIR}/LICENSE.md" ]]; then
    cp -f "${SRC_DIR}/LICENSE.md" "${LICENSE_DEST}"
else
    echo "warning: no LGPL license file found in source tree" >&2
    echo "ffmpeg ${FFMPEG_VERSION_SOURCE} — LGPL-2.1+ (no LICENSE file in upstream tree)" \
        > "${LICENSE_DEST}"
fi

# Configure-line lint: re-run the binary and inspect its banner. yt-dlp-ui's
# LGPL-only posture forbids any of these flags appearing in the configure
# line that ffmpeg embeds in its binary.
echo "linting configure flags"
CONFIG_LINE="$("${DEST}" -version 2>&1 | grep -E '^\s*configuration:' || true)"
if [[ -z "${CONFIG_LINE}" ]]; then
    echo "warning: ffmpeg -version did not echo a configuration line; lint inconclusive" >&2
fi

FORBIDDEN_REGEX='--enable-libx264|--enable-libx265|--enable-libfdk-aac|--enable-gpl|--enable-nonfree'
if [[ -n "${CONFIG_LINE}" ]] && echo "${CONFIG_LINE}" | grep -E -- "${FORBIDDEN_REGEX}" >/dev/null; then
    echo "error: forbidden GPL/nonfree flag detected in built ffmpeg" >&2
    echo "  config: ${CONFIG_LINE}" >&2
    exit 75
fi

echo "placed ${DEST} (ffmpeg ${FFMPEG_VERSION_SOURCE}, LGPL-only)"
echo "config: ${CONFIG_LINE}"
