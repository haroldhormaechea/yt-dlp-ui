# 0011 — macOS signing and notarization

- **Status:** accepted
- **Date:** 2026-05-10

## Context

Use case 26 traced a complete launch failure on macOS 26.3.1 (Apple
Silicon) for the unsigned universal `.dmg` produced by the UC 06 release
pipeline. The reporter's `sample` capture pinned the parent `yt-dlp-ui`
binary at offset 0 of `_dyld_start` with a 96 KB physical footprint —
the kernel suspends the process at the very first dyld instruction
pending an `AppleSystemPolicy` decision; `syspolicyd` returns a deny
verdict; the process never reaches `main()`. Combined with the kernel
log line `kernel: (AppleSystemPolicy) ASP: Security policy would not
allow process: <pid>, /Applications/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui`,
the failure mode is unambiguous: macOS 26.x enforces hardened-runtime +
Developer ID + notarization at exec time, INDEPENDENTLY of the
Gatekeeper "open anyway" assessment override the user already granted.

The Posture 3 decision recorded in `PROJECT_BRIEF.md` § Deployment §
Code signing (defer all signing until traction triggers fire) anticipated
SmartScreen / Gatekeeper *warnings* — not a hard launch block. macOS 26
moved the goalposts: the unsigned-binary posture is no longer just a
"users see a scary dialog" UX concern, it is a "the app does not run on
arm64 macOS 26" correctness bug. Posture 3 is therefore obsolete on
macOS specifically, ahead of the original three-trigger schedule.

Three constraints shape the choice:

1. **Scope.** The fix has to work on macOS 26.3.1 arm64 (the failing
   environment) without regressing macOS 11–15 (the supported floor per
   ADR 0010 and the bumped `LSMinimumSystemVersion`). Linux and Windows
   posture is unchanged — Posture 3 still holds for those OSes until
   their own forcing functions appear.
2. **Bundled-binary diversity.** The `.app` ships four executable
   Mach-Os: the parent `yt-dlp-ui` (Rust + Slint), the `ad-window` child
   helper (wry/WebKit), `Contents/Resources/yt-dlp` (PyInstaller-frozen
   Python), `Contents/Resources/deno` (V8-JIT), and
   `Contents/Resources/ffmpeg` (statically built C). Each has different
   hardened-runtime requirements; a single shared entitlements file
   would either over-grant (yt-dlp's
   `disable-library-validation` applied to ffmpeg, weakening ffmpeg's
   posture for no benefit) or under-grant (omitting yt-dlp's required
   entitlement and leaving the PyInstaller bootloader bouncing).
3. **CI surface.** The release pipeline is GitHub Actions; the
   `package-dmg.yml` job already does the universal-binary lipo-merge
   and `.dmg` creation. Adding signing means importing a Developer ID
   cert into a temporary keychain on a disposable runner, signing in
   place, and cleaning up — a well-trodden GHA pattern but ours alone
   to maintain.

## Decision

**On macOS only, the project upgrades from Posture 3 (skip-all-signing)
to Posture 1 (Developer ID + hardened runtime + notarization +
stapling).** Linux + Windows remain on Posture 3 until their own
triggers fire (per the unchanged
`PROJECT_BRIEF.md` § Deployment § Code signing trigger schedule).

**Five components:**

1. **Hardened runtime** on every Mach-O via `codesign --options
   runtime`.
2. **Per-binary entitlements** via the directory contract documented in
   `installer/macos-signing.sh`. Five files live under
   `installer/entitlements/`:

   | Binary | File | Entitlements |
   |---|---|---|
   | `yt-dlp-ui` (parent + outer .app) | `yt-dlp-ui.entitlements` | `allow-jit` (defensive) |
   | `ad-window` | `ad-window.entitlements` | `allow-jit` (wry/WebKit V8 JIT) |
   | `yt-dlp` | `yt-dlp.entitlements` | `disable-library-validation` + `allow-unsigned-executable-memory` (PyInstaller — non-negotiable) |
   | `deno` | `deno.entitlements` | `allow-jit` + `allow-unsigned-executable-memory` (V8) |
   | `ffmpeg` | `ffmpeg.entitlements` | empty `<dict/>` (deliberate review-trail marker) |

   The contract: a missing per-binary file means "sign without
   `--entitlements`" (binary needs none); an existing file (even an
   empty `<dict/>`) means "sign with `--entitlements <file>`" (someone
   reviewed this and made an explicit decision). UC 28's future
   `ffprobe` binary will follow the same convention by dropping a
   `Resources/ffprobe.entitlements` with an empty `<dict/>`.

3. **Children-first deep sign.** `installer/macos-signing.sh::deep_sign_app`
   walks `Contents/MacOS/*` and `Contents/Resources/*`, filters to
   Mach-O via `file ... | grep -q Mach-O`, signs each child with its
   per-binary entitlements lookup, then signs the outer `.app` last
   with `yt-dlp-ui.entitlements`. The order matters: the outer-bundle
   signature is computed over the inner files including their signature
   load commands, so signing the outer first invalidates the inner
   children.

4. **Notarization of the `.dmg` only.** The release-pipeline
   `package-dmg.yml` submits the signed `.dmg` (not the bare `.app`) to
   `xcrun notarytool submit --wait --output-format json`. On
   non-Accepted, the helper dumps `xcrun notarytool log <submission-id>`
   to the GHA log so the failure cause is visible without a follow-up
   round trip. App Store Connect API key auth is preferred over
   `APPLE_ID_USERNAME` because the API key is single-purpose and
   revocable without disturbing the publisher's Apple-ID password.

5. **DMG-only stapling.** `xcrun stapler staple` writes the
   notarization ticket into the `.dmg`; `xcrun stapler validate`
   confirms it parses. The `.app` extracted from the mounted `.dmg`
   does NOT carry its own stapled ticket — Gatekeeper validates the
   `.dmg`'s signature and ticket at install/copy time, after which the
   user's drag-to-/Applications copy is inheritable-trusted under the
   per-app cdhash. Single round-trip; sufficient.

**Codesign / notarytool / stapler invocation shape:**

Deep-sign order (children first, all `--options runtime --timestamp
--force --sign "$IDENTITY"` plus per-binary `--entitlements`):

1. `Contents/Resources/yt-dlp` — disable-library-validation +
   allow-unsigned-executable-memory.
2. `Contents/Resources/ffmpeg` — empty `<dict/>`.
3. `Contents/Resources/deno` — allow-jit + allow-unsigned-executable-memory.
4. `Contents/MacOS/ad-window` — allow-jit.
5. `Contents/MacOS/yt-dlp-ui` — allow-jit (defensive).
6. The outer `yt-dlp-ui.app` — yt-dlp-ui.entitlements.

Verify: `codesign --verify --deep --strict --verbose=2 yt-dlp-ui.app`
followed by `spctl --assess --type execute --verbose=4 yt-dlp-ui.app`.

Sign DMG: `codesign --force --sign "$IDENTITY" --timestamp <dmg>` (no
`--options runtime` — DMGs are not executables; hardened runtime is
meaningless on a disk image).

Notarize: `xcrun notarytool submit <dmg> --key /tmp/<id>.p8 --key-id
"$KEY_ID" --issuer "$ISSUER" --wait --output-format json`. On
non-Accepted, the helper fetches `notarytool log <submission-id>` and
dumps to stderr so the GHA log shows the cause.

Staple: `xcrun stapler staple <dmg>` then `xcrun stapler validate <dmg>`.

**Secret inventory (six secrets — single source of truth, mirrored to
`README.md` § macOS release prerequisites and to
`.github/workflows/package-dmg.yml`):**

1. `APPLE_TEAM_ID` — 10-char Apple Developer team identifier.
2. `APP_STORE_CONNECT_API_KEY_ID` — App Store Connect API key id.
3. `APP_STORE_CONNECT_API_KEY_ISSUER_ID` — issuer id paired with the key.
4. `APP_STORE_CONNECT_API_KEY_P8` — base64-encoded `.p8` key file.
5. `MACOS_CERTIFICATE` — base64-encoded `.p12` Developer ID Application
   cert export.
6. `MACOS_CERTIFICATE_PASSWORD` — `.p12` export password.

`MACOS_KEYCHAIN_PASSWORD` is **not** a stored secret — the temporary
keychain's password is generated inline via `openssl rand -hex 32` and
lives only in the running job's process env. Adding it as a 7th secret
would create a long-lived credential without buying any property
rotating it could not. `APPLE_ID_USERNAME` is also not used —
`notarytool` API-key auth is preferred.

**Workflow gating.** All sign / notarize / staple / cleanup steps in
`package-dmg.yml` are gated on `if: env.MACOS_CERTIFICATE != ''`. A
PR-from-fork build, or a master build that runs before the secrets are
provisioned, sees an empty `MACOS_CERTIFICATE` and skips the entire
signing block, producing the same unsigned `.dmg` the pre-UC-26
pipeline produced. The cleanup-keychain step uses `if: always() &&
env.MACOS_CERTIFICATE != ''` so a partial failure (cert imported but
codesign exploded) still clears the runner's keychain.

**`LSMinimumSystemVersion`.** Bumped from 10.13 → 11.0. Hardened
runtime requires ≥ 10.14.5; ADR 0010's macOS ffmpeg already targets
11.0; lifting the floor to 11.0 collapses the fragmentation and
matches the actually-tested support range.

## Cert rotation cadence and compromise response

**Rotation.** Apple Developer ID Application certs expire every 5
years from issuance. Rotation procedure:

1. Generate a CSR locally; submit via developer.apple.com → Account →
   Certificates → Developer ID Application.
2. Download the new `.cer`; import into Keychain Access; export to
   `.p12` with a fresh export password.
3. Base64-encode the `.p12` (`base64 -i cert.p12 | pbcopy` on macOS).
4. Update GHA repo secrets `MACOS_CERTIFICATE` (the new base64 blob)
   and `MACOS_CERTIFICATE_PASSWORD` (the new export password).
5. Re-tag and re-run the release pipeline against a candidate tag to
   confirm the new cert signs and notarizes cleanly before deleting
   the old `.p12`.

**Compromise response.** If the `.p12` or its export password leak:

1. Revoke the cert in developer.apple.com → Certificates immediately.
   Apple's revocation lists propagate to Gatekeeper within hours.
2. Rotate `MACOS_CERTIFICATE` and `MACOS_CERTIFICATE_PASSWORD` to a
   freshly issued cert (per the rotation procedure above). Past
   releases stamped with the revoked cert may still work for users who
   had already downloaded and validated them; new downloads from
   GitHub Releases of the same artifact will fail Gatekeeper.
3. Cut a fresh release tag with the new cert. Update the GitHub
   Release notes for any affected previous tags pointing users at the
   new build.
4. Audit the GHA workflow run logs around the leak window for any
   confirmation that the secrets were exfiltrated; rotate any other
   secrets with the same exposure window (the App Store Connect API
   key's `.p8` lives in the same secret store).

## Consequences

**Positive:**
- macOS 26.3.1 arm64 launch works. The UC 26 root cause is closed.
- The `.app` carries a Developer ID signature that ties bundled
  artifacts to this project's identity, satisfying THREATS.md § T9's
  "users cannot cryptographically verify they downloaded the genuine
  artifact" mitigation gap on macOS.
- Hardened runtime + per-binary entitlements give a per-binary review
  trail. A future security-conscious maintainer can `codesign -d
  --entitlements -` any bundled binary and see exactly what permissions
  it was granted, no source-archeology needed.
- The DMG-only stapling path is a single round-trip per release; no
  double-stapling state machine to debug.

**Negative:**
- **Six new long-lived secrets to manage** (the inventory above). Each
  is a rotation surface and a compromise vector. The rotation cadence
  documented above is the answer.
- **$99/year Apple Developer Program membership** is now a hard release
  cost. Lapsing the membership invalidates the cert and breaks future
  releases until renewed.
- **Notarization throughput is Apple-rate-limited.** A failed
  submission (network blip, bad entitlement, transient Apple outage)
  can stall a release for tens of minutes. The `--wait` flag gives us
  Accepted-or-failed verdicts in the same job, which is the right
  trade-off; an async submission would push verdict-handling into a
  follow-up workflow.
- **The dev loop on macOS is unchanged.** `cargo run` does not touch
  codesign — `scripts/macos-signing-local.sh` exists as an opt-in
  helper for the rare maintainer flow of "sign locally before tagging,"
  but it is not part of the day-to-day dev cycle. This is by design;
  forcing every dev to import a real Developer ID cert just to compile
  would be unnecessary friction.

## Alternatives considered

- **Hardened runtime without notarization.** Rejected: macOS 26.x's
  AppleSystemPolicy denial extends past the Gatekeeper assessment that
  hardened-runtime alone clears. Without notarization, `syspolicyd`
  has no Apple-side acceptance record to consult and the bounce-and-die
  reproduces.

- **Double-stapling app and dmg.** Rejected: stapling the inner `.app`
  AND the outer `.dmg` is two notarization round-trips for no extra
  validation property. Gatekeeper validates the `.dmg`'s signature and
  ticket at install/copy time; the `.app` copied to `/Applications`
  inherits trust under its own cdhash. Apple's own tooling docs
  recommend stapling the outermost distributable artifact only.

- **A single shared entitlements file across all bundled Mach-Os.**
  Rejected: a shared file forces either over-granting
  (`disable-library-validation` applied to ffmpeg purely because
  yt-dlp needs it, weakening ffmpeg's posture) or under-granting
  (omitting yt-dlp's required entitlement, leaving PyInstaller
  bouncing). The per-binary directory contract is one extra file per
  binary in exchange for a per-binary review trail.

- **Long-lived `MACOS_KEYCHAIN_PASSWORD` as a 7th GHA secret.**
  Rejected: the keychain's password is a per-job credential that exists
  for as long as the runner; an inline `openssl rand -hex 32` produces
  a strong password without adding a long-lived secret to the
  inventory. Maintaining a 7th secret with no rotation property to
  protect is unnecessary overhead.

- **Posture 1 across all OSes immediately.** Rejected: Linux and
  Windows have not had a forcing function. The original Posture 3
  trigger schedule (100+ stars / 1000+ downloads / 6 months active
  maintenance) still applies for those OSes. Upgrading them now would
  burden the project with `signtool` cert costs ($199–699/year) and
  Linux package-signing infrastructure for no current correctness
  benefit. Each OS's posture upgrade should be triggered by its own
  forcing function.

## References

- use-cases/26-fix-macos-arm64-launch-failure.md (the failure
  reproduction, `sample` analysis, and corrected H4 hypothesis)
- ADR 0010 (FFmpeg bundling — the pin context that drives the
  `MACOSX_DEPLOYMENT_TARGET=11.0` build target and pairs with the
  `LSMinimumSystemVersion` bump here)
- THREATS.md § T9 (transitions from "unsigned MVP" to "Developer-ID-
  signed on macOS, unsigned elsewhere")
- THREATS.md § T13 (bundled binaries now project-Developer-ID-signed
  in addition to the existing fetch-time SHA-256 / GPG verification)
- THREATS.md § T15 (the new long-lived secret: the Developer ID cert
  — numbered T15 because T14 was already in use for the UC 20
  licensing transition)
- PROJECT_BRIEF.md § Deployment § Code signing (records the macOS-only
  Posture-3 → Posture-1 upgrade)
