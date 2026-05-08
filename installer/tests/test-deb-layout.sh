#!/usr/bin/env bash
# test-deb-layout.sh — installer-level smoke for the .deb produced by
# package-deb-rpm.yml + installer/nfpm.yaml.
#
# Verifies the file layout, exec bits, and dependency declarations of a
# built .deb. Runs in CI (Linux runners) and locally on Linux developer
# workstations after `nfpm package --packager deb --config installer/nfpm.yaml`.
#
# Usage: bash installer/tests/test-deb-layout.sh <path-to-deb>

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <path-to-deb>" >&2
    exit 64
fi

DEB="$1"
if [[ ! -f "${DEB}" ]]; then
    echo "error: ${DEB} not found" >&2
    exit 65
fi

if ! command -v dpkg-deb >/dev/null 2>&1; then
    echo "error: dpkg-deb not available; this test requires a Debian-family host" >&2
    exit 70
fi

EXIT_CODE=0
check() {
    local desc="$1"
    local expected="$2"
    local actual="$3"
    if [[ "${expected}" == "${actual}" ]]; then
        echo "ok: ${desc}"
    else
        echo "FAIL: ${desc} (expected '${expected}', got '${actual}')"
        EXIT_CODE=1
    fi
}

CONTENTS="$(dpkg-deb -c "${DEB}")"

for path in \
    /opt/yt-dlp-ui/yt-dlp-ui \
    /opt/yt-dlp-ui/ad-window \
    /opt/yt-dlp-ui/yt-dlp \
    /opt/yt-dlp-ui/deno \
    /usr/bin/yt-dlp-ui \
    /usr/share/doc/yt-dlp-ui/LICENSE \
    /usr/share/doc/yt-dlp-ui/yt-dlp-LICENSE.txt
do
    if echo "${CONTENTS}" | grep -qE "[[:space:]]\.${path}$"; then
        echo "ok: ${path} present"
    else
        echo "FAIL: ${path} missing from .deb"
        EXIT_CODE=1
    fi
done

# Exec-bit checks on the four binaries + the launcher wrapper.
for path in \
    /opt/yt-dlp-ui/yt-dlp-ui \
    /opt/yt-dlp-ui/ad-window \
    /opt/yt-dlp-ui/yt-dlp \
    /opt/yt-dlp-ui/deno \
    /usr/bin/yt-dlp-ui
do
    MODE="$(echo "${CONTENTS}" | awk -v p=".${path}" '$NF == p {print $1}')"
    check "exec bit on ${path}" "-rwxr-xr-x" "${MODE}"
done

# Depends declaration must list the Debian/Ubuntu names.
DEPENDS="$(dpkg-deb -f "${DEB}" Depends 2>/dev/null || true)"
for dep in libwebkit2gtk-4.1-0 libsoup-3.0-0; do
    if [[ "${DEPENDS}" == *"${dep}"* ]]; then
        echo "ok: depends on ${dep}"
    else
        echo "FAIL: ${dep} missing from Depends ('${DEPENDS}')"
        EXIT_CODE=1
    fi
done

exit "${EXIT_CODE}"
