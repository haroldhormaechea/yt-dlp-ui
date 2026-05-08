#!/usr/bin/env bash
# generate-fixtures.sh — regenerate the hermetic test fixtures used by
# scripts/test-fetch-yt-dlp.bats.
#
# This script is run by-hand on a developer workstation. The generated
# fixtures are committed to the repo so the bats tests are self-contained
# (no per-CI-run regeneration; deterministic).
#
# Run from the repo root:
#   bash scripts/tests/fixtures/generate-fixtures.sh
#
# Side effects: rewrites every fixture file in this directory using freshly
# generated test keys (so signatures and key IDs change on every run; this
# is intentional — fixtures are opaque to humans).

set -euo pipefail

FIX_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Isolated GNUPGHOMEs — never pollute the user's keyring.
GH_TEST="$(mktemp -d)"
GH_WRONG="$(mktemp -d)"
chmod 700 "${GH_TEST}" "${GH_WRONG}"
trap 'rm -rf "${GH_TEST}" "${GH_WRONG}"' EXIT

echo "generating test signing key..."
GNUPGHOME="${GH_TEST}" gpg --batch --quiet --pinentry-mode loopback \
    --passphrase '' \
    --quick-generate-key 'yt-dlp-ui test signer (do not trust)' ed25519 sign 2y

echo "generating wrong key..."
GNUPGHOME="${GH_WRONG}" gpg --batch --quiet --pinentry-mode loopback \
    --passphrase '' \
    --quick-generate-key 'yt-dlp-ui wrong key (do not trust)' ed25519 sign 2y

echo "exporting wrong-key.asc (used as the imported public key in the GPG-failure bats test)"
GNUPGHOME="${GH_WRONG}" gpg --batch --quiet --armor --export > "${FIX_DIR}/wrong-key.asc"

echo "exporting test-signer.asc (record of which key signed valid-sig; not used at test time)"
GNUPGHOME="${GH_TEST}" gpg --batch --quiet --armor --export > "${FIX_DIR}/test-signer.asc"

echo "writing valid-binary.bin and tampered-binary.bin"
printf 'yt-dlp test fixture binary v1\n' > "${FIX_DIR}/valid-binary.bin"
printf 'yt-dlp tampered binary v1\n' > "${FIX_DIR}/tampered-binary.bin"

# valid-sha256sums emulates upstream's SHA2-256SUMS file: each row is
# "<hex>  <asset-name>". The hex matches valid-binary.bin's actual SHA256
# under EVERY asset name we map to in fetch-yt-dlp.sh, so the bats tests
# can run any target-triple branch and find a row.
echo "computing SHA256 and writing valid-sha256sums"
SHA="$(shasum -a 256 "${FIX_DIR}/valid-binary.bin" | awk '{print $1}')"
{
    printf '%s  yt-dlp_linux\n' "${SHA}"
    printf '%s  yt-dlp_macos\n' "${SHA}"
    printf '%s  yt-dlp_linux_aarch64\n' "${SHA}"
    printf '%s  yt-dlp.exe\n' "${SHA}"
} > "${FIX_DIR}/valid-sha256sums"

echo "signing valid-sha256sums with the test key (detached, ASCII-armored)"
GNUPGHOME="${GH_TEST}" gpg --batch --quiet --pinentry-mode loopback \
    --passphrase '' \
    --detach-sign --armor \
    --output "${FIX_DIR}/valid-sig" \
    "${FIX_DIR}/valid-sha256sums"

echo "done. Files in ${FIX_DIR}:"
ls -la "${FIX_DIR}"
