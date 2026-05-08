#!/usr/bin/env bash
# test-dmg-mount.sh — installer-level smoke for the .dmg produced by
# package-dmg.yml + installer/build-macos-dmg.sh.
#
# Mounts the .dmg, verifies the universal-binary .app layout, and detaches.
#
# Usage: bash installer/tests/test-dmg-mount.sh <path-to-dmg>

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <path-to-dmg>" >&2
    exit 64
fi

DMG="$1"
if [[ ! -f "${DMG}" ]]; then
    echo "error: ${DMG} not found" >&2
    exit 65
fi

if ! command -v hdiutil >/dev/null 2>&1; then
    echo "error: hdiutil not available; macOS only" >&2
    exit 70
fi

# Mount silently; capture mountpoint.
MOUNT_POINT=""
cleanup() {
    if [[ -n "${MOUNT_POINT}" ]]; then
        hdiutil detach "${MOUNT_POINT}" -quiet >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

hdiutil attach -nobrowse -quiet "${DMG}"
MOUNT_POINT="$(hdiutil info | awk '/yt-dlp-ui$/ {print $NF; exit}')"
if [[ -z "${MOUNT_POINT}" ]]; then
    echo "error: could not determine mountpoint after attach" >&2
    exit 71
fi

EXIT_CODE=0
APP="${MOUNT_POINT}/yt-dlp-ui.app"

if [[ ! -d "${APP}" ]]; then
    echo "FAIL: ${APP} missing"
    exit 1
fi

for path in \
    "${APP}/Contents/MacOS/yt-dlp-ui" \
    "${APP}/Contents/MacOS/ad-window" \
    "${APP}/Contents/Resources/yt-dlp" \
    "${APP}/Contents/Resources/deno" \
    "${APP}/Contents/Info.plist"
do
    if [[ -f "${path}" ]]; then
        echo "ok: ${path##${MOUNT_POINT}/} present"
    else
        echo "FAIL: ${path##${MOUNT_POINT}/} missing"
        EXIT_CODE=1
    fi
done

# Exec bits + universal-binary check on the four binaries.
for path in \
    "${APP}/Contents/MacOS/yt-dlp-ui" \
    "${APP}/Contents/MacOS/ad-window" \
    "${APP}/Contents/Resources/yt-dlp" \
    "${APP}/Contents/Resources/deno"
do
    if [[ ! -x "${path}" ]]; then
        echo "FAIL: ${path##${MOUNT_POINT}/} not executable"
        EXIT_CODE=1
    fi
    FILE_OUT="$(file -b "${path}")"
    if [[ "${FILE_OUT}" == *"universal binary with 2 architectures"* ]]; then
        echo "ok: ${path##${MOUNT_POINT}/} is universal (x86_64 + arm64)"
    else
        echo "FAIL: ${path##${MOUNT_POINT}/} not universal — file says: ${FILE_OUT}"
        EXIT_CODE=1
    fi
done

# Info.plist sanity — version string must look like a semver, not the
# template literal "0.1.0" (unless that IS the release version).
PLIST="${APP}/Contents/Info.plist"
VERSION_LINE="$(grep -A1 'CFBundleShortVersionString' "${PLIST}" | grep '<string>' | head -1)"
if [[ -z "${VERSION_LINE}" ]]; then
    echo "FAIL: Info.plist missing CFBundleShortVersionString"
    EXIT_CODE=1
else
    echo "ok: ${VERSION_LINE//[$'\t ']/}"
fi

exit "${EXIT_CODE}"
