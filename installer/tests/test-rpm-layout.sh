#!/usr/bin/env bash
# test-rpm-layout.sh — installer-level smoke for the .rpm produced by
# package-deb-rpm.yml + installer/nfpm.yaml.
#
# Usage: bash installer/tests/test-rpm-layout.sh <path-to-rpm>

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <path-to-rpm>" >&2
    exit 64
fi

RPM="$1"
if [[ ! -f "${RPM}" ]]; then
    echo "error: ${RPM} not found" >&2
    exit 65
fi

if ! command -v rpm >/dev/null 2>&1; then
    echo "error: rpm not available; this test requires a Fedora-family host" >&2
    exit 70
fi

EXIT_CODE=0

CONTENTS="$(rpm -qpl "${RPM}")"

for path in \
    /opt/yt-dlp-ui/yt-dlp-ui \
    /opt/yt-dlp-ui/ad-window \
    /opt/yt-dlp-ui/yt-dlp \
    /opt/yt-dlp-ui/deno \
    /usr/bin/yt-dlp-ui \
    /usr/share/doc/yt-dlp-ui/LICENSE \
    /usr/share/doc/yt-dlp-ui/yt-dlp-LICENSE.txt
do
    if echo "${CONTENTS}" | grep -qE "^${path}$"; then
        echo "ok: ${path} present"
    else
        echo "FAIL: ${path} missing from .rpm"
        EXIT_CODE=1
    fi
done

# Exec-bit / mode check via `rpm -qplv` (note: -l lists files; without -l,
# rpm -qpv emits only package metadata and contains no file paths).
PERMS="$(rpm -qplv "${RPM}")"
for path in \
    /opt/yt-dlp-ui/yt-dlp-ui \
    /opt/yt-dlp-ui/ad-window \
    /opt/yt-dlp-ui/yt-dlp \
    /opt/yt-dlp-ui/deno \
    /usr/bin/yt-dlp-ui
do
    MODE="$(echo "${PERMS}" | awk -v p="${path}" '$NF == p {print $1}')"
    if [[ "${MODE}" == "-rwxr-xr-x" ]]; then
        echo "ok: exec bit on ${path}"
    else
        echo "FAIL: exec bit on ${path} (got '${MODE}', expected '-rwxr-xr-x')"
        EXIT_CODE=1
    fi
done

# Requires (Fedora/openSUSE names).
REQUIRES="$(rpm -qpR "${RPM}")"
for req in webkit2gtk4.1 libsoup3; do
    if echo "${REQUIRES}" | grep -qE "^${req}([[:space:]]|$)"; then
        echo "ok: requires ${req}"
    else
        echo "FAIL: ${req} missing from Requires"
        EXIT_CODE=1
    fi
done

exit "${EXIT_CODE}"
