#!/usr/bin/env bash
# macos-signing-local.sh — developer helper for exercising the macOS
# signing path locally, BEFORE pushing a tag and burning a release.
#
# This script is macOS-only. On other OSes it prints a banner and exits
# 0 (so curious devs running it from a Linux dev box don't see a
# confusing error).
#
# It supports three modes, gated on env vars:
#
# ─── Mode A (default) — explanatory banner ────────────────────────────
#     No env set. Prints a banner pointing at `cargo run` (the
#     primary dev flow on macOS — the dev workflow does NOT involve
#     signing) and pointing at the other two modes below.
#
# ─── Mode B — ad-hoc-sign a built .app for local-launch sanity ───────
#     env: SIGNING_IDENTITY=- ./scripts/macos-signing-local.sh path/to/yt-dlp-ui.app
#
#     Useful when you want to mimic the deep-sign step against a freshly
#     synthesized .app (output of installer/build-macos-dmg.sh) without
#     a real Developer ID cert. CAVEAT: macOS 26.x's AppleSystemPolicy
#     enforces signature validity at exec time INDEPENDENT of Gatekeeper
#     assessment. An ad-hoc-signed .app may still bounce-and-die on
#     macOS 26 with the same `ASP: Security policy would not allow
#     process` kernel message UC 26 traced. This mode is genuinely
#     useful on macOS 11–15 and as a structural smoke; do not draw
#     conclusions from a successful local launch about whether the
#     real Dev ID build will launch on macOS 26.
#
# ─── Mode C — full Dev-ID deep-sign for .app-from-.dmg validation ────
#     env: SIGNING_IDENTITY="Developer ID Application: Foo (TEAMID)" \
#          ./scripts/macos-signing-local.sh path/to/yt-dlp-ui.app
#
#     Re-uses `installer/macos-signing.sh::deep_sign_app` and
#     `assess_app` against the user's own keychain-resident Developer
#     ID cert. Lets a maintainer verify the entitlements directory
#     contract before a release tag is cut. Notarization is NOT done
#     here — that needs the App Store Connect API key, which lives in
#     GHA secrets, not on dev machines.
#
# Both signed-modes preserve the input .app on the chance the dev wants
# to inspect it after; if you need a clean reset, `lipo -info` /
# `codesign -dvv` the result and rebuild via `installer/build-macos-dmg.sh`.

set -euo pipefail

case "$(uname)" in
    Darwin) ;;
    *) echo "macOS-only; no-op." ; exit 0 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER_DIR="$(cd "${SCRIPT_DIR}/../installer" && pwd)"
ENTITLEMENTS_DIR="${INSTALLER_DIR}/entitlements"

if [[ ! -d "${ENTITLEMENTS_DIR}" ]]; then
    echo "error: entitlements dir missing at ${ENTITLEMENTS_DIR}" >&2
    exit 75
fi

# Mode A — no SIGNING_IDENTITY, print banner.
if [[ -z "${SIGNING_IDENTITY:-}" ]]; then
    cat <<'BANNER'
yt-dlp-ui macOS local-signing helper.

This is a *helper* for the rare maintainer flow of "I want to deep-sign
a locally built .app before cutting a release tag." It is NOT part of
the day-to-day dev loop — `cargo run` (or `just run`) is. Day-to-day
development on macOS does not touch codesign at all.

Modes:

  ad-hoc:
      SIGNING_IDENTITY=- ./scripts/macos-signing-local.sh path/to/yt-dlp-ui.app
      Sign with the ad-hoc identity. Caveat: macOS 26.x's
      AppleSystemPolicy may still kill the launch — ad-hoc-signing
      clears Gatekeeper *assessment* but does NOT bypass exec-time
      AMFI policy enforcement on macOS 26. Useful on macOS 11–15 and
      as a structural smoke; do NOT use a passing local launch on
      macOS 11–15 as proof a Dev ID build will launch on 26.

  Developer ID:
      SIGNING_IDENTITY="Developer ID Application: Foo (TEAMID)" \
          ./scripts/macos-signing-local.sh path/to/yt-dlp-ui.app
      Use a Developer ID Application cert resident in the user's
      keychain. Re-uses installer/macos-signing.sh::deep_sign_app for
      the deep walk and assess_app for the verify pair. Notarization
      is NOT performed locally — that needs the App Store Connect
      API key which lives in GHA secrets.

References:
  installer/macos-signing.sh           — shared bash library
  installer/entitlements/              — per-binary entitlements
  docs/adr/0011-macos-signing-and-notarization.md
  use-cases/26-fix-macos-arm64-launch-failure.md
BANNER
    exit 0
fi

# Mode B / C — sign the supplied .app.
if [[ $# -lt 1 ]]; then
    echo "usage: SIGNING_IDENTITY=<identity-or-dash> $0 path/to/yt-dlp-ui.app" >&2
    exit 64
fi
APP_PATH="$1"
if [[ ! -d "${APP_PATH}" ]]; then
    echo "error: ${APP_PATH} is not a directory" >&2
    exit 65
fi

# shellcheck disable=SC1091
source "${INSTALLER_DIR}/macos-signing.sh"

if [[ "${SIGNING_IDENTITY}" == "-" ]]; then
    echo "ad-hoc-signing ${APP_PATH}"
    echo "REMINDER: macOS 26.x AppleSystemPolicy may still deny exec." >&2
    deep_sign_app "${APP_PATH}" "-" "${ENTITLEMENTS_DIR}"
else
    echo "Developer-ID-signing ${APP_PATH} with: ${SIGNING_IDENTITY}"
    deep_sign_app "${APP_PATH}" "${SIGNING_IDENTITY}" "${ENTITLEMENTS_DIR}"
    assess_app "${APP_PATH}"
fi

echo "done."
