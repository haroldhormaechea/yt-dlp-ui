#!/usr/bin/env bash
# macos-signing.sh — shared bash library for the macOS signing,
# notarization, and stapling pipeline.
#
# This file is `source`-d, not executed standalone. Functions defined
# here are called from:
#
#   1. .github/workflows/package-dmg.yml — the release pipeline. Imports
#      the Developer ID Application cert, deep-signs the universal .app,
#      signs the .dmg, submits to notarytool, polls until accepted, and
#      staples. Cleanup-keychain runs in the always() trailer.
#
#   2. installer/build-macos-dmg.sh — for the actual deep_sign_app and
#      DMG-codesign calls. The build script gates on $MACOS_SIGNING_IDENTITY
#      so an unset identity short-circuits to the pre-UC-26 unsigned path
#      bit-for-bit (PR-from-fork CI, pre-credential master builds).
#
#   3. scripts/macos-signing-local.sh — local one-off bundle validation
#      against a developer's own Developer ID. Indirect — the local
#      helper sources THIS file and re-uses deep_sign_app + assess_app.
#
# ─── Per-binary entitlements convention (the contract) ────────────────
#
# `deep_sign_app` walks Contents/MacOS/* and Contents/Resources/* in the
# given .app, filters to Mach-O files (`file <path> | grep -q Mach-O`),
# and for each one looks up:
#
#     <entitlements_dir>/<basename>.entitlements
#
# Two outcomes:
#
# • File MISSING:        sign without --entitlements. The binary inherits
#                        the codesign default (no entitlements at all),
#                        which is the right answer for a binary that
#                        genuinely needs none.
#
# • File EXISTS:         pass --entitlements <path>. THIS HOLDS EVEN IF
#   (incl. <dict/> only) THE FILE IS AN EMPTY <dict/>. An empty plist is
#                        a deliberate review-trail marker: someone looked
#                        at this binary, decided it needs no entitlements,
#                        and wants future maintainers to make the same
#                        explicit decision instead of reflexively adding
#                        disable-library-validation. Passing
#                        --entitlements with an empty dict produces a
#                        signature with an empty entitlements blob (still
#                        recorded by codesign), which is auditable.
#
# UC 28's future ffprobe binary will follow this convention: drop a
# Resources/ffprobe.entitlements with an empty <dict/> alongside the
# binary and `deep_sign_app` will pick it up automatically.
#
# Sign order (mandatory): children FIRST, then the outer .app. macOS
# requires inner Mach-O signatures to exist before the outer-bundle
# signature is computed; signing the outer bundle first invalidates the
# inner-children's signatures because the outer signature is over the
# inner files' bytes including their signature LCs.

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────
# setup_temp_keychain — create an ephemeral keychain and import the
# Developer ID Application cert into it. The keychain password is
# generated INLINE via `openssl rand -hex 32` and is NOT a stored
# secret; it lives only as long as this CI job. The password is
# captured via process-env and the keychain is removed in the
# always-trailer cleanup_temp_keychain.
#
# Required env (caller must export):
#   MACOS_CERTIFICATE          — base64-encoded .p12 cert blob
#   MACOS_CERTIFICATE_PASSWORD — .p12 export password
#
# Outputs:
#   $TEMP_KEYCHAIN              — full path to the created keychain
#   $TEMP_KEYCHAIN_PASSWORD     — the inline-generated password
#   $MACOS_SIGNING_IDENTITY     — the codesign-ready CN of the imported
#                                 cert (extracted via `security
#                                 find-identity -v -p codesigning`)
#
# Both are exported via $GITHUB_ENV when running under GHA so subsequent
# steps can codesign without re-importing.
# ─────────────────────────────────────────────────────────────────────
setup_temp_keychain() {
    local keychain_path="${RUNNER_TEMP:-/tmp}/yt-dlp-ui-signing.keychain-db"
    local cert_path="${RUNNER_TEMP:-/tmp}/yt-dlp-ui-cert.p12"

    # Generate a fresh keychain password per run. NOT a stored secret —
    # never written to disk outside this keychain's own metadata, never
    # surfaced beyond this job's env.
    local keychain_password
    keychain_password="$(openssl rand -hex 32)"

    # Decode the .p12 from base64 into a temp file for `security import`.
    if [[ -z "${MACOS_CERTIFICATE:-}" ]]; then
        echo "error: MACOS_CERTIFICATE env var is empty" >&2
        return 65
    fi
    if [[ -z "${MACOS_CERTIFICATE_PASSWORD:-}" ]]; then
        echo "error: MACOS_CERTIFICATE_PASSWORD env var is empty" >&2
        return 65
    fi
    printf '%s' "${MACOS_CERTIFICATE}" | base64 --decode > "${cert_path}"

    # Create + unlock + import. `set-keychain-settings -lut 21600` keeps
    # the keychain unlocked for the duration of a 6-hour job ceiling.
    security create-keychain -p "${keychain_password}" "${keychain_path}"
    security set-keychain-settings -lut 21600 "${keychain_path}"
    security unlock-keychain -p "${keychain_password}" "${keychain_path}"
    security import "${cert_path}" \
        -P "${MACOS_CERTIFICATE_PASSWORD}" \
        -A \
        -t cert \
        -f pkcs12 \
        -k "${keychain_path}"

    # Allow codesign to use the cert without prompting for keychain
    # access.
    security set-key-partition-list \
        -S apple-tool:,apple:,codesign: \
        -s \
        -k "${keychain_password}" \
        "${keychain_path}" >/dev/null

    # Prepend the new keychain to the search list so codesign finds the
    # imported identity. Preserves the system + login keychains.
    local -a existing_keychains
    # shellcheck disable=SC2207
    existing_keychains=( $(security list-keychains -d user | tr -d '"') )
    security list-keychains -d user -s "${keychain_path}" "${existing_keychains[@]}"

    # Wipe the decoded .p12 — the cert is now in the keychain.
    rm -f "${cert_path}"

    # Extract the codesigning identity name from the keychain. The
    # `find-identity` output looks like:
    #   1) ABC123... "Developer ID Application: Foo Bar (TEAMID)"
    # We want the quoted CN.
    local identity
    identity="$(security find-identity -v -p codesigning "${keychain_path}" \
        | grep -m 1 'Developer ID Application' \
        | sed -E 's/^[[:space:]]*[0-9]+\)[[:space:]]+[A-F0-9]+[[:space:]]+"(.+)"$/\1/')"

    if [[ -z "${identity}" ]]; then
        echo "error: no Developer ID Application identity found in ${keychain_path}" >&2
        return 75
    fi

    export TEMP_KEYCHAIN="${keychain_path}"
    export TEMP_KEYCHAIN_PASSWORD="${keychain_password}"
    export MACOS_SIGNING_IDENTITY="${identity}"

    if [[ -n "${GITHUB_ENV:-}" ]]; then
        {
            echo "TEMP_KEYCHAIN=${keychain_path}"
            echo "TEMP_KEYCHAIN_PASSWORD=${keychain_password}"
            echo "MACOS_SIGNING_IDENTITY=${identity}"
        } >> "${GITHUB_ENV}"
    fi

    echo "imported codesigning identity: ${identity}"
}

# ─────────────────────────────────────────────────────────────────────
# cleanup_temp_keychain — remove the ephemeral keychain. Invoked from
# the workflow's always()-gated cleanup step so a partial failure does
# not leave a keychain with the imported cert lying around on the
# runner. Idempotent (`security delete-keychain` no-ops on missing).
# ─────────────────────────────────────────────────────────────────────
cleanup_temp_keychain() {
    if [[ -n "${TEMP_KEYCHAIN:-}" && -f "${TEMP_KEYCHAIN}" ]]; then
        security delete-keychain "${TEMP_KEYCHAIN}" || true
        echo "deleted ${TEMP_KEYCHAIN}"
    else
        echo "no temp keychain to clean up"
    fi
}

# ─────────────────────────────────────────────────────────────────────
# deep_sign_app — children-first deep sign of a .app bundle.
#
# Args:
#   $1 — path to the .app bundle
#   $2 — codesigning identity (a CN string, e.g. "Developer ID
#        Application: Foo Bar (TEAMID)")
#   $3 — path to the entitlements directory (typically
#        installer/entitlements/)
#
# For each Mach-O found under Contents/MacOS/ and Contents/Resources/
# (filtered via `file ... | grep -q Mach-O`), this function:
#   • looks up <entitlements_dir>/<basename>.entitlements
#   • signs with --options runtime --timestamp --force --sign $IDENTITY
#   • passes --entitlements only if the file exists (see contract above)
#
# After every child is signed, signs the outer .app. The outer-bundle
# signature ALWAYS uses the parent binary's entitlements file
# (yt-dlp-ui.entitlements) because that's the one applied to the bundle
# as a whole.
# ─────────────────────────────────────────────────────────────────────
deep_sign_app() {
    local app_path="$1"
    local identity="$2"
    local entitlements_dir="$3"

    if [[ ! -d "${app_path}" ]]; then
        echo "error: ${app_path} is not a directory" >&2
        return 65
    fi
    if [[ -z "${identity}" ]]; then
        echo "error: deep_sign_app called with empty identity" >&2
        return 65
    fi
    if [[ ! -d "${entitlements_dir}" ]]; then
        echo "error: ${entitlements_dir} is not a directory" >&2
        return 65
    fi

    local -a sign_dirs=("${app_path}/Contents/MacOS" "${app_path}/Contents/Resources")
    local d binary basename ent_file
    for d in "${sign_dirs[@]}"; do
        if [[ ! -d "${d}" ]]; then
            continue
        fi
        # NOTE: we deliberately glob with -maxdepth 1 — we only sign
        # binaries that are direct children of MacOS/ or Resources/.
        # Embedded frameworks live under Contents/Frameworks/ and would
        # need their own walk; this app does not ship any today.
        while IFS= read -r -d '' binary; do
            # Skip non-Mach-O files (LICENSE texts, plist resources,
            # the .app's own Info.plist isn't in these dirs but be
            # defensive).
            if ! file "${binary}" 2>/dev/null | grep -q 'Mach-O'; then
                continue
            fi
            basename="$(basename "${binary}")"
            ent_file="${entitlements_dir}/${basename}.entitlements"

            if [[ -f "${ent_file}" ]]; then
                echo "signing ${binary} with entitlements ${ent_file}"
                codesign \
                    --options runtime \
                    --timestamp \
                    --force \
                    --entitlements "${ent_file}" \
                    --sign "${identity}" \
                    "${binary}"
            else
                echo "signing ${binary} (no entitlements file at ${ent_file})"
                codesign \
                    --options runtime \
                    --timestamp \
                    --force \
                    --sign "${identity}" \
                    "${binary}"
            fi
        done < <(find "${d}" -maxdepth 1 -type f -print0)
    done

    # Outer .app last. Apply the parent binary's entitlements at the
    # bundle level — that's what AMFI sees when launching the .app.
    local outer_ent="${entitlements_dir}/yt-dlp-ui.entitlements"
    echo "signing outer bundle ${app_path} with entitlements ${outer_ent}"
    codesign \
        --options runtime \
        --timestamp \
        --force \
        --entitlements "${outer_ent}" \
        --sign "${identity}" \
        "${app_path}"
}

# ─────────────────────────────────────────────────────────────────────
# notarize_dmg — submit a .dmg to Apple's notary service and wait for
# the verdict. On a non-Accepted result, fetches the detailed log
# (`notarytool log <submission-id>`) and dumps it to stderr / GHA log.
#
# Args:
#   $1 — path to the .dmg
#   $2 — path to the App Store Connect API key (.p8 file)
#   $3 — App Store Connect key id
#   $4 — App Store Connect issuer id
#
# Uses --output-format json so the submission id is parseable without
# regex-spelunking the human output.
# ─────────────────────────────────────────────────────────────────────
notarize_dmg() {
    local dmg="$1"
    local key_p8="$2"
    local key_id="$3"
    local issuer="$4"

    if [[ ! -f "${dmg}" ]]; then
        echo "error: ${dmg} is not a file" >&2
        return 65
    fi
    if [[ ! -f "${key_p8}" ]]; then
        echo "error: ${key_p8} is not a file" >&2
        return 65
    fi

    echo "submitting ${dmg} to notarytool"
    local submit_json
    submit_json="$(xcrun notarytool submit "${dmg}" \
        --key "${key_p8}" \
        --key-id "${key_id}" \
        --issuer "${issuer}" \
        --wait \
        --output-format json)"

    echo "${submit_json}"

    # Pull out id and status. Avoid jq because we don't want to require
    # `brew install jq` on the GHA runner. The JSON shape is small and
    # documented; sed works fine.
    local submission_id status
    submission_id="$(printf '%s' "${submit_json}" \
        | sed -nE 's/.*"id":[[:space:]]*"([^"]+)".*/\1/p' | head -n 1)"
    status="$(printf '%s' "${submit_json}" \
        | sed -nE 's/.*"status":[[:space:]]*"([^"]+)".*/\1/p' | head -n 1)"

    echo "submission id: ${submission_id}"
    echo "submission status: ${status}"

    if [[ "${status}" != "Accepted" ]]; then
        echo "notarization not Accepted — fetching detailed log" >&2
        if [[ -n "${submission_id}" ]]; then
            xcrun notarytool log "${submission_id}" \
                --key "${key_p8}" \
                --key-id "${key_id}" \
                --issuer "${issuer}" >&2 || true
        fi
        return 75
    fi
}

# ─────────────────────────────────────────────────────────────────────
# staple — embed the notarization ticket in the artifact and verify it.
# Two-step: `stapler staple` writes the ticket; `stapler validate`
# confirms it parses and matches the artifact's signature.
#
# Arg:
#   $1 — path to the .dmg or .app to staple
# ─────────────────────────────────────────────────────────────────────
staple() {
    local target="$1"

    if [[ ! -e "${target}" ]]; then
        echo "error: ${target} does not exist" >&2
        return 65
    fi

    echo "stapling ${target}"
    xcrun stapler staple "${target}"

    echo "validating staple on ${target}"
    xcrun stapler validate "${target}"
}

# ─────────────────────────────────────────────────────────────────────
# assess_app — local Gatekeeper assessment of a signed .app. Two
# checks:
#   • `codesign --verify --deep --strict --verbose=2` — every nested
#     Mach-O signs correctly and the outer bundle hashes match the
#     inner files.
#   • `spctl --assess --type execute --verbose=4` — Gatekeeper's view
#     (what a user double-clicking the .app would hit). On macOS 26.x
#     this also includes the AppleSystemPolicy decision that UC 26
#     traced as the launch failure root cause.
#
# Note: a passing assess_app on a notarized+stapled .app from a Dev ID
# cert is the closest we can get to "this will launch on a clean macOS
# 26 install" without actually shipping it through the GHA pipeline.
#
# Arg:
#   $1 — path to the .app
# ─────────────────────────────────────────────────────────────────────
assess_app() {
    local app="$1"

    if [[ ! -d "${app}" ]]; then
        echo "error: ${app} is not a directory" >&2
        return 65
    fi

    echo "codesign --verify --deep --strict --verbose=2 ${app}"
    codesign --verify --deep --strict --verbose=2 "${app}"

    echo "spctl --assess --type execute --verbose=4 ${app}"
    spctl --assess --type execute --verbose=4 "${app}"
}
