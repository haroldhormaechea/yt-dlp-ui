#!/usr/bin/env bats
# Hermetic tests for scripts/fetch-yt-dlp.sh.
#
# Strategy:
#   - PATH shim replaces `curl` with a script that emits fixture bytes
#     selected by URL pattern (no network).
#   - REPO_ROOT is pointed at a per-test tempdir whose `scripts/keys/yt-dlp.asc`
#     is either the test signer's key (success) or the wrong key (GPG fail).
#   - YT_DLP_BASE_URL points at a sentinel host so the stub-curl can
#     route URL patterns deterministically.
#
# Fixtures live in scripts/tests/fixtures/ and are produced by
# scripts/tests/fixtures/generate-fixtures.sh.

setup_file() {
    SCRIPT_REPO_ROOT="$(cd "${BATS_TEST_DIRNAME}/.." && pwd)"
    export SCRIPT_REPO_ROOT
    export FIXTURES_DIR="${SCRIPT_REPO_ROOT}/scripts/tests/fixtures"
    export FETCH_SCRIPT="${SCRIPT_REPO_ROOT}/scripts/fetch-yt-dlp.sh"

    if [[ ! -d "${FIXTURES_DIR}" ]]; then
        echo "fixtures dir missing: ${FIXTURES_DIR}" >&2
        return 1
    fi
    if [[ ! -x "${FETCH_SCRIPT}" ]]; then
        echo "fetch-yt-dlp.sh not executable: ${FETCH_SCRIPT}" >&2
        return 1
    fi
}

setup() {
    # Per-test sandbox: stub-curl, fake REPO_ROOT, fake output dir.
    SANDBOX="$(mktemp -d 2>/dev/null || mktemp -d -t bats-fetch-yt-dlp)"
    export SANDBOX
    export STUB_BIN="${SANDBOX}/bin"
    export FAKE_REPO="${SANDBOX}/repo"
    export OUT_DIR="${SANDBOX}/out"
    mkdir -p "${STUB_BIN}" "${FAKE_REPO}" "${OUT_DIR}"

    # Stub curl: routes URL → fixture file. Flag handling is intentionally
    # loose; we only honor `--output` since that's what fetch-yt-dlp.sh uses.
    cat > "${STUB_BIN}/curl" <<'STUBEOF'
#!/usr/bin/env bash
out=""
url=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --output)
            out="$2"
            shift 2
            ;;
        --fail|--silent|--show-error|--location|-fsSL|-fsS|-L|-s|-S|-f)
            shift
            ;;
        http*|https*)
            url="$1"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if [[ -z "${out}" || -z "${url}" ]]; then
    echo "stub-curl: missing --output or url (got out=${out} url=${url})" >&2
    exit 2
fi

case "${url}" in
    *SHA2-256SUMS.sig)
        cp "${STUB_FIXTURES}/valid-sig" "${out}"
        ;;
    *SHA2-256SUMS)
        cp "${STUB_FIXTURES}/valid-sha256sums" "${out}"
        ;;
    *yt-dlp_linux|*yt-dlp_macos|*yt-dlp_linux_aarch64|*yt-dlp.exe)
        cp "${STUB_FIXTURES}/${STUB_BINARY:-valid-binary.bin}" "${out}"
        ;;
    *)
        echo "stub-curl: unhandled URL ${url}" >&2
        exit 3
        ;;
esac
STUBEOF
    chmod +x "${STUB_BIN}/curl"

    export STUB_FIXTURES="${FIXTURES_DIR}"

    # Default REPO_ROOT has the matching test-signer key — success path.
    mkdir -p "${FAKE_REPO}/scripts/keys"
    cp "${FIXTURES_DIR}/test-signer.asc" "${FAKE_REPO}/scripts/keys/yt-dlp.asc"

    # Pin a base-URL the stub-curl recognises (any value works; it only
    # routes by basename). Pin a sentinel YT_DLP_VERSION too.
    export YT_DLP_BASE_URL="https://stub.example/release"
    export YT_DLP_VERSION="0.0.0-test"
    export REPO_ROOT="${FAKE_REPO}"

    # Prepend stub PATH so our curl wins over /usr/bin/curl.
    export PATH="${STUB_BIN}:${PATH}"
}

teardown() {
    rm -rf "${SANDBOX}" 2>/dev/null || true
}

# --- argv / env contract -----------------------------------------------------

@test "missing argv → exit 64" {
    run bash "${FETCH_SCRIPT}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"usage:"* ]]
}

@test "one argv → exit 64" {
    run bash "${FETCH_SCRIPT}" only-one
    [ "$status" -eq 64 ]
}

@test "missing YT_DLP_VERSION → exit 65" {
    unset YT_DLP_VERSION
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 65 ]
    [[ "$output" == *"YT_DLP_VERSION"* ]]
}

@test "unknown target triple → exit 64" {
    run bash "${FETCH_SCRIPT}" potato-unknown-triple "${OUT_DIR}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"unknown target triple"* ]]
}

@test "missing yt-dlp.asc → exit 75" {
    rm -f "${FAKE_REPO}/scripts/keys/yt-dlp.asc"
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 75 ]
    [[ "$output" == *"yt-dlp.asc not found"* ]]
}

# --- target → asset mapping --------------------------------------------------

@test "x86_64-unknown-linux-gnu → success path produces canonical 'yt-dlp'" {
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/yt-dlp" ]
    [ -x "${OUT_DIR}/yt-dlp" ]
    # No .exe extension on Unix output (canonical-name decision, UC 06 Smoke 1).
    [ ! -f "${OUT_DIR}/yt-dlp.exe" ]
}

@test "aarch64-unknown-linux-gnu → success path" {
    run bash "${FETCH_SCRIPT}" aarch64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/yt-dlp" ]
}

@test "x86_64-apple-darwin → success path" {
    run bash "${FETCH_SCRIPT}" x86_64-apple-darwin "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/yt-dlp" ]
}

@test "aarch64-apple-darwin → success path" {
    run bash "${FETCH_SCRIPT}" aarch64-apple-darwin "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/yt-dlp" ]
}

@test "x86_64-pc-windows-msvc → success path produces canonical 'yt-dlp' (no .exe)" {
    run bash "${FETCH_SCRIPT}" x86_64-pc-windows-msvc "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/yt-dlp" ]
}

# --- verification failure paths ----------------------------------------------

@test "SHA mismatch → exit 73" {
    # Tampered binary's bytes don't match the SHA in valid-sha256sums.
    export STUB_BINARY="tampered-binary.bin"
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 73 ]
    [[ "$output" == *"sha256 mismatch"* ]]
    # The mismatched binary should NOT have been moved into OUT_DIR.
    [ ! -f "${OUT_DIR}/yt-dlp" ]
}

@test "GPG verify failure → exit 74" {
    # Swap the imported yt-dlp.asc for the wrong key — verification fails.
    cp "${FIXTURES_DIR}/wrong-key.asc" "${FAKE_REPO}/scripts/keys/yt-dlp.asc"
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 74 ]
    [[ "$output" == *"GPG verification failed"* ]]
    [ ! -f "${OUT_DIR}/yt-dlp" ]
}
