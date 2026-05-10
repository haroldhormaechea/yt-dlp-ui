#!/usr/bin/env bash
# build-macos-dmg.sh — synthesize a universal-binary `yt-dlp-ui.app` and
# package it into a single `yt-dlp-ui-universal.dmg`.
#
# cargo-dist 0.31.0 does not produce an .app bundle for macOS; this script
# fills that gap. Called from .github/workflows/package-dmg.yml after the
# dual-arch `build-local-artifacts` jobs have finished and their archives
# are available locally.
#
# Required env:
#   VERSION  — semver string (workflow extracts from ${{ github.ref_name }} minus 'v')
#   AARCH64_BIN_DIR — path containing the aarch64-apple-darwin binaries (app, ad-window)
#   X86_64_BIN_DIR  — path containing the x86_64-apple-darwin binaries (app, ad-window)
#   AARCH64_DEPS_DIR — path containing the aarch64-apple-darwin runtime-deps (yt-dlp, deno)
#   X86_64_DEPS_DIR  — path containing the x86_64-apple-darwin runtime-deps (yt-dlp, deno)
#   OUT_DIR  — output directory (created if absent); receives the .app and .dmg
#
# Required tools: lipo (Xcode), hdiutil (macOS-shipped), sed (BSD/GNU).
#
# Universal-binary policy (locked, team-lead approved):
#   Single .dmg lipo-merging x86_64 + aarch64. UC 01's non-technical-user
#   audience does not know "Apple Silicon" vs "Intel" — one download.
#   yt-dlp_macos is upstream-universal2; lipo-merging two identical inputs
#   is benign. deno publishes per-arch; lipo produces a real fat binary.

set -euo pipefail

if [[ -z "${VERSION:-}" ]]; then
    echo "error: VERSION env var is required" >&2
    exit 65
fi
for var in AARCH64_BIN_DIR X86_64_BIN_DIR AARCH64_DEPS_DIR X86_64_DEPS_DIR OUT_DIR; do
    if [[ -z "${!var:-}" ]]; then
        echo "error: ${var} env var is required" >&2
        exit 65
    fi
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INFO_PLIST_TEMPLATE="${SCRIPT_DIR}/Info.plist"

if [[ ! -f "${INFO_PLIST_TEMPLATE}" ]]; then
    echo "error: Info.plist template missing at ${INFO_PLIST_TEMPLATE}" >&2
    exit 75
fi

mkdir -p "${OUT_DIR}"
APP_DIR="${OUT_DIR}/yt-dlp-ui.app"
MACOS_DIR="${APP_DIR}/Contents/MacOS"
RES_DIR="${APP_DIR}/Contents/Resources"

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RES_DIR}"

echo "lipo-merging app binary"
lipo -create -output "${MACOS_DIR}/yt-dlp-ui" \
    "${AARCH64_BIN_DIR}/app" \
    "${X86_64_BIN_DIR}/app"

echo "lipo-merging ad-window binary"
lipo -create -output "${MACOS_DIR}/ad-window" \
    "${AARCH64_BIN_DIR}/ad-window" \
    "${X86_64_BIN_DIR}/ad-window"

# yt-dlp_macos is a single upstream binary that scripts/fetch-yt-dlp.sh
# maps to both aarch64 and x86_64 targets, so the two per-arch paths are
# byte-identical. lipo rejects duplicate-arch inputs, so just copy one.
echo "copying yt-dlp (single upstream binary, no lipo merge needed)"
cp "${AARCH64_DEPS_DIR}/yt-dlp" "${RES_DIR}/yt-dlp"

echo "lipo-merging deno (per-arch inputs)"
lipo -create -output "${RES_DIR}/deno" \
    "${AARCH64_DEPS_DIR}/deno" \
    "${X86_64_DEPS_DIR}/deno"

# UC 17: ffmpeg lipo-merge. aarch64 first then x86_64 to match the UC 06
# convention used for the app and ad-window binaries above.
echo "lipo-merging ffmpeg (per-arch inputs)"
lipo -create -output "${RES_DIR}/ffmpeg" \
    "${AARCH64_DEPS_DIR}/ffmpeg" \
    "${X86_64_DEPS_DIR}/ffmpeg"

# Bundle the LGPL license text. Pick whichever per-arch copy the build
# script produced — both are equivalent (same upstream source tarball).
if [[ -f "${AARCH64_DEPS_DIR}/ffmpeg-LICENSE.txt" ]]; then
    cp "${AARCH64_DEPS_DIR}/ffmpeg-LICENSE.txt" "${RES_DIR}/ffmpeg-LICENSE.txt"
elif [[ -f "${X86_64_DEPS_DIR}/ffmpeg-LICENSE.txt" ]]; then
    cp "${X86_64_DEPS_DIR}/ffmpeg-LICENSE.txt" "${RES_DIR}/ffmpeg-LICENSE.txt"
fi

# UC 20: ship the bundled yt-dlp's Unlicense terms alongside the binary,
# matching the ffmpeg-LICENSE.txt precedent from UC 17.
cp "$(dirname "$0")/yt-dlp-LICENSE.txt" "${RES_DIR}/yt-dlp-LICENSE.txt"

chmod +x "${MACOS_DIR}/yt-dlp-ui" "${MACOS_DIR}/ad-window" \
         "${RES_DIR}/yt-dlp" "${RES_DIR}/deno" "${RES_DIR}/ffmpeg"

# Version-template Info.plist via sed. The template has CFBundleVersion and
# CFBundleShortVersionString set to "0.1.0"; replace both with $VERSION.
echo "templating Info.plist with VERSION=${VERSION}"
sed "s/0\.1\.0/${VERSION}/g" "${INFO_PLIST_TEMPLATE}" > "${APP_DIR}/Contents/Info.plist"

# UC 17: clear extended attributes from the .app bundle before packaging
# so Gatekeeper does not double-quarantine bundled binaries on first
# launch (the user already pays one Gatekeeper-prompt cost for the app
# itself; without -cr each subprocess spawn would re-prompt). Idempotent
# under -c (no-op if no xattrs are set); -r recurses into Resources/.
echo "clearing extended attributes on ${APP_DIR}"
xattr -cr "${APP_DIR}"

# Build the .dmg via hdiutil (macOS-shipped, no third-party tool).
DMG_PATH="${OUT_DIR}/yt-dlp-ui-universal.dmg"
rm -f "${DMG_PATH}"
echo "creating ${DMG_PATH}"
hdiutil create \
    -volname yt-dlp-ui \
    -srcfolder "${APP_DIR}" \
    -ov \
    -format UDZO \
    "${DMG_PATH}"

echo "done: ${APP_DIR} + ${DMG_PATH}"
