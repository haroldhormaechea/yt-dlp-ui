#!/usr/bin/env bash
# lib-net-retry.sh — transient-HTTP retry helper for all fetch scripts.
#
# Source this file, then call download_with_retry <url> <dest>.
#
# Retry policy (mirrors lib-net-retry.ps1 — dual-script discipline applies):
#   - 3 total attempts per URL.
#   - Exponential back-off: 1 s before attempt 2, 4 s before attempt 3.
#   - Retries on: any curl network error, HTTP 5xx, HTTP 429 (rate-limit).
#   - No retry on: HTTP 4xx except 429 (404 = stale version pin; 403 = auth;
#     silent retry would mask a genuine pin regression).
#   - Integrity verification (SHA256, GPG) is NEVER inside this wrapper — it
#     runs exactly once on the final payload AFTER download succeeds.
#
# Exit codes from download_with_retry:
#   0 — download succeeded (HTTP 2xx).
#   1 — non-retriable failure (4xx) or exhausted retries; caller should exit.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib-net-retry.sh"
#   download_with_retry "${URL}" "${DEST_FILE}"
#
# This library is paired with lib-net-retry.ps1 — every change in one MUST
# land in the other in the same PR (see scripts/README.md § Dual-script
# discipline).

# download_with_retry <url> <dest>
#
# Wraps `curl --fail` with up to 3 attempts and exponential back-off.
# On HTTP 4xx (except 429), probes with a HEAD request to confirm the status
# and gives up immediately — a 4xx from a pinned release URL is a permanent
# error (stale version pin, deleted release, auth required) that retrying
# cannot fix.
download_with_retry() {
    local url="$1"
    local dest="$2"
    local attempt exit_code delay http_code
    local _err_tmp
    _err_tmp="$(mktemp)"

    for attempt in 1 2 3; do
        if curl --fail --silent --show-error --location --max-time 60 \
               --output "${dest}" "${url}" 2>"${_err_tmp}"; then
            rm -f "${_err_tmp}"
            return 0
        fi
        exit_code=$?

        # `curl --fail` exits 22 for HTTP 400+ responses (both 4xx and 5xx).
        # Do a lightweight HEAD probe to get the exact status code so we can
        # decide whether to retry.
        if [[ $exit_code -eq 22 ]]; then
            http_code="$(curl --silent --head --max-time 15 \
                              --write-out '%{http_code}' --output /dev/null \
                              "${url}" 2>/dev/null || true)"
            http_code="${http_code:-000}"
            # 4xx except 429: don't retry — it's a configuration problem, not
            # a transient network issue.
            if [[ "$http_code" == 4?? && "$http_code" != "429" ]]; then
                echo "error: HTTP ${http_code} fetching ${url}" >&2
                echo "  4xx response — not retrying (check version pin or base URL)" >&2
                cat "${_err_tmp}" >&2
                rm -f "${_err_tmp}"
                return 1
            fi
        fi

        if [[ $attempt -lt 3 ]]; then
            case $attempt in
                1) delay=1 ;;
                *) delay=4 ;;
            esac
            echo "download attempt ${attempt}/3 failed (curl exit ${exit_code}); retrying in ${delay}s…" >&2
            cat "${_err_tmp}" >&2
            sleep "${delay}"
        else
            echo "error: all 3 download attempts failed for ${url} (curl exit ${exit_code})" >&2
            cat "${_err_tmp}" >&2
            rm -f "${_err_tmp}"
            return 1
        fi
    done
}
