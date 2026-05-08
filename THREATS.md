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

**Surface:** the installer artifacts on macOS and Windows are unsigned at
MVP per the deliberate signing posture (PROJECT_BRIEF.md § Deployment).

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

## What this document does NOT cover

- The user's host OS being compromised (rootkit, malware, hostile admin).
- Network-level attacks on the user's connection beyond what TLS already
  defeats.
- Side-channel attacks against the host hardware.
- Threats specific to upstream yt-dlp itself (covered by upstream's own
  posture).
- Future features (auto-update, browser-extension integration, etc.).
  Re-evaluate this document when those land.
