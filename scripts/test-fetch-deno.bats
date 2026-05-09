#!/usr/bin/env bats
# Hermetic tests for scripts/fetch-deno.sh.
#
# Strategy:
#   - Parser-shape tests source `scripts/lib-deno-sha.sh` directly and assert
#     `parse_deno_sha256sum_file` against committed real-upstream fixtures
#     (Unix and Windows v2.7.14 sha256sum shapes) and synthetic
#     malformed / empty inputs.
#   - Full-flow tests use a PATH-shim curl that serves a per-test fake zip
#     (built at setup time) plus a per-test `.sha256sum` whose hash matches
#     the fake zip's hash. DENO_BASE_URL is pinned at a sentinel host so
#     the stub-curl can route patterns deterministically.
#
# Fixtures live in scripts/tests/fixtures/:
#   - deno-v2.7.14-unix.sha256sum     (real upstream, captured via curl)
#   - deno-v2.7.14-windows.sha256sum  (real upstream, captured via curl)
#   - malformed.sha256sum             (literal "not a valid hash file" text)
#   - empty.sha256sum                 (zero bytes)
#
# Parallel tests for the PowerShell port live in test-fetch-ps1.ps1.

# Upstream-published lower-case hashes for the captured v2.7.14 fixtures.
# These are literal compares — if upstream republishes the v2.7.14 assets
# with different bytes, regenerate the fixtures (curl -fsSL ...) and update
# these constants.
readonly UPSTREAM_UNIX_SHA="3287efef53606966469cb6a02781327be22b908959397f976e2996dc1b64ae0f"
readonly UPSTREAM_WINDOWS_SHA="25f9871f5c1d9e999d60071f8069767134495fd601d2e2c7ce1e8c641487bda0"

setup_file() {
    SCRIPT_REPO_ROOT="$(cd "${BATS_TEST_DIRNAME}/.." && pwd)"
    export SCRIPT_REPO_ROOT
    export FIXTURES_DIR="${SCRIPT_REPO_ROOT}/scripts/tests/fixtures"
    export FETCH_SCRIPT="${SCRIPT_REPO_ROOT}/scripts/fetch-deno.sh"
    export LIB_SCRIPT="${SCRIPT_REPO_ROOT}/scripts/lib-deno-sha.sh"

    if [[ ! -f "${LIB_SCRIPT}" ]]; then
        echo "lib-deno-sha.sh missing: ${LIB_SCRIPT}" >&2
        return 1
    fi
    if [[ ! -f "${FETCH_SCRIPT}" ]]; then
        echo "fetch-deno.sh missing: ${FETCH_SCRIPT}" >&2
        return 1
    fi
    for fx in deno-v2.7.14-unix.sha256sum deno-v2.7.14-windows.sha256sum \
              malformed.sha256sum empty.sha256sum; do
        if [[ ! -e "${FIXTURES_DIR}/${fx}" ]]; then
            echo "fixture missing: ${FIXTURES_DIR}/${fx}" >&2
            return 1
        fi
    done
}

setup() {
    SANDBOX="$(mktemp -d 2>/dev/null || mktemp -d -t bats-fetch-deno)"
    export SANDBOX
    export STUB_BIN="${SANDBOX}/bin"
    export OUT_DIR="${SANDBOX}/out"
    export ARCHIVE_DIR="${SANDBOX}/archives"
    export SHA_DIR="${SANDBOX}/shas"
    mkdir -p "${STUB_BIN}" "${OUT_DIR}" "${ARCHIVE_DIR}" "${SHA_DIR}"

    # Stub curl: routes URL → archive bytes or per-test sha file. Order
    # matters — `.zip.sha256sum` must match before `.zip`.
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
    *.zip.sha256sum)
        if [[ -z "${SHA_FIXTURE:-}" || ! -f "${SHA_FIXTURE}" ]]; then
            echo "stub-curl: SHA_FIXTURE not set or missing (${SHA_FIXTURE:-<unset>})" >&2
            exit 4
        fi
        cp "${SHA_FIXTURE}" "${out}"
        ;;
    *.zip)
        if [[ -z "${ZIP_FIXTURE:-}" || ! -f "${ZIP_FIXTURE}" ]]; then
            echo "stub-curl: ZIP_FIXTURE not set or missing (${ZIP_FIXTURE:-<unset>})" >&2
            exit 4
        fi
        cp "${ZIP_FIXTURE}" "${out}"
        ;;
    *)
        echo "stub-curl: unhandled URL ${url}" >&2
        exit 3
        ;;
esac
STUBEOF
    chmod +x "${STUB_BIN}/curl"

    # Pin a sentinel base-URL so stub-curl's pattern matching is the routing
    # layer, not GitHub. Pin a sentinel DENO_VERSION; the script only uses
    # it inside the URL.
    export DENO_BASE_URL="https://stub.example/deno"
    export DENO_VERSION="2.7.14"

    # Prepend stub PATH so our curl wins.
    export PATH="${STUB_BIN}:${PATH}"
}

teardown() {
    rm -rf "${SANDBOX}" 2>/dev/null || true
}

# --- helpers -----------------------------------------------------------------

# Compute SHA256 for a path. Honours both sha256sum (Linux) and shasum (macOS).
sha256_of() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${path}" | awk '{print $1}'
    else
        shasum -a 256 "${path}" | awk '{print $1}'
    fi
}

# Build a fake zip archive that mirrors deno's release-zip layout: a single
# `deno` (or `deno.exe` for Windows triples) at the archive root. fetch-deno.sh
# extracts to `<workdir>/extract/<deno|deno.exe>` and moves to `<out-dir>/deno`.
build_fake_zip() {
    local triple="$1"
    local archive="$2"
    local stage="${SANDBOX}/stage-${triple}"
    rm -rf "${stage}"
    mkdir -p "${stage}"
    local bin_name="deno"
    if [[ "${triple}" == *windows* ]]; then
        bin_name="deno.exe"
    fi
    printf '#!/bin/sh\necho fake-deno %s\n' "${triple}" > "${stage}/${bin_name}"
    chmod +x "${stage}/${bin_name}"
    (cd "${stage}" && zip -q "${archive}" "${bin_name}")
}

# Emit a Unix-format `.sha256sum` (GNU coreutils shape, lower-case hash).
#   write_unix_format_sha <hash> <out-path> <asset-name>
write_unix_format_sha() {
    local hash="$1"
    local out="$2"
    local asset="$3"
    printf '%s  %s\n' "${hash}" "${asset}" > "${out}"
}

# Emit a Windows-format `.sha256sum` (PowerShell `Get-FileHash | Format-List`
# shape, upper-case hash, CRLF line endings, leading + trailing blank lines).
# Mirrors the exact shape upstream emits — see deno-v2.7.14-windows.sha256sum.
#   write_windows_format_sha <hash> <out-path> <asset-name>
write_windows_format_sha() {
    local hash="$1"
    local out="$2"
    local asset="$3"
    local upper
    upper="$(echo "${hash}" | tr 'a-f' 'A-F')"
    printf '\r\nAlgorithm : SHA256\r\nHash      : %s\r\nPath      : C:\\fake\\path\\%s\r\n\r\n' \
        "${upper}" "${out}.fake-${asset}" > "${out}"
}

# --- parser shape tests (lib-deno-sha.sh, no fetch flow) ---------------------

@test "parser: unix-format fixture → upstream-published lowercase hash" {
    run bash -c "source '${LIB_SCRIPT}' && parse_deno_sha256sum_file '${FIXTURES_DIR}/deno-v2.7.14-unix.sha256sum'"
    [ "$status" -eq 0 ]
    [ "$output" = "${UPSTREAM_UNIX_SHA}" ]
}

@test "parser: windows-format fixture → upstream-published lowercase hash" {
    run bash -c "source '${LIB_SCRIPT}' && parse_deno_sha256sum_file '${FIXTURES_DIR}/deno-v2.7.14-windows.sha256sum'"
    [ "$status" -eq 0 ]
    [ "$output" = "${UPSTREAM_WINDOWS_SHA}" ]
}

@test "parser: malformed file → return code 1 with 'could not parse'" {
    run bash -c "source '${LIB_SCRIPT}' && parse_deno_sha256sum_file '${FIXTURES_DIR}/malformed.sha256sum'"
    [ "$status" -eq 1 ]
    [[ "$output" == *"could not parse"* ]]
}

@test "parser: empty file → return code 1" {
    run bash -c "source '${LIB_SCRIPT}' && parse_deno_sha256sum_file '${FIXTURES_DIR}/empty.sha256sum'"
    [ "$status" -eq 1 ]
    [[ "$output" == *"could not parse"* ]]
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

@test "missing DENO_VERSION → exit 65" {
    unset DENO_VERSION
    run bash "${FETCH_SCRIPT}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 65 ]
    [[ "$output" == *"DENO_VERSION"* ]]
}

# --- full-flow success: Unix-format .sha256sum, all four Unix triples --------

@test "full success (Unix .sha256sum) — x86_64-unknown-linux-gnu" {
    local triple="x86_64-unknown-linux-gnu"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha
    sha="$(sha256_of "${archive}")"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    write_unix_format_sha "${sha}" "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/deno" ]
    [ -x "${OUT_DIR}/deno" ]
    # No .exe extension on Unix output (canonical-name decision, UC 06).
    [ ! -f "${OUT_DIR}/deno.exe" ]
}

@test "full success (Unix .sha256sum) — aarch64-unknown-linux-gnu" {
    local triple="aarch64-unknown-linux-gnu"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha
    sha="$(sha256_of "${archive}")"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    write_unix_format_sha "${sha}" "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/deno" ]
    [ -x "${OUT_DIR}/deno" ]
}

@test "full success (Unix .sha256sum) — x86_64-apple-darwin" {
    local triple="x86_64-apple-darwin"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha
    sha="$(sha256_of "${archive}")"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    write_unix_format_sha "${sha}" "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/deno" ]
    [ -x "${OUT_DIR}/deno" ]
}

@test "full success (Unix .sha256sum) — aarch64-apple-darwin" {
    local triple="aarch64-apple-darwin"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha
    sha="$(sha256_of "${archive}")"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    write_unix_format_sha "${sha}" "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/deno" ]
    [ -x "${OUT_DIR}/deno" ]
}

# --- full-flow success: Windows-format .sha256sum ----------------------------

@test "full success (Windows .sha256sum) — x86_64-pc-windows-msvc" {
    local triple="x86_64-pc-windows-msvc"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha
    sha="$(sha256_of "${archive}")"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    write_windows_format_sha "${sha}" "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/deno" ]
    # Canonical-name-on-every-OS contract: archive carried `deno.exe`, but
    # destination is `deno` (no extension) — paths.rs Windows branch probes
    # `deno.exe` first and falls back to `deno`.
    [ ! -f "${OUT_DIR}/deno.exe" ]
}

# --- failure paths -----------------------------------------------------------

@test "SHA mismatch (Unix-format) → exit 73" {
    local triple="x86_64-unknown-linux-gnu"
    local asset="deno-${triple}.zip"
    local archive="${ARCHIVE_DIR}/${asset}"
    build_fake_zip "${triple}" "${archive}"
    local sha_file="${SHA_DIR}/${asset}.sha256sum"
    # Deliberately wrong hash (all zeros) — does not match the fake zip's SHA.
    write_unix_format_sha \
        "0000000000000000000000000000000000000000000000000000000000000000" \
        "${sha_file}" "${asset}"

    export ZIP_FIXTURE="${archive}"
    export SHA_FIXTURE="${sha_file}"

    run bash "${FETCH_SCRIPT}" "${triple}" "${OUT_DIR}"
    [ "$status" -eq 73 ]
    [[ "$output" == *"sha256 mismatch"* ]]
    # Mismatched binary must NOT have been moved into OUT_DIR.
    [ ! -f "${OUT_DIR}/deno" ]
}
