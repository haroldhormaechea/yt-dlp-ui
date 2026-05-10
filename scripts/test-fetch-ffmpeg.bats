#!/usr/bin/env bats
# Hermetic tests for scripts/fetch-ffmpeg.sh.
#
# Strategy:
#   - PATH shim replaces `curl` with a script that emits fixture bytes
#     selected by URL pattern (no network).
#   - Each test builds a fresh archive (tar.xz on Linux, zip on Windows) at
#     setup time, computes its SHA256, and writes a per-test pins file.
#   - FFMPEG_BASE_URL points at a sentinel host so the stub-curl can route
#     URL patterns deterministically.
#
# Why no shared fixture archive: BtbN's archive layout (single top-level
# directory containing `bin/ffmpeg`) is reproduced inside the test by
# staging the directory tree and packing it on the fly. Keeps the fixtures
# directory text-only and avoids committing binary archives.
#
# Parallel tests for the PowerShell port live in test-fetch-ffmpeg.ps1.

setup_file() {
    SCRIPT_REPO_ROOT="$(cd "${BATS_TEST_DIRNAME}/.." && pwd)"
    export SCRIPT_REPO_ROOT
    export FETCH_SCRIPT="${SCRIPT_REPO_ROOT}/scripts/fetch-ffmpeg.sh"

    if [[ ! -x "${FETCH_SCRIPT}" ]]; then
        echo "fetch-ffmpeg.sh not executable: ${FETCH_SCRIPT}" >&2
        return 1
    fi
}

setup() {
    SANDBOX="$(mktemp -d 2>/dev/null || mktemp -d -t bats-fetch-ffmpeg)"
    export SANDBOX
    export STUB_BIN="${SANDBOX}/bin"
    export OUT_DIR="${SANDBOX}/out"
    export PINS_FILE="${SANDBOX}/runtime-deps-pins.env"
    export ARCHIVE_DIR="${SANDBOX}/archives"
    mkdir -p "${STUB_BIN}" "${OUT_DIR}" "${ARCHIVE_DIR}"

    # Build the three archives the closed-set asset map cares about. Each
    # archive contains a top-level directory matching the asset name (minus
    # the extension) with a `bin/ffmpeg` (or `bin/ffmpeg.exe`) file inside,
    # mirroring the BtbN layout fetch-ffmpeg.sh's locate-binary loop scans.
    build_archives

    # Per-test pins file — SHAs match the freshly built archives so the
    # in-tree pin check passes in the happy path. Negative tests override
    # individual SHA values to force mismatches.
    write_pins_file

    # Stub curl: routes URL → archive / sidecar bytes.
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

# Archives + per-asset .sha256 sidecars by asset filename.
case "${url}" in
    *.tar.xz.sha256|*.zip.sha256)
        # Per-asset .sha256 sidecar — sha256 of the archive, single line.
        asset_url="${url%.sha256}"
        asset_name="$(basename "${asset_url}")"
        archive="${ARCHIVE_DIR}/${asset_name}"
        if [[ ! -f "${archive}" ]]; then
            echo "stub-curl: no archive at ${archive} for ${url}" >&2
            exit 4
        fi
        if command -v sha256sum >/dev/null 2>&1; then
            sha="$(sha256sum "${archive}" | awk '{print $1}')"
        else
            sha="$(shasum -a 256 "${archive}" | awk '{print $1}')"
        fi
        # Emit "<sha>  <asset>" — fetch-ffmpeg.sh awks field 1.
        printf '%s  %s\n' "${sha}" "${asset_name}" > "${out}"
        ;;
    *.tar.xz|*.zip)
        asset_name="$(basename "${url}")"
        archive="${ARCHIVE_DIR}/${asset_name}"
        if [[ ! -f "${archive}" ]]; then
            echo "stub-curl: no archive at ${archive} for ${url}" >&2
            exit 4
        fi
        cp "${archive}" "${out}"
        ;;
    *)
        echo "stub-curl: unhandled URL ${url}" >&2
        exit 3
        ;;
esac
STUBEOF
    chmod +x "${STUB_BIN}/curl"

    # Pin a sentinel base URL so the stub-curl recognises patterns by basename.
    export FFMPEG_BASE_URL="https://stub.example/release"
    # Override the default pins-file path: fetch-ffmpeg.sh resolves it via
    # SCRIPT_DIR. Easiest hermetic strategy is symlinking our pins file in
    # place of the real one; see below.
    export PATH="${STUB_BIN}:${PATH}"
}

teardown() {
    rm -rf "${SANDBOX}" 2>/dev/null || true
}

# --- helpers -----------------------------------------------------------------

# Build the three target-triple archives the script's closed-set map covers.
# Each archive's top-level dir matches the asset name (minus extension), with
# `bin/ffmpeg` (or `bin/ffmpeg.exe` for Windows) inside. License text is
# placed at the top-level so the script's LICENSE-locate loop finds it.
build_archives() {
    local release_tag="n7.1.4"
    for triple_pair in \
        "linux64:tar.xz" \
        "linuxarm64:tar.xz" \
        "win64:zip"; do
        local triple="${triple_pair%%:*}"
        local ext="${triple_pair##*:}"
        local asset_base="ffmpeg-${release_tag}-${triple}-lgpl-7.1"
        local asset="${asset_base}.${ext}"
        local stage="${SANDBOX}/stage-${triple}"
        mkdir -p "${stage}/${asset_base}/bin"
        if [[ "${triple}" == "win64" ]]; then
            printf '#!/bin/sh\nexit 0\n' > "${stage}/${asset_base}/bin/ffmpeg.exe"
        else
            printf '#!/bin/sh\nexit 0\n' > "${stage}/${asset_base}/bin/ffmpeg"
            chmod +x "${stage}/${asset_base}/bin/ffmpeg"
        fi
        printf 'LGPL-2.1 fixture license\n' > "${stage}/${asset_base}/LICENSE.txt"

        if [[ "${ext}" == "tar.xz" ]]; then
            (cd "${stage}" && tar -cJf "${ARCHIVE_DIR}/${asset}" "${asset_base}")
        else
            (cd "${stage}" && zip -qr "${ARCHIVE_DIR}/${asset}" "${asset_base}")
        fi
    done
}

# Compute SHA256 for a path. Honors sha256sum / shasum.
sha256_of() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${path}" | awk '{print $1}'
    else
        shasum -a 256 "${path}" | awk '{print $1}'
    fi
}

# Write a fresh pins file matching the freshly built archive SHAs. The
# script source's `${PINS_FILE}` resolution is `${SCRIPT_DIR}/runtime-deps-pins.env`,
# so we must overlay our pins onto that exact location for the test run.
# Strategy: copy the script and pins file into the SANDBOX as a unit and
# call the copy. This avoids touching the real repo file.
write_pins_file() {
    local linux64_sha
    local linuxarm64_sha
    local win64_sha
    linux64_sha="$(sha256_of "${ARCHIVE_DIR}/ffmpeg-n7.1.4-linux64-lgpl-7.1.tar.xz")"
    linuxarm64_sha="$(sha256_of "${ARCHIVE_DIR}/ffmpeg-n7.1.4-linuxarm64-lgpl-7.1.tar.xz")"
    win64_sha="$(sha256_of "${ARCHIVE_DIR}/ffmpeg-n7.1.4-win64-lgpl-7.1.zip")"

    cat > "${PINS_FILE}" <<EOF
FFMPEG_VERSION=autobuild-test
FFMPEG_RELEASE_TAG=n7.1.4
FFMPEG_SHA256_LINUX64=${linux64_sha}
FFMPEG_SHA256_LINUXARM64=${linuxarm64_sha}
FFMPEG_SHA256_WIN64=${win64_sha}
FFMPEG_VERSION_SOURCE=7.1
FFMPEG_TARBALL_SHA256_SOURCE=0000000000000000000000000000000000000000000000000000000000000000
EOF
}

# Stage a runnable copy of the fetch script alongside the test pins file
# (the script resolves PINS_FILE via SCRIPT_DIR at runtime). Returns the
# path to the staged copy via stdout.
stage_script() {
    local script_dir="${SANDBOX}/script_dir"
    mkdir -p "${script_dir}"
    cp "${FETCH_SCRIPT}" "${script_dir}/fetch-ffmpeg.sh"
    cp "${PINS_FILE}" "${script_dir}/runtime-deps-pins.env"
    # fetch-ffmpeg.sh sources lib-net-retry.sh from its own SCRIPT_DIR.
    # Stage it alongside so the staged copy can resolve the dependency.
    cp "${SCRIPT_REPO_ROOT}/scripts/lib-net-retry.sh" "${script_dir}/lib-net-retry.sh"
    echo "${script_dir}/fetch-ffmpeg.sh"
}

# --- argv / env contract -----------------------------------------------------

@test "missing argv → exit 64" {
    local script
    script="$(stage_script)"
    run bash "${script}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"usage:"* ]]
}

@test "one argv → exit 64" {
    local script
    script="$(stage_script)"
    run bash "${script}" only-one
    [ "$status" -eq 64 ]
}

@test "missing pins file → exit 75" {
    # Stage the script WITHOUT a pins file alongside it. lib-net-retry.sh
    # must still be present — it's sourced unconditionally before the pins
    # check, so omitting it would mask the pins-file error.
    local script_dir="${SANDBOX}/script_dir_no_pins"
    mkdir -p "${script_dir}"
    cp "${FETCH_SCRIPT}" "${script_dir}/fetch-ffmpeg.sh"
    cp "${SCRIPT_REPO_ROOT}/scripts/lib-net-retry.sh" "${script_dir}/lib-net-retry.sh"
    run bash "${script_dir}/fetch-ffmpeg.sh" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 75 ]
    [[ "$output" == *"pins file not found"* ]]
}

# --- target → asset mapping --------------------------------------------------

@test "x86_64-unknown-linux-gnu → success path produces canonical 'ffmpeg' (no ext)" {
    local script
    script="$(stage_script)"
    run bash "${script}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/ffmpeg" ]
    [ -x "${OUT_DIR}/ffmpeg" ]
    # No .exe on Unix output.
    [ ! -f "${OUT_DIR}/ffmpeg.exe" ]
    # License text dropped alongside per fetch-ffmpeg.sh's redistribution rule.
    [ -f "${OUT_DIR}/ffmpeg-LICENSE.txt" ]
    # Stub-built archive carried "LGPL-2.1 fixture license" — assert the
    # license text was actually copied from the archive (not the stub).
    grep -q "LGPL-2.1 fixture license" "${OUT_DIR}/ffmpeg-LICENSE.txt"
}

@test "aarch64-unknown-linux-gnu → success path resolves linuxarm64 asset" {
    local script
    script="$(stage_script)"
    run bash "${script}" aarch64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/ffmpeg" ]
    [[ "$output" == *"linuxarm64-lgpl"* ]]
}

@test "x86_64-pc-windows-msvc → success path produces canonical 'ffmpeg' (no .exe)" {
    local script
    script="$(stage_script)"
    run bash "${script}" x86_64-pc-windows-msvc "${OUT_DIR}"
    [ "$status" -eq 0 ]
    [ -f "${OUT_DIR}/ffmpeg" ]
    # Canonical-name-on-every-OS contract: input archive has ffmpeg.exe but
    # destination is named `ffmpeg` (no extension), matching paths.rs Windows
    # canonical-name fallback.
    [ ! -f "${OUT_DIR}/ffmpeg.exe" ]
}

# --- macOS hard-fail ---------------------------------------------------------

@test "x86_64-apple-darwin → exit 64 with build-ffmpeg-macos.sh redirect" {
    local script
    script="$(stage_script)"
    run bash "${script}" x86_64-apple-darwin "${OUT_DIR}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"build-ffmpeg-macos.sh"* ]]
    [ ! -f "${OUT_DIR}/ffmpeg" ]
}

@test "aarch64-apple-darwin → exit 64 with build-ffmpeg-macos.sh redirect" {
    local script
    script="$(stage_script)"
    run bash "${script}" aarch64-apple-darwin "${OUT_DIR}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"build-ffmpeg-macos.sh"* ]]
}

# --- closed-set guard --------------------------------------------------------

@test "FFMPEG_ASSET env override is ignored — closed set prevents asset rewrite" {
    # The script does not honor an FFMPEG_ASSET env override; the asset name
    # is hardcoded per target triple inside the case branches. Setting the
    # env should NOT change which file is fetched.
    local script
    script="$(stage_script)"
    export FFMPEG_ASSET="ffmpeg-master-latest-linux64-gpl-shared.tar.xz"
    run bash "${script}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 0 ]
    # The success line confirms the linux64-LGPL asset was fetched, not
    # the GPL-shared one we tried to inject via env.
    [[ "$output" == *"linux64-lgpl"* ]]
    [[ "$output" != *"gpl-shared"* ]]
}

@test "unknown target triple → exit 64 with 'unknown target' message" {
    local script
    script="$(stage_script)"
    run bash "${script}" aarch64-unknown-freebsd "${OUT_DIR}"
    [ "$status" -eq 64 ]
    [[ "$output" == *"unknown target triple"* ]]
}

# --- supply-chain verification -----------------------------------------------

@test "in-tree SHA mismatch → exit 73" {
    # Tamper the pins file so the in-tree pin no longer matches the freshly
    # built archive.
    local script
    script="$(stage_script)"
    # Rewrite the LINUX64 pin to an obviously bogus value.
    sed -i.bak \
        -e "s/^FFMPEG_SHA256_LINUX64=.*/FFMPEG_SHA256_LINUX64=0000000000000000000000000000000000000000000000000000000000000000/" \
        "$(dirname "${script}")/runtime-deps-pins.env"

    run bash "${script}" x86_64-unknown-linux-gnu "${OUT_DIR}"
    [ "$status" -eq 73 ]
    [[ "$output" == *"sha256"* || "$output" == *"SHA256"* ]]
    [ ! -f "${OUT_DIR}/ffmpeg" ]
}
