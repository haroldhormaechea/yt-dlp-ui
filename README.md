# yt-dlp-ui

A desktop UI that wraps the [yt-dlp](https://github.com/yt-dlp/yt-dlp) CLI so non-technical users can download audio and video from a URL without learning command-line flags.

This file documents the UI/installer layer in this repository. The upstream yt-dlp source tree (`yt_dlp/`, `bundle/`, `devscripts/`, `test/`, `pyproject.toml`, `Makefile`, the existing `README.md`, etc.) is treated as read-only third-party code and is not modified here. For upstream documentation see the existing `README.md` at the repository root.

## What it does

- Single-URL download with format and quality selection.
- A persistent download queue with configurable concurrency (default 3, range 1–10) and per-item or whole-queue cancellation.
- Live progress UI: percentage, speed, ETA, per-item logs.
- Cross-platform: Linux, macOS, Windows. Per-OS installers are part of the project scope (`.dmg`, NSIS `.exe`, `.deb`, `.rpm`, `.snap`).

The UI invokes a bundled copy of the upstream `yt-dlp` standalone binary via subprocess. It does not import, interpret, or modify the upstream Python source.

## Requirements

### End users (installing from a release artifact)

- **macOS / Windows / Linux** — that's it. The released `.dmg`, NSIS `.exe`,
  `.deb`, and `.rpm` artifacts bundle both `yt-dlp` and `deno` inside the
  installer; nothing else needs to be on PATH. UC 06 closed this gap as of
  the 2026.04 release pipeline.
- On macOS the installer is a single universal-binary `.dmg` (Apple Silicon
  + Intel). You don't need to know which chip you have.
- Binaries are unsigned at MVP — see *Known limitations* below for the
  Gatekeeper / SmartScreen bypass procedure.

### Developers (building from source)

- **Rust toolchain `1.95.0`** (edition 2024). The repository pins this in `rust-toolchain.toml`; install [rustup](https://rustup.rs) and the toolchain auto-installs on first `cargo` invocation.
- **`just`** for the project task runner (`brew install just` / `cargo install just` / `winget install --id Casey.Just`).
- **`bats-core`** (optional, for `just test-scripts`): `brew install bats-core` on macOS, `apt install bats` on Ubuntu 22.04+. Skipped if not present.
- **Linux native deps** (Debian/Ubuntu): `build-essential pkg-config libwebkit2gtk-4.1-dev libsoup-3.0-dev libssl-dev`. Equivalent `dnf` packages on Fedora.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Windows:** WebView2 Runtime (preinstalled on Windows 11 and recent Windows 10) and Visual Studio Build Tools 2022 for the MSVC toolchain.
- **Python 3** on PATH for development. The dev `yt-dlp` wrapper is a thin shell/cmd shim that invokes the upstream `yt_dlp/` Python module vendored in this repo. Release builds bundle the standalone `yt-dlp` binary instead and do not require Python at runtime.
- **Deno** *(recommended for YouTube)*: `brew install deno` (macOS), `apt install deno` (Linux, when packaged) or upstream binary from <https://github.com/denoland/deno/releases>, equivalent on Windows. yt-dlp 2026.x uses Deno to resolve YouTube signature challenges; without it, format selection is degraded and some YouTube videos may partially fail. **Release builds bundle Deno alongside `yt-dlp` (UC 06)** so end users don't hit this; in development the app probes `$PATH` and shows a one-time, dismissible warning banner if Deno is missing.
- On a fresh `rustup` install you may need to `source "$HOME/.cargo/env"` (or open a new shell) before `cargo` is on PATH — this is rustup default behavior, not a project bug, but it bites first-run users.

## Quick start

```sh
git clone git@github.com:HaroldHormaechea/yt-dlp-ui.git
cd yt-dlp-ui
just fetch-runtime-deps  # one-time: pulls bundled ffmpeg into runtime-deps/
just lint test           # clippy + tests across the workspace
just run                 # launch the main app (debug build)
just adwin               # run the ad-window helper standalone
just fake-ad-server      # serve a placeholder ad locally on port 7878
cargo run --release --bin app   # plain cargo path, no `just` needed
```

`just fetch-runtime-deps` is the documented escape hatch for the bundled
ffmpeg binary (UC 17). On Linux/Windows it pulls a SHA-pinned LGPL-only
static prebuilt from BtbN/FFmpeg-Builds; on macOS it builds from upstream
FFmpeg source (~10–15 minutes the first time). Pins live in
`scripts/runtime-deps-pins.env`. `cargo build` also auto-fetches on Unix
when `runtime-deps/ffmpeg` is missing — set `YT_DLP_UI_FETCH_DEPS=0` to
opt out (offline / air-gapped builds). If ffmpeg is missing at runtime,
downloads requiring DASH-merge surface a clear error in the row instead
of silently producing split files.

The main binary is `crates/app`. The ad-window helper is `crates/ad-window`. The wrapper around the bundled `yt-dlp` binary is `crates/yt-dlp-bridge`. Building installers is wired through `cargo-dist`; run `cargo dist build` once `cargo dist init` has produced the workflow file.

## Cookies and YouTube bot-check

YouTube intermittently flags downloads with "Sign in to confirm you're not a bot." When yt-dlp returns this error, the app surfaces a single modal dialog that asks you to pick one of your installed browsers — yt-dlp will read that browser's cookie database (the same session cookies your browser already uses) and retry the download with `--cookies-from-browser <browser>`.

What you should know:

- **Cookies stay on your machine.** They accompany the request to YouTube the same way your own browser would, and never leave to any project-controlled server. yt-dlp reads the cookie DB locally; the UI process does not relay credentials anywhere.
- **macOS Chrome triggers a Keychain prompt** the first time. That prompt is the OS asking you for permission to share Chrome's cookie store with yt-dlp; it is not the app pretending to be Chrome. Approve it once and subsequent downloads run without prompts.
- **Linux Chrome requires the browser to be closed.** Chromium-family browsers on Linux hold an exclusive lock on their cookie SQLite while running; yt-dlp will surface a "database is locked" error if Chrome is open. Close it and retry, or pick a different browser whose database is not locked.
- **Snap- or flatpak-confined browsers may not expose their cookie databases** at the standard paths. If a pick fails repeatedly, try a non-confined install (`apt install` of the upstream `.deb`, the upstream `.tar.bz2`, etc.) or pick a different browser.
- **Change the choice later** in **Settings → Cookies source**. Pick "None" to clear it, or pick a different browser. The choice is persisted across app restarts when you check "Remember this choice" in the dialog.

## Project layout

```
yt-dlp-ui/
├── PROJECT_BRIEF.md      machine-readable contract (frontmatter + prose)
├── README.md             this file
├── LICENSE               PolyForm Noncommercial 1.0.0 (UI/installer code)
├── Cargo.toml            Rust workspace manifest
├── rust-toolchain.toml   pinned toolchain
├── Justfile              dev task runner
├── deny.toml             cargo-deny license & dependency policy
├── THREATS.md            threat model
├── CONTRIBUTING.md       contributor guide
├── crates/
│   ├── app/              main UI process (Slint + tokio + rusqlite)
│   ├── ad-window/        ad-rendering helper (wry + tao)
│   └── yt-dlp-bridge/    subprocess wrapper, UI-free
├── docs/adr/             architecture decision records
├── examples/             sample URLs + fake-ad-server for local dev
├── installer/            packaging assets (.nsi, nfpm.yaml, .dmg builder, bundled-binary LICENSE texts)
├── scripts/              build-time fetchers + verifiers for bundled binaries (yt-dlp, deno, ffmpeg)
└── use-cases/            individual use case files indexed by USE_CASES.md
```

## macOS release prerequisites

Cutting a macOS release requires Apple Developer ID credentials wired
into GitHub Actions. UC 26 traced a hard launch failure on macOS 26.x
arm64 to the unsigned-binary posture (the kernel's
`AppleSystemPolicy` denies exec at dyld-startup, independent of any
Gatekeeper "open anyway" override the user grants); the macOS-only fix
is full Posture-1 signing + notarization. Linux + Windows release
posture is unchanged. The full design rationale lives in
[ADR 0011](docs/adr/0011-macos-signing-and-notarization.md).

**One-time setup (project owner):**

- Active membership in the **Apple Developer Program** (~$99/year).
  Lapsing the membership invalidates the Developer ID cert and breaks
  future macOS releases until renewed.
- A **Developer ID Application** certificate exported as a `.p12`
  (Keychain Access → right-click → Export → `.p12`).
- An **App Store Connect API key** (`.p8`) with the
  *Developer ID notary service* role.

**Six GHA repository secrets** (the inventory; mirrored in ADR 0011 and
in `.github/workflows/package-dmg.yml`):

| Secret | What it is |
|---|---|
| `APPLE_TEAM_ID` | 10-char Apple Developer team identifier |
| `APP_STORE_CONNECT_API_KEY_ID` | App Store Connect API key id |
| `APP_STORE_CONNECT_API_KEY_ISSUER_ID` | Issuer id paired with the key |
| `APP_STORE_CONNECT_API_KEY_P8` | Base64-encoded `.p8` key file |
| `MACOS_CERTIFICATE` | Base64-encoded `.p12` Developer ID Application cert |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` export password |

`MACOS_KEYCHAIN_PASSWORD` is **not** a stored secret — the temporary
keychain's password is generated inline via `openssl rand -hex 32` for
each release run and lives only as long as the GHA job. `APPLE_ID_USERNAME`
is also not used — `notarytool` API-key auth is preferred.

PR-from-fork builds and master builds run before the secrets are
provisioned see empty values and skip the entire signing block,
producing the same unsigned `.dmg` the pre-UC-26 pipeline produced.

**Cert rotation cadence.** Apple Developer ID Application certs expire
every 5 years from issuance. The full rotation procedure and
compromise-response checklist live in [ADR 0011 § Cert rotation
cadence and compromise response](docs/adr/0011-macos-signing-and-notarization.md#cert-rotation-cadence-and-compromise-response).

**Day-to-day macOS development does NOT involve signing.** `cargo run`
(or `just run`) is the primary dev flow — the dev workflow does not
touch codesign at all. The opt-in helper for the rare maintainer flow
of "deep-sign and verify a locally built `.app` before tagging" is
[`scripts/macos-signing-local.sh`](scripts/macos-signing-local.sh) (run
without args for usage; macOS-only, no-op elsewhere).

**Bundle structure verification.** A QA-owned macOS bundle-structure
verifier (`scripts/macos-verify-bundle.sh`) is on the testing roadmap
for UC 26; once it lands it runs `codesign --verify --deep --strict`
+ `spctl --assess` + `stapler validate` against a freshly built `.dmg`
as part of the release acceptance checklist.

## Known limitations

- **Pre-implementation scaffold.** No UI exists yet; the three crates are stubs that compile and log. The download queue, ad-window IPC, settings, and SQLite schema are designed (see `PROJECT_BRIEF.md` § Architecture and `docs/adr/`) but not built.
- **No release has been cut yet.** `cargo-dist` (`dist` CLI, v0.31.0) is initialized — `dist-workspace.toml`, `[profile.dist]`, and `.github/workflows/release.yml` are committed. Upstream's reusable release workflow was renamed to `.github/workflows/release-upstream.yml` to free the path; its callers (`release-master.yml`, `release-nightly.yml`) were updated. Cutting a release requires a git tag matching the dist contract; no tag exists yet. **UC 06 wired the four native installer formats** (.dmg, NSIS .exe, .deb, .rpm) via cargo-dist's `global-artifacts-jobs` splice + per-format packagers (nfpm, makensis, hdiutil); both `yt-dlp` and `deno` are bundled inside each artifact. `.snap` remains a separate workflow gap (`snapcore/action-build` + `snapcore/action-publish`); cargo-dist does not generate snap artifacts.
- **Binaries are unsigned on Linux + Windows.** Posture decision for the MVP on those OSes. Windows SmartScreen will warn users on first launch — "More info → Run anyway". Linux distros do not warn. Re-evaluate Windows signing (~$199–699/yr) once the project demonstrates real-world demand. **macOS upgraded to Posture 1 (UC 26):** Developer ID + hardened runtime + notarization + DMG-only stapling. See *macOS release prerequisites* above and ADR 0011 for the full picture. **macOS quarantine note (UC 17):** the `.dmg` build runs `xattr -cr` on the staged `.app` before packaging, and the app strips `com.apple.quarantine` again at startup, so Gatekeeper prompts only once — not separately for yt-dlp, ffmpeg, and deno.
- **Ad SDK and vendor are not selected.** The `ad-window` crate is a stub; integrating a real third-party ad-network SDK is deferred until a vendor is chosen. Telemetry implications and the first-launch consent disclosure depend on that choice.
- **No automated UI tests.** MVP relies on a smoke-binary CI test (the binary boots and DB migrations succeed) plus manual per-OS smoke checks. True UI automation is deferred to production maturity.
- **Coverage target is aspirational.** 60% at MVP, 80% at production; not yet enforced because there is no code to cover.
- **Source-available, not open source.** PolyForm Noncommercial 1.0.0 forbids commercial use. This is not OSI-approved; package registries that require OSI/FSF licenses (Homebrew core, Debian main, Fedora) will not accept it. Distribution is via GitHub Releases and the project's own snap.
- **Wayland modal dialog is not strictly modal.** On Wayland (Linux), the bot-check pop-up's modality is best-effort — a determined user can tab back to the main window. Slint's `PopupWindow.close-policy: no-auto-close` blocks accidental dismissal but not OS-level focus stealing. On X11, macOS, and Windows the pop-up behaves as a standard modal.
- **Cancel latency on large multi-segment downloads.** UC 02 wires a two-stage `SIGTERM` → 2 s grace → `SIGKILL` body on Unix so yt-dlp can finish flushing its current `.part` segment before being force-killed. On big multi-segment downloads (large fragmented videos, audio playlists with embedded thumbnails), yt-dlp may hold off honoring `SIGTERM` until its current chunk is on disk. The 2-second grace is usually enough; if not, the bridge falls back to `SIGKILL` so the cancel is never indefinite. On Windows there is no `SIGTERM` analog — `child.start_kill()` issues an immediate `TerminateProcess`, so cancel latency is bounded but partial files may be incomplete.

## License

UI and installer code in this repository: **PolyForm Noncommercial 1.0.0** (see repo-root `LICENSE`). The bundled `yt-dlp` binary remains under the **Unlicense** (see `installer/yt-dlp-LICENSE.txt`, shipped at the per-OS bundled-binary install path); the bundled `ffmpeg` binary remains under the **LGPL-2.1+** (see `installer/ffmpeg-LICENSE.txt`, similarly shipped).
