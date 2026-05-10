# Threat model — yt-dlp-ui

This document enumerates the security-relevant assumptions, attack surfaces,
and mitigations for `yt-dlp-ui`. It is a hard MVP deliverable, not deferred
to production maturity, because the project ships executables to non-technical
users, integrates a third-party ad-network SDK, and runs untrusted user input
(URLs) through a bundled subprocess.

This is a **living document**. Update it when:
- A new attack surface is introduced (e.g., adding network listeners,
  changing the IPC channel, integrating a different ad vendor).
- An assumption changes (e.g., adopting OS sandboxing, or moving from
  unsigned binaries to a signed-release posture).
- A real incident teaches us something the model missed.

## Scope and audience

- **Scope:** the UI / installer code in this repository (everything under
  `crates/`, `examples/`, `docs/`, the workspace manifests, and the per-OS
  installer artifacts produced by `cargo-dist` and the snap workflow).
- **Out of scope:** the upstream yt-dlp Python tree (covered by upstream's
  own threat posture), and the user's broader operating system / network /
  filesystem.
- **Audience:** contributors, security reviewers, and forkers who need to
  understand the trust assumptions before changing anything.

## Trust boundaries (recap from PROJECT_BRIEF.md § Architecture)

1. **Source-level read-only zone:** the upstream yt-dlp Python tree. Never
   modified at the source level.
2. **Runtime invocation surface:** only the bundled `yt-dlp` standalone
   binary is executed at runtime. The Python tree is never imported or
   interpreted.
3. **`app` ↔ `ad-window` IPC:** newline-delimited JSON over stdin/stdout
   pipes. Kernel-enforced parent/child boundary; no third-party process can
   attach.
4. **`ad-window` privilege envelope:** read its own stdin, write its own
   stdout/stderr, make outbound HTTPS for ad creative, read/write its own
   per-process WebView cache directory. May NOT open the SQLite database,
   spawn further subprocesses, or read files outside the cache dir.
5. **Ad webview boundary:** ad creative is treated as untrusted. No shared
   JS bridge with `app`. Click navigations are intercepted and opened in
   the user's default browser.

## Threat catalog

### T1. Untrusted user-supplied URLs

**Surface:** the app accepts arbitrary URLs from the user and passes them to
the bundled `yt-dlp` binary.

**Risks:**
- Shell-injection if URLs were ever interpolated into a shell command line.
- `yt-dlp`-side vulnerabilities triggered by hostile URLs (e.g., format
  spec edge cases, malicious server responses).

**Mitigations:**
- URLs are passed as positional arguments via
  `Command::new(yt_dlp_path).arg(url)`. **Never** shelled out via `sh -c` or
  equivalent. There is no shell involved at any point.
- yt-dlp is responsible for its own URL validation and server-response
  handling. Vulnerabilities discovered upstream are mitigated by bumping
  the pinned `YT_DLP_VERSION` in CI.
- **UC 26 / macOS hardened-runtime narrowing:** on macOS the post-exec
  attack surface is now narrower than on Linux/Windows. Even if a
  hostile URL succeeded in coaxing `yt-dlp` into loading attacker-
  controlled native code, the bundled binary runs under the project's
  Developer ID with hardened-runtime enabled (per ADR 0011); macOS
  AMFI rejects unsigned-executable-memory writes and library-validation
  bypasses except where the `installer/entitlements/yt-dlp.entitlements`
  file explicitly grants them (and that grant is scoped to the
  PyInstaller bootloader's existing dlopen pattern, not arbitrary
  attacker code paths). This is defense-in-depth on top of the no-shell
  invocation, not a replacement for it.

### T2. Untrusted ad creative

**Surface:** the `ad-window` process loads ad creative (HTML, CSS, JS,
images) from third-party endpoints into a system WebView.

**Risks:**
- Malicious creative attempting to escape the WebView, exfiltrate user data,
  or pivot into the main `app` process.
- Drive-by downloads triggered by ad-served redirects.
- Cross-origin scripting against the WebView.

**Mitigations:**
- The ad WebView runs in a **separate OS process** (`ad-window`) with no
  shared memory, no IPC, and no JS bridge to `app`.
- The WebView is the system WebView (WebKit / WebView2), which is regularly
  patched by the OS vendor; we do not maintain our own WebView.
- Click navigations are intercepted at the navigation event and opened in
  the user's **default browser** via `open` / `xdg-open` / `start`. No
  navigation is ever followed inside the ad WebView itself.
- The `ad-window` cache directory is per-process and isolated from the
  rest of the user's filesystem; cleaned up on exit.

### T3. Untrusted third-party ad-SDK code

**Surface:** the chosen ad-network SDK (vendor TBD) ships JavaScript that
collects telemetry by design — device fingerprint, behavioral signals,
identifiers, and similar data.

**Risks:**
- Privacy leakage that the user did not understand they were consenting to.
- Vendor-side compromise leading to telemetry-as-malware.
- Sudden vendor policy changes that violate the project's stated posture.

**Mitigations:**
- **First-launch consent flow** with plain-language disclosure (per
  PROJECT_BRIEF.md § Monetization), including a link to the SDK vendor's
  privacy policy. Persistent settings entry naming the SDK.
- The ad SDK runs **only inside the `ad-window` process**, never inside
  `app`. If the user disables ads in settings, `ad-window` is never spawned
  and no SDK code executes.
- Vendor selection itself is a future decision; revisit this section once a
  vendor is chosen and surface any vendor-specific risks.

### T4. Supply-chain risk via the bundled `yt-dlp` binary

**Surface:** the release pipeline downloads upstream yt-dlp's prebuilt
standalone binary from GitHub Releases at build time and ships it inside
each installer.

**Risks:**
- A compromised upstream release could ship malicious code through our
  installers.
- A man-in-the-middle attack on the download could substitute a tampered
  binary if verification is skipped.

**Mitigations:**
- **GPG signature verification** against upstream's public key
  (`scripts/keys/yt-dlp.asc` in this repo, controlled by upstream).
- **SHA256 hash check** against upstream's published `SHA2-256SUMS` file
  for the same release.
- **Pinned version** via `YT_DLP_VERSION` env var; bumps go through PR
  review.
- Both verifications run in CI and fail the build if either does not match.

### T5. Local DB tampering

**Surface:** the SQLite database at the per-OS app-data location.

**Risks:**
- A user (or malware running as the user) modifies the queue / settings /
  history database to change app behavior.

**Decision:** **out of scope.** A single-user desktop app cannot defend
against the user's own filesystem write access. Any such defense (encrypted
DB with a key not derivable from local state, integrity HMACs sealed in
keychain) introduces complexity disproportionate to the MVP threat. The
appropriate response is to document this clearly so users understand the
boundary.

### T6. Crate supply-chain risk

**Surface:** the Cargo dependency graph (transitive Rust dependencies).

**Risks:**
- A malicious crate (typo-squat, hijacked maintainer account, intentional
  backdoor) entering the dependency graph.
- A vulnerability in an existing crate.

**Mitigations:**
- **`cargo-audit`** in CI on every PR and a weekly scheduled run, checking
  the RustSec advisory database for known CVEs.
- **`cargo-deny`** for source policy: `unknown-registry = "deny"` and
  `unknown-git = "deny"` in `deny.toml`. Only the official crates.io
  registry is permitted.
- **Dependabot** (configured at the repo level) raises automated PRs for
  dependency updates.

### T7. Bundled-crate license drift

**Surface:** transitive dependencies whose licenses are incompatible with
the project's PolyForm Noncommercial posture.

**Risks:**
- A new transitive dependency under GPL / LGPL / AGPL / SSPL would force
  the application to be open-sourced under a copyleft license, in conflict
  with the source-available, non-commercial intent.

**Mitigations:**
- **`cargo-deny`** license policy in `deny.toml`. Explicit allow-list of
  permissive SPDX identifiers (MIT, Apache-2.0, BSD, ISC, MPL-2.0, etc.).
  The complete GPL/LGPL/AGPL/SSPL family is implicitly denied because they
  are not on the allow-list.
- License check runs in CI on every PR; any new dependency with an
  unrecognized or incompatible license fails the build.

**UC 17 extension — bundled native binaries.** `cargo-deny` only inspects
the Cargo dependency graph; it does NOT see the bundled native binaries
(yt-dlp, deno, ffmpeg) shipped inside the installer. Native-binary
license drift is policed by:

- The build-time configure-flag lint in `scripts/build-ffmpeg-macos.sh`
  (rejects `--enable-libx264|libx265|libfdk-aac|gpl|nonfree` in the
  built binary's banner).
- The asset-name guard in `scripts/fetch-ffmpeg.sh` (refuses any asset
  whose filename does not contain `-lgpl-`).
- The in-tree SHA256 pin in `scripts/runtime-deps-pins.env` (rotating
  to a non-LGPL build would mismatch the pin and fail the fetch).

### T8. `app` ↔ `ad-window` IPC channel

**Surface:** newline-delimited JSON messages on stdin/stdout pipes between
`app` (parent) and `ad-window` (child).

**Risks:**
- Malformed JSON crashing either process.
- Unbounded message size exhausting memory.
- A third process attempting to inject messages.

**Mitigations:**
- Each side parses JSON with a length cap on the line buffer (8 KB ought to
  be more than enough; enforced by the read loop).
- A third process cannot attach to the pipe — the kernel enforces the
  parent/child relationship. **No authentication is needed** because there
  is no third party with access in the first place.
- Both sides treat parse errors as a recoverable event (log + skip line for
  `ad-window`; log + treat as helper crash for `app`, triggering the
  exponential-backoff respawn policy).

### T9. Unsigned binaries (Posture 3, MVP only)

**UC 26 / 2026-05-10 update — macOS-only transition.** The macOS `.dmg`
is now Posture 1 (Developer ID + hardened runtime + notarization +
DMG-only stapling), per ADR 0011 — kernel-level
`AppleSystemPolicy` enforcement on macOS 26.x made the unsigned posture
a hard launch failure on arm64 hardware, not just a UX warning. T9 still
applies to **Windows** installer artifacts; Linux distros do not warn on
unsigned packages. The Mitigations bullets below remain accurate for the
non-macOS surface.

**Surface:** the installer artifacts on Windows are unsigned at MVP per
the deliberate signing posture (PROJECT_BRIEF.md § Deployment). On
macOS, see T15 for the new long-lived-secret surface introduced by
Posture 1.

**Risks:**
- Users cannot cryptographically verify that an installer they downloaded
  was actually built by this project.
- A tampered installer hosted on a non-official mirror would be
  indistinguishable from the genuine artifact at the OS level.

**Mitigations (transitional, until Posture 1 is adopted):**
- **Distribute exclusively from GitHub Releases** on the canonical repo URL
  (`https://github.com/HaroldHormaechea/yt-dlp-ui/releases`). The README
  must repeat this prominently.
- **Publish per-asset SHA256 hashes** in each GitHub Release's release
  notes. Users (or downstream packagers) can verify the hash matches before
  running the installer.
- The signing posture decision (PROJECT_BRIEF.md § Deployment) lists
  concrete triggers for moving to Posture 1. Until then, this threat is
  acknowledged and partially compensated by the GitHub-Releases-only
  distribution channel.

### T10. Cookies-from-browser feature (UC 05)

**Surface:** when YouTube returns the bot-check error, the app offers to
read cookies from one of the user's installed browsers and forward them to
yt-dlp via `--cookies-from-browser <browser>`. yt-dlp opens the browser's
local cookie SQLite, decrypts session cookies for `*.youtube.com`, and
attaches them to the request to YouTube.

**Trust posture:**
- Cookies live and stay on the user's machine. The UI process does not
  read or relay cookies; `yt-dlp` does, and it sends them only as the
  outbound request to YouTube — the same path the user's own browser would
  take. No upload to any project-controlled server.
- The exfiltration boundary is identical to "the user using YouTube in
  their own browser." Nothing new is exposed by enabling the feature.

**Browser-cookie-DB encryption considerations:**
- macOS Chrome cookies are encrypted with a Keychain-backed key. yt-dlp's
  first cookie read triggers a Keychain prompt; the prompt is the OS
  asking the user, not the app — the app cannot bypass it.
- Linux Chrome / Chromium hold an exclusive lock on their cookie SQLite
  while the browser is running. yt-dlp surfaces a clear "database is
  locked" error in that case, which UC 05 propagates verbatim to the UI.
- Snap- and flatpak-confined browser installs may not expose their cookie
  DBs at the standard paths; yt-dlp's "cookie file not found" error path
  applies.

**Mitigations:**
- The dialog is opt-in (modal pop-up; user chooses which browser, or
  cancels entirely).
- The persisted choice is local-only (SQLite KV row); never transmitted.
- "None" is always available; the feature can be disabled per-batch
  (no "Remember") or globally (Settings → Cookies source → None).

### T11. Bundled-deno SHA256-only verification asymmetry

**Surface:** the release-time hook `scripts/fetch-deno.sh` fetches the
upstream deno binary from `https://github.com/denoland/deno/releases/...`
and verifies its SHA256 against the matching `.sha256sum` published in the
same release. This is weaker than the bundled-`yt-dlp` posture, which
verifies SHA256 *and* GPG (per `PROJECT_BRIEF.md` § Deployment).

**Risks:**
- A GitHub Releases compromise that replaced both the asset and its
  `.sha256sum` in lock-step would be undetected by the SHA-only check.
  The yt-dlp GPG verification step covers this scenario for that binary;
  deno does not currently publish GPG-signed releases (no upstream signing
  key to pin).

**Mitigations:**
- Pin `DENO_VERSION` in the release workflow; bump only via reviewed PR.
- Re-evaluate when deno ships Sigstore / cosign release attestations
  (tracking upstream: <https://github.com/denoland/deno/issues>).
- The fetch-and-verify happens in CI on a per-release basis, so a
  compromised dev workstation cannot inject a bad deno into the bundled
  artifact without also compromising the CI workflow.

### T12. Release-pipeline tooling supply chain (UC 06)

**Surface:** the GitHub Actions packaging workflows
(`.github/workflows/package-deb-rpm.yml`, `package-nsis.yml`,
`package-dmg.yml`) download two third-party tooling binaries during the
build:

1. **`nfpm`** (Linux runner, used to produce `.deb` and `.rpm`).
2. **Microsoft Evergreen WebView2 Bootstrapper** (Windows runner, bundled
   inside the NSIS installer to install WebView2 runtime on first launch
   for users on older Windows 10 without modern Edge).

**Risks:**
- A compromised upstream release of either binary could ship malicious
  code through our installers.
- A man-in-the-middle attack on the download could substitute a tampered
  binary if verification is skipped.

**Mitigations:**
- **`nfpm`** is downloaded from `github.com/goreleaser/nfpm/releases/...`
  and verified against a SHA256 hash pinned in `package-deb-rpm.yml`'s
  `env: NFPM_SHA256`. Bumping the version requires editing both the
  pinned version and SHA in the same PR.
- **WebView2 Evergreen Bootstrapper** is downloaded from a stable
  Microsoft fwlink URL and verified against a SHA256 hash pinned in
  `package-nsis.yml`'s `env: WEBVIEW2_BOOTSTRAPPER_SHA256`. Microsoft
  rotates the bootstrapper binary in place periodically (the URL points
  to "always latest"); a rotation will fail the SHA check and require a
  developer to re-pin via PR. This is the same posture as any other
  pinned third-party binary.
- **Out of supply-chain scope:** `hdiutil` (macOS-shipped, OS trust
  boundary), `lipo` (Xcode Command Line Tool, OS trust boundary), and
  `makensis` (`choco install nsis`-pinned, same trust boundary as
  `gnupg` for fetch-yt-dlp.ps1).
- **Out of supply-chain scope:** the upstream package metadata for the
  `.deb` `depends:` and `.rpm` `Requires:` declarations. Those names
  resolve at install time on the user's machine against the user's own
  trusted package mirrors (`apt`, `dnf`); we do not ship those packages.

### T13. Bundled-ffmpeg supply chain (UC 17)

**Surface:** the release pipeline produces a per-OS bundled ffmpeg binary.
The supply chain has two distinct paths:

1. **Linux + Windows.** Prebuilt LGPL-only static binary fetched from
   `BtbN/FFmpeg-Builds` (a third-party reproducible-CI redistribution of
   FFmpeg).
2. **macOS.** Built from upstream FFmpeg source on the GHA macOS runner
   (no LGPL-only mainstream macOS prebuilt exists; evermeet.cx ships
   x86_64-only and Homebrew/MacPorts include GPL components).

**Risks:**
- A compromised BtbN release could ship a tampered ffmpeg through our
  Linux/Windows installers.
- A compromise of `ffmpeg.org/releases/` could ship tampered upstream
  source code that the macOS runner builds and embeds.
- A man-in-the-middle attack on either download could substitute a
  tampered artifact if verification is skipped.
- BtbN has no GPG signing; we cannot transitively trust upstream FFmpeg
  for the prebuilt path.
- A configure-flag regression on the macOS source build path could
  re-enable GPL or nonfree components without anyone noticing.

**Mitigations:**

*Linux + Windows path:*
- **In-tree SHA256 pin** in `scripts/runtime-deps-pins.env`
  (`FFMPEG_SHA256_LINUX64`, `FFMPEG_SHA256_LINUXARM64`,
  `FFMPEG_SHA256_WIN64`). Pin rotation is a manual PR with reviewer
  diffing the new SHA against the BtbN release page.
- **Remote `<asset>.sha256` defense-in-depth** in
  `scripts/fetch-ffmpeg.sh` and `.ps1`. Catches an attacker who
  compromised the in-tree pin without also rotating BtbN's published
  per-asset checksum.
- **Belt-and-suspenders LGPL filename guard** in both fetch scripts:
  any asset whose name does not contain `-lgpl-` is rejected
  (`exit 64`) before the network fetch.
- **Pinned BtbN release tag** — never `master-latest`. `FFMPEG_VERSION`
  is a release-branded autobuild (`autobuild-YYYY-MM-DD-HH-MM`) plus
  the in-tag stable-release `FFMPEG_RELEASE_TAG` (`n7.x`).

*macOS source-build path:*
- **In-tree SHA256 pin** of the upstream FFmpeg source tarball
  (`FFMPEG_TARBALL_SHA256_SOURCE`).
- **Locked configure flags** documented in `scripts/build-ffmpeg-macos.sh`
  and ADR 0010. Any drift from the locked flags (e.g. accidentally
  enabling `--enable-libx264`) is caught by the configure-line lint at
  the tail of the build script.
- **`MACOSX_DEPLOYMENT_TARGET=11.0`** to ensure the binary is binary-
  compatible across the supported macOS range.
- **No GPG signature verification.** Upstream FFmpeg signs source
  tarballs with a long-lived PGP key, but maintaining a pinned keyring
  is itself a long-tail rotation problem (compare T11 deno). The
  in-tree SHA pin + the `https://ffmpeg.org/releases/` TLS channel are
  the ceiling of supply-chain assurance for this binary today; revisit
  if upstream FFmpeg ships Sigstore/cosign attestations.

**macOS quarantine sub-bullet.** Bundled binaries inside an unsigned
`.dmg` (Posture 3) inherit `com.apple.quarantine` from Gatekeeper on
first run. Without intervention, every auxiliary binary spawn (yt-dlp,
ffmpeg, deno) re-prompts Gatekeeper.

Mitigations applied:
- `installer/build-macos-dmg.sh` runs `xattr -cr "${APP_DIR}"` before
  `hdiutil create` to clear extended attributes from the staged bundle.
- `crates/app/src/paths.rs::strip_macos_quarantine_if_needed()` runs
  `xattr -d com.apple.quarantine` at app startup as defense-in-depth
  for cases where xattrs are re-applied (e.g. an end-user-modified
  install or a future signing transition).

**UC 26 / 2026-05-10 — project-Developer-ID signing on macOS.** In
addition to the fetch-time SHA-256 / GPG verification documented above
(yt-dlp GPG + SHA, deno + ffmpeg SHA-only with in-tree pin), every
bundled Mach-O on macOS is now also signed with the project's
**Developer ID Application** certificate at release time as part of the
deep-sign step in `installer/macos-signing.sh::deep_sign_app`:

- `Contents/MacOS/yt-dlp-ui` (parent)
- `Contents/MacOS/ad-window`
- `Contents/Resources/yt-dlp`
- `Contents/Resources/deno`
- `Contents/Resources/ffmpeg`

Two signature layers therefore protect the macOS bundled-binary surface:
the **upstream** signature/hash verified at fetch time (build-time
supply-chain check against upstream tampering), and the **project**
Developer ID signature verified at exec time (runtime check that the
binary is the one the project shipped, enforced by macOS AMFI). The
project signature is what allows the hardened-runtime + per-binary
entitlements posture documented in ADR 0011 to actually take effect on
macOS 26.x. Linux + Windows are unchanged — they remain at the
fetch-time-only verification layer until their own signing posture is
upgraded.

The new long-lived secret introduced by this signing layer (the
Developer ID Application certificate) is its own threat surface; see T15.

### T14. Repository licensing transition (UC 20)

The repo-root `LICENSE` file is now **PolyForm Noncommercial 1.0.0** —
the project's source-available, non-commercial license that applies to
all UI and installer code in this repository. Prior to UC 20 (the fork
extraction migration) the repo-root `LICENSE` was upstream yt-dlp's
**Unlicense**; it has been replaced.

The bundled `yt-dlp` binary continues to ship under upstream's
**Unlicense** terms; the canonical Unlicense text is shipped as
`installer/yt-dlp-LICENSE.txt` to the per-OS bundled-binary install path
(matching UC 17's `ffmpeg-LICENSE.txt` precedent for the bundled
`ffmpeg` binary, which remains LGPL-2.1+).

**Consequence:** the project's GitHub landing page now displays
PolyForm Noncommercial 1.0.0 instead of the Unlicense. This is a
user-visible licensing change documented in CONTRIBUTING.md and
README.md.

**Mitigations:** none required — the licensing is explicit in repo
files, in the artifact-bundled `yt-dlp-LICENSE.txt` shipped to end
users, and in `crates/app/src/about.rs`'s About-modal license listing.

### T15. Developer ID Application cert as a new long-lived secret (UC 26)

(Numbered T15 because T14 is already in use for the UC 20 licensing
transition. T15 is the next free integer.)

**Surface:** UC 26's macOS-only Posture-1 upgrade (ADR 0011) introduces
six GitHub Actions secrets stored at the repository level:

| Secret | What it is |
|---|---|
| `APPLE_TEAM_ID` | 10-char Apple Developer team identifier |
| `APP_STORE_CONNECT_API_KEY_ID` | App Store Connect API key id |
| `APP_STORE_CONNECT_API_KEY_ISSUER_ID` | Issuer id paired with the key |
| `APP_STORE_CONNECT_API_KEY_P8` | Base64-encoded `.p8` key file |
| `MACOS_CERTIFICATE` | Base64-encoded `.p12` Developer ID Application cert |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` export password |

`MACOS_KEYCHAIN_PASSWORD` is **not** a stored secret — generated inline
via `openssl rand -hex 32` per release run.

**Risks:**
- **`.p12` export disclosure.** Anyone with the cert export and its
  password can sign Mach-Os under the project's Developer ID, indirectly
  attesting to malicious code as project-built. macOS users would have
  no easy way to tell a malicious signed binary from a legitimate one
  short of comparing cdhash against a trusted release.
- **App Store Connect API key (`.p8`) disclosure.** Combined with the
  key id and issuer id, an attacker could submit malicious binaries to
  Apple's notary service under the project's identity, producing a
  notarization ticket that a victim's macOS would accept.
- **Cert expiry mid-release-cycle.** Apple Developer ID Application
  certs expire every 5 years. Forgetting to rotate ahead of expiry
  silently breaks future releases (codesign starts failing, CI fails
  loudly — but the failure is at release time, not at PR time, so it
  surfaces late).

**Mitigations:**

*Storage and access:*
- Secrets live in **GitHub Actions repository secrets only**. Never
  printed to logs (the workflow uses `if: env.MACOS_CERTIFICATE != ''`
  for gating, never `echo "$MACOS_CERTIFICATE"`).
- The temp keychain and decoded `.p8` file are removed in the
  always-trailer cleanup step, so a runner reuse cannot leak them.
- The keychain password is per-job (inline `openssl rand -hex 32`),
  not a stored secret — there is no long-lived keychain password to
  rotate.

*Rotation cadence:*
- **Every 5 years** (cert expiry). Procedure: regenerate cert via
  developer.apple.com, export to fresh `.p12`, base64-encode, update
  `MACOS_CERTIFICATE` + `MACOS_CERTIFICATE_PASSWORD` GHA secrets,
  re-tag a candidate release to confirm the new cert signs and
  notarizes cleanly. Full procedure in
  [ADR 0011 § Cert rotation cadence and compromise response](docs/adr/0011-macos-signing-and-notarization.md#cert-rotation-cadence-and-compromise-response).

*Compromise response:*
1. **Revoke the cert immediately** in developer.apple.com →
   Certificates. Apple's revocation lists propagate to Gatekeeper
   within hours.
2. **Rotate `MACOS_CERTIFICATE` + `MACOS_CERTIFICATE_PASSWORD`** to a
   freshly issued cert.
3. **Cut a fresh release tag** with the new cert. Update GitHub
   Release notes for any affected previous tags pointing users at the
   new build.
4. **Audit GHA workflow run logs** around the leak window for
   evidence of secret exfiltration; rotate any other secrets with the
   same exposure (the App Store Connect API key's `.p8` lives in the
   same secret store).
5. Existing already-downloaded artifacts stamped with the revoked
   cert may continue to work for users who validated them before
   revocation propagated; new downloads of the same artifact will
   fail Gatekeeper.

**Out of scope:** an attacker who compromises a maintainer's Apple
Developer Program account directly (via Apple-ID phishing, Apple-ID
session hijack, etc.) can rotate certs and re-issue API keys at will;
that's an Apple-account-security boundary, not something the project
infrastructure can defend against. Mitigated by enabling Apple's
two-factor auth on the Developer Program account.

## What this document does NOT cover

- The user's host OS being compromised (rootkit, malware, hostile admin).
- Network-level attacks on the user's connection beyond what TLS already
  defeats.
- Side-channel attacks against the host hardware.
- Threats specific to upstream yt-dlp itself (covered by upstream's own
  posture).
- Future features (auto-update, browser-extension integration, etc.).
  Re-evaluate this document when those land.
