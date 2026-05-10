#!/usr/bin/env bash
# fetch-yt-dlp.sh — release-time hook to fetch the upstream yt-dlp standalone
# binary, verify it (SHA256 + GPG against the upstream key in this repo's
# `scripts/keys/yt-dlp.asc`), and place it in the bundled-binary location used by paths.rs.
#
# Argv:
#   $1 — target triple (e.g. "aarch64-apple-darwin",
#                       "x86_64-unknown-linux-gnu",
#                       "aarch64-unknown-linux-gnu",
#                       "x86_64-apple-darwin")
#   $2 — output directory (the bundled-binary location used by paths.rs)
#
# Env:
#   YT_DLP_VERSION — required, e.g. "2026.04.21". The pinned yt-dlp release.
#                    Bumping is a manual PR.
#   REPO_ROOT      — optional override; defaults to two levels up from this
#                    script (i.e. the repo root). Used to locate `scripts/keys/yt-dlp.asc`.
#
# Behavior:
#   1. Map the target triple to the upstream asset name.
#   2. Fetch the binary + SHA2-256SUMS + SHA2-256SUMS.sig from the GitHub release.
#   3. GPG-verify SHA2-256SUMS using a temp keyring loaded from `scripts/keys/yt-dlp.asc`.
#   4. SHA-verify the binary against the row in SHA2-256SUMS.
#   5. Move binary to <output-dir>/yt-dlp (canonical name on every OS — see
#      Smoke 1 outcome of UC 06; cargo-dist's `include` is a single global
#      list and fails on missing entries, so we ship under one filename
#      everywhere). chmod +x on Unix.
#
# Exit codes:
#   64 — usage / unknown target triple
#   65 — required env var missing
#   70 — neither sha256sum nor shasum available
#   72 — asset not listed in SHA2-256SUMS (upstream release-asset drift)
#   73 — SHA mismatch
#   74 — GPG verify failed
#   75 — yt-dlp.asc not found

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <target-triple> <output-dir>" >&2
    exit 64
fi

TARGET_TRIPLE="$1"
OUTPUT_DIR="$2"

if [[ -z "${YT_DLP_VERSION:-}" ]]; then
    echo "error: YT_DLP_VERSION env var is required" >&2
    exit 65
fi

# Resolve REPO_ROOT — env override wins, else derive from script location.
if [[ -z "${REPO_ROOT:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
fi

KEY_PATH="${REPO_ROOT}/scripts/keys/yt-dlp.asc"
if [[ ! -f "${KEY_PATH}" ]]; then
    echo "error: yt-dlp.asc not found at ${KEY_PATH}" >&2
    exit 75
fi

# Asset-name map. Verified against
# `gh release view <YT_DLP_VERSION> --repo yt-dlp/yt-dlp --json assets`.
case "${TARGET_TRIPLE}" in
    x86_64-unknown-linux-gnu)
        ASSET="yt-dlp_linux"
        ;;
    aarch64-unknown-linux-gnu)
        ASSET="yt-dlp_linux_aarch64"
        ;;
    x86_64-apple-darwin|aarch64-apple-darwin)
        # For the pinned YT_DLP_VERSION (2026.03.17), yt-dlp_macos is x86_64-only;
        # the same asset is downloaded for both aarch64-apple-darwin and
        # x86_64-apple-darwin targets; the dmg builder handles the duplication by
        # copying once. When YT_DLP_VERSION is bumped, verify whether upstream now
        # ships universal2 (lipo -info yt-dlp_macos); if so, this script and
        # installer/build-macos-dmg.sh must be re-examined.
        ASSET="yt-dlp_macos"
        ;;
    x86_64-pc-windows-msvc)
        # Windows runners use the .ps1 script; this branch exists for symmetry
        # if the .sh is invoked under Git Bash / WSL.
        ASSET="yt-dlp.exe"
        ;;
    *)
        echo "error: unknown target triple: ${TARGET_TRIPLE}" >&2
        exit 64
        ;;
esac

BASE_URL="${YT_DLP_BASE_URL:-https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}}"

WORK_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t fetch-yt-dlp)"
GNUPG_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t fetch-yt-dlp-gpg)"
chmod 700 "${GNUPG_DIR}"
trap 'rm -rf "${WORK_DIR}" "${GNUPG_DIR}"' EXIT

echo "fetching ${BASE_URL}/${ASSET}"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/${ASSET}" \
    "${BASE_URL}/${ASSET}"

echo "fetching ${BASE_URL}/SHA2-256SUMS"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/SHA2-256SUMS" \
    "${BASE_URL}/SHA2-256SUMS"

echo "fetching ${BASE_URL}/SHA2-256SUMS.sig"
curl --fail --silent --show-error --location \
    --output "${WORK_DIR}/SHA2-256SUMS.sig" \
    "${BASE_URL}/SHA2-256SUMS.sig"

echo "verifying GPG signature on SHA2-256SUMS"
GNUPGHOME="${GNUPG_DIR}" gpg --batch --quiet --import "${KEY_PATH}"
if ! GNUPGHOME="${GNUPG_DIR}" gpg --batch --quiet \
    --verify "${WORK_DIR}/SHA2-256SUMS.sig" "${WORK_DIR}/SHA2-256SUMS"; then
    echo "error: GPG verification failed for SHA2-256SUMS" >&2
    exit 74
fi

echo "verifying SHA256 for ${ASSET}"
EXPECTED_SHA="$(grep " ${ASSET}\$" "${WORK_DIR}/SHA2-256SUMS" | awk '{print $1}' || true)"
if [[ -z "${EXPECTED_SHA}" ]]; then
    echo "error: ${ASSET} not listed in SHA2-256SUMS" >&2
    exit 72
fi

if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_SHA="$(sha256sum "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_SHA="$(shasum -a 256 "${WORK_DIR}/${ASSET}" | awk '{print $1}')"
else
    echo "error: neither sha256sum nor shasum available; cannot verify" >&2
    exit 70
fi

if [[ "${EXPECTED_SHA}" != "${ACTUAL_SHA}" ]]; then
    echo "error: sha256 mismatch for ${ASSET}" >&2
    echo "  expected: ${EXPECTED_SHA}" >&2
    echo "  actual:   ${ACTUAL_SHA}" >&2
    exit 73
fi

mkdir -p "${OUTPUT_DIR}"

# Canonical name on every OS — see Smoke 1 outcome of UC 06. cargo-dist's
# `include` is a single global list with no per-target pruning, so we ship
# `yt-dlp` (no extension) everywhere; the Windows branch of paths.rs probes
# `yt-dlp.exe` first and falls back to `yt-dlp`.
DEST="${OUTPUT_DIR}/yt-dlp"
mv "${WORK_DIR}/${ASSET}" "${DEST}"

case "${TARGET_TRIPLE}" in
    *windows*) ;;
    *) chmod +x "${DEST}" ;;
esac

echo "placed ${DEST} (yt-dlp ${YT_DLP_VERSION}, ${TARGET_TRIPLE})"
