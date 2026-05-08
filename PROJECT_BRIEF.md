---
schema_version: 1
project:
  name: "yt-dlp-ui"
  target_dir: "/Users/hhormaechea/Projects/yt-dlp-ui"
  maturity_target: mvp
stack:
  languages: ["rust"]
  frameworks: ["slint", "tokio"]
  runtimes: ["desktop"]
  versions:
    rust: "1.95.0"
    rust_edition: "2024"
    slint: "1.16.1"
    wry: "0.55.0"
    tao: "0.35.0"
    rusqlite: "0.39.0"
    reqwest: "0.13.2"
    tokio: "1.52.1"
    tracing: "0.1.44"
    tracing-subscriber: "0.3.23"
  data_stores: ["sqlite"]
build:
  tool: "cargo"
  commands:
    test: "cargo test --workspace"
    lint: "cargo clippy --workspace --all-targets -- -D warnings"
    format: "cargo fmt --all"
paths:
  production: ["crates/*/src/**"]
  test: ["crates/*/tests/**", "crates/*/src/**/*_test.rs"]
  api_boundary: null
test:
  framework: "cargo test"
  levels: ["unit", "integration", "smoke"]
  coverage_target: "60% at MVP, ratcheting to 80% at production"
profiles: []
deployment:
  provider: "GitHub Actions + GitHub Releases"
  iac: "none"
  environments: ["development", "ci", "release"]
vcs:
  enabled: true
  already_initialized: true
  default_branch: "master"
  remote: "git@github.com:HaroldHormaechea/yt-dlp-ui.git"
use_cases:
  index: USE_CASES.md
  folder: use-cases/
---

# Project Brief

## Overview

- **Name:** yt-dlp-ui
- **Problem:** Users currently have to know how to use a CLI to download videos with yt-dlp; we want to simplify that.
- **Primary users:**
  - Non-technical media saver — wants to grab a video/audio from a URL without learning a CLI.
  - Podcast / audio ripper — wants audio-only downloads from podcast or video sources.
- **Value proposition:** Improve access to yt-dlp's capabilities for non-technical users, and make it easy to keep local saves of interesting content.
- **Maturity target:** MVP, with explicit intent to evolve toward production over time (auto-update, crash reporting, broader format coverage, signed artifacts hardening, etc. are deferred but anticipated).
- **In-scope capabilities (MVP):**
  - Single-URL download with format/quality selection (video / audio / auto).
  - Download queue with potential parallelization of downloads (concurrency cap / setting to be decided in the architecture step).
  - Progress UI: live progress, speed, ETA, per-item logs.
  - Cross-platform installers for Linux, macOS, and Windows (locked scope item).
- **Non-goals:**
  - No modifications to the upstream yt-dlp Python source tree (`yt_dlp/`, `bundle/`, `devscripts/`, `test/`, `pyproject.toml`, `uv.lock`, `Makefile`, etc.) — treated as read-only third-party code at all times (locked scope item).
  - No mobile (iOS / Android) clients.
  - No SaaS, hosted, or web version — strictly local, desktop-only.
  - No built-in media player.
  - No DRM circumvention beyond what yt-dlp itself already supports.
  - No content discovery, search, or recommendations.
- **Success criteria:**
  - App startup under 1 second on a mid-range laptop (target, not a hard gate).
  - Unlimited download queue, with every queued or in-flight item cancellable individually and the whole queue cancellable as a batch.
  - Signed / installable artifacts produced on every release for macOS (.dmg or .pkg), Windows (.msi or .exe), and Linux (.deb / .rpm / .AppImage) (locked scope item).
  - The UI never modifies any file under the upstream yt-dlp tree at runtime or build time (locked scope item).

## Monetization

- **Commercial intent:** Yes.
- **Model:** Donations + in-app advertising (third-party ad-network SDK). Both revenue streams run in parallel. No paid tiers, no one-time purchase, no SaaS, no subscriptions.
- **License:** **PolyForm Noncommercial 1.0.0** for all UI and installer code authored in this repository. Source-available, non-commercial — forks, redistribution, modification, and personal/internal/educational use are permitted; commercial use is not. This is **not** an OSI-approved open-source license; the project must be described as "source-available, non-commercial," never as "open source."
- **Dual-licensing reality:** The existing `LICENSE` file at the repository root is upstream yt-dlp's Unlicense and applies to the upstream tree (`yt_dlp/`, `bundle/`, `devscripts/`, `test/`, `pyproject.toml`, etc.) only. A separate `LICENSE.UI.md` (or similarly distinct filename) will be added during scaffolding containing the PolyForm Noncommercial 1.0.0 text and applies to the UI / installer code. The upstream `LICENSE` file is not overwritten.
- **Ads + non-commercial-license interaction:** Running ads in the licensor's own distribution of the app is permitted under PolyForm Noncommercial — the license restriction binds licensees, not the licensor. Forks may strip the ads (permitted). Forks may **not** add their own ads, paid features, or other commercialization and redistribute (forbidden by the non-commercial clause). This must be made clear to contributors and forkers.
- **Target market:** Worldwide; unskilled / non-technical users who want to download random songs and podcasts. No geographic targeting.
- **Tiers / pricing:** Not applicable — donations are user-discretion, ads are revenue-side and not user-paid.
- **Constraints:**
  - **Telemetry:** Permitted only as required by the bundled ad-network SDK. The project itself collects no first-party behavioral telemetry; whatever the third-party SDK does, the user inherits. No additional telemetry beyond the SDK's own.
  - **First-launch disclosure + persistent settings entry:** Required regardless of revenue. The UI must surface, on first launch and as a permanent entry in a settings/about page, a clear plain-language notice that (a) ads are shown, (b) the ad SDK collects device and behavioral data, (c) links to the SDK vendor's privacy policy, and (d) provides a consent flow appropriate to GDPR / CCPA jurisdictions when the user is in one. Target audience is non-technical; the wording must be readable, not legalese.
  - **No data resale by the project itself.** Trivially true — the project does not collect or sell data. The ad SDK vendor's data handling is the SDK vendor's responsibility; the disclosure obligation above ensures users are made aware.
  - **Respect upstream's spirit.** Even with ads + non-commercial license, position the project as community-aligned, not predatory. Do not misrepresent the relationship to upstream yt-dlp.
  - **No DRM circumvention beyond what yt-dlp itself supports** (already in non-goals; restated here to avoid inheriting liability via paid distribution).

## Technologies

- **Constraints:** Cross-platform desktop (Linux, macOS, Windows). Per-OS installers required. Wraps the bundled `yt-dlp` CLI via subprocess; never modifies the upstream Python tree. Ad-network SDK integration is required and is the dominant forcing function on framework choice. **VERY LEAN** RAM footprint is a primary constraint — idle RAM (no ad visible) targeted at 30–60 MB.
- **Runtime targets:** Desktop on Linux, macOS, Windows. No browser, mobile, or server runtimes.
- **Language:** Rust (edition 2024), toolchain `1.95.0` (stable channel).
- **UI framework:** **Slint** (via `.slint` markup files compiled at build time through `slint-build` — preferred over the inline `slint!` macro for tooling and clearer separation of UI from logic).
- **Ad slot:** Separate child process built on **`wry`** (system WebView) + **`tao`** (window/event-loop). Spawned only when an ad should be visible; killed (process exit) when the main window is minimized, the user is in a focus mode, or settings disable ads. This out-of-process design is the single biggest contributor to the lean idle-RAM target.
- **yt-dlp invocation:** Bundle the upstream `yt-dlp` single-file binary (produced by upstream's own PyInstaller-based bundler) inside each installer. Invoke via `std::process::Command` from a dedicated bridge crate. The upstream `yt_dlp/` Python source tree is **not** referenced at build time and is **not** invoked at runtime — only the bundled standalone binary is.
- **Database:** **SQLite** via `rusqlite 0.39.0` (synchronous; cleanest fit for a single-process desktop app). Used for the unlimited cancellable download queue, settings, and history.
- **HTTP:** `reqwest 0.13.2` configured with `default-features = false` and features `["rustls-tls", "json"]` to avoid an OpenSSL dependency (cross-platform simplicity, no system-OpenSSL gotchas on macOS or Windows).
- **Async runtime:** `tokio 1.52.1` (multi-threaded scheduler) — used by the app crate for concurrent download supervision and IPC.
- **Logging:** `tracing 0.1.44` + `tracing-subscriber 0.3.23` (features `["env-filter", "fmt"]`).
- **Build tool:** `cargo`.
- **Workspace layout:** Cargo workspace with three crates:
  - `crates/app` — main UI process. Owns the Slint window, the queue, settings, the ad-window lifecycle, and the tokio runtime. Depends on `yt-dlp-bridge`.
  - `crates/ad-window` — ad-rendering helper. A separate binary built on `wry` + `tao`. No UI dependencies on Slint, no dependency on `yt-dlp-bridge`. Communicates with `app` via a simple stdin/stdout JSON protocol (or local domain socket — finalized in `define-architecture`).
  - `crates/yt-dlp-bridge` — typed wrapper around the bundled `yt-dlp` binary: subprocess spawn, JSON / progress parsing, cancellation, error mapping. **Free of UI dependencies** so it can be tested in isolation and reused by future tooling. Depends only on `tokio`, `tracing`, `serde` / `serde_json`, and the standard library.
- **Auth / authz:** None (local-only desktop, no accounts).
- **External services:** Third-party ad-network endpoints (vendor TBD). No other external services.
- **AI / ML dependency:** None.

## Architecture

- **Platforms:** Desktop on Linux, macOS, Windows. No web, mobile, CLI front-end, or server.
- **Service shape:** Multi-process desktop app — one main process, one helper process spawned on demand, N transient `yt-dlp` child processes per active download.
- **Multi-tenancy:** Not applicable (single user, single installation).

### Process topology

```
+----------------------------------------------------------+
|  Operating System                                        |
|                                                          |
|  +------------------+         +-------------------+      |
|  |   app (Slint)    | <-IPC-> |  ad-window (wry)  |      |
|  |  - UI thread     |  JSON   |  - WebView only   |      |
|  |  - tokio rt      |  over   |  - no DB          |      |
|  |  - SQLite        |  pipes  |  - no fs write    |      |
|  |  - DownloadMgr   |         |  - no subprocess  |      |
|  +--------+---------+         +-------------------+      |
|           | spawn(yt-dlp)                                |
|           v                                              |
|  +------------------+   +------------------+   +-------+ |
|  |  yt-dlp child #1 |   |  yt-dlp child #2 |   |  ...  | |
|  |  (stdout/stderr  |   |  (stdout/stderr  |   |       | |
|  |   piped to app)  |   |   piped to app)  |   |       | |
|  +------------------+   +------------------+   +-------+ |
|                                                          |
+----------------------------------------------------------+
```

Process count under typical load: `1 (app) + 1 (ad-window, if visible) + min(queued, concurrency_cap) yt-dlp children`. Idle (no downloads, ad hidden): a single process — `app`.

### Ad-window lifecycle (MVP design — subject to revision once real-world ad-vendor behavior is observed)

- **Spawn triggers (any of):**
  - Main window becomes visible (deminimized, focused, or shown for the first time).
  - Ad-consent flag is true in settings.
  - A timer-based rotation tick when the user has been on a screen long enough to warrant a refresh.
- **Kill triggers (any of):**
  - Main window minimized or hidden for more than 5 seconds (debounce avoids thrashing on alt-tab).
  - "Focus mode" toggled on.
  - User disables ads in settings.
  - App quit.
- **IPC channel:** stdin/stdout newline-delimited JSON line protocol. `tokio::process::Command` with `Stdio::piped()`; messages parsed line-by-line. Chosen over local sockets and TCP for portability (no per-OS code paths) and security (kernel-enforced parent/child pipe with no third-party access).
  - `app → ad-window`: `{"command": "show", "creative_url": "..."}`, `{"command": "hide"}`, `{"command": "shutdown"}`.
  - `ad-window → app`: `{"event": "ready"}`, `{"event": "click", "url": "..."}`, `{"event": "error", "msg": "..."}`. Heartbeat optional.
- **Shutdown protocol on `app` quit:**
  1. `app` sends `{"command": "shutdown"}` on stdin.
  2. Wait up to 2 seconds for clean exit.
  3. SIGTERM (Unix) / `TerminateProcess` (Windows).
  4. After 1 more second, SIGKILL / forced terminate.
- **Crash-recovery policy if `ad-window` dies unexpectedly:**
  - Log via `tracing` at WARN.
  - Exponential-backoff respawn within the session: 5 s → 30 s → 5 min → give up for the rest of the session.
  - The main app continues to function fully without ads — ad failures must never block downloads.
  - UI surfaces a small "ads currently unavailable" indicator; cleared on next successful spawn.
- **Re-evaluation:** The above is an MVP design. Once we have real ad-vendor behavior data (creative load times, vendor-side reliability, the SDK's own retry semantics, click-through expectations), the lifecycle policy will be revisited.

### Download concurrency model

- **Concurrency cap:** user-configurable in settings, range **1–10**, default **3**.
- **Per-download task:**
  - One `tokio::task` per active download.
  - Spawns the bundled `yt-dlp` binary via `tokio::process::Command` with `--newline` and `--progress-template` flags so progress is machine-parseable.
  - Stdout/stderr line-piped into a parser that emits structured events: `Started`, `Progress { pct, speed, eta }`, `PostProcessing`, `Finished`, `Error`.
  - Events flow on a `tokio::sync::mpsc` channel into the UI; Slint's event loop consumes them via a per-window callback.
- **SQLite as durable queue:**
  - Tables (sketch — exact DDL belongs in implementation):
    - `queue_items(id, url, format_pref, dest_dir, status, progress_pct, speed_bps, eta_s, error_msg, created_at, started_at, finished_at)`
    - `settings(key, value)` — KV table for app preferences (concurrency cap, default download dir, ad-consent state, focus mode flag, etc.).
    - `history(id, queue_item_id, file_path, bytes, completed_at)` — append-only.
  - Status enum: `queued`, `in_flight`, `paused`, `cancelled`, `done`, `error`.
  - Migrations via a hand-rolled `schema_version` table; one migration function per version, called at startup. (`refinery` is overkill at this size.)
- **Cancellation semantics:**
  - **Per-item:** UI sends cancel signal to the task; **on Unix** the bridge sends `SIGTERM` to the `yt-dlp` child and waits up to 2 s for clean exit, then `SIGKILL` if still alive. **On Windows** the bridge calls `child.start_kill()` (an immediate `TerminateProcess`) — there is no kernel analog of `SIGTERM`, so the Windows path has no grace period. Item status → `cancelled`. Partial `.part` files remain on disk for possible later resume. Concurrency slot is freed; next `queued` item promoted.
  - **Whole queue:** Iterate active tasks, send cancel to each. New items cannot be promoted while cancellation is in flight. All in-flight items → `cancelled`. UI shows a "queue cancelled" toast.
- **Resume-after-restart (R2 — yt-dlp-driven resume):** On startup, items with status `in_flight` are reverted to `queued`. When a task picks one up, it invokes `yt-dlp` normally; `yt-dlp`'s built-in `--continue` behavior decides whether to resume from the existing `.part` file or restart. Edge cases (corrupt `.part`, format selection changed between sessions) are delegated to `yt-dlp`'s own handling.

### Trust boundaries

- **Source-level read-only zone:** the upstream Python tree and its tooling — `yt_dlp/`, `bundle/`, `devscripts/`, `test/`, `pyproject.toml`, `uv.lock`, `Makefile`, `Changelog.md`, `CONTRIBUTORS`, `Maintainers.md`, `supportedsites.md`, `THIRD_PARTY_LICENSES.txt`, the existing `LICENSE`, the existing `README.md`, `yt-dlp.cmd`, `yt-dlp.sh`, `public.key`, `.pre-commit-config.yaml`, `.pre-commit-hatch.yaml`, the existing `.editorconfig`, the existing `.gitattributes`, and the existing `.github/`. The dev-team's `developer` and `qa` agents must not write into any of these. The `paths.production = ["crates/*/src/**"]` glob enforces this scope.
- **Runtime invocation surface:** only the bundled `yt-dlp` standalone binary is executed. The upstream Python tree is never imported or interpreted at runtime.
- **`ad-window` privilege envelope:**
  - May: read its own stdin (the IPC line protocol); write its own stdout/stderr (events + `tracing` logs); make outbound HTTPS calls (the WebView fetches ad creative); read/write its own per-process WebView cache directory (a temp dir created by the helper and cleaned up on exit).
  - May NOT: open the SQLite database file; spawn further subprocesses; read files outside its cache dir; use shared memory or any IPC channel with `app` beyond the stdin/stdout pipe.
  - Enforcement is by code review and process design, not OS sandboxing. Per-OS sandboxing (entitlements / AppArmor profiles / job objects) is deferred to production maturity.
- **Ad webview boundary:** fully sandboxed system WebView. There is no shared JS bridge with `app`. Ad creative is treated as untrusted input; clicks are intercepted at the navigation event and opened in the user's default browser via `open` / `xdg-open` / `start`, never followed inside the WebView.
- **User-supplied URLs:** treated as untrusted input. Passed through to the `yt-dlp` binary as positional args. Never shelled out via `sh -c`; always `Command::new(yt_dlp_path).arg(url)` so there is no shell-injection surface.

### Storage layout

App-data location is resolved via the **`directories`** crate (per-OS XDG / Apple / Windows conventions):

- **Linux:** `~/.local/share/yt-dlp-ui/` (XDG `data_dir`).
- **macOS:** `~/Library/Application Support/yt-dlp-ui/`.
- **Windows:** `%LOCALAPPDATA%\yt-dlp-ui\`.

Each app-data directory holds:

- `db.sqlite` — main database (queue, settings, history).
- `logs/yt-dlp-ui.log` — rotated via `tracing-appender`.

Default download destination (where downloaded media lands; user-configurable in settings):

- **Linux:** `~/Downloads/yt-dlp-ui/` (or `$XDG_DOWNLOAD_DIR` if set).
- **macOS:** `~/Downloads/yt-dlp-ui/`.
- **Windows:** `%USERPROFILE%\Downloads\yt-dlp-ui\`.

### Bundled-binary path

The `yt-dlp` standalone binary ships next to the `app` binary in each installer; the location is per-OS:

- **Linux (.deb / .rpm):** `/usr/lib/yt-dlp-ui/yt-dlp` (or `/opt/yt-dlp-ui/yt-dlp` for AppImage). Main binary at `/usr/bin/yt-dlp-ui` resolves it via `std::env::current_exe()` → install prefix.
- **macOS (.app bundle):** `yt-dlp-ui.app/Contents/Resources/yt-dlp`. Main binary at `yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui` resolves it via `current_exe().parent().parent().join("Resources/yt-dlp")`.
- **Windows (.msi / .exe):** `<InstallDir>\yt-dlp.exe` next to `yt-dlp-ui.exe`. Resolved via `current_exe().parent()`.
- **(UC 17, 2026-05-07) `ffmpeg`** is bundled at the same per-OS bundled-binary path as `yt-dlp` (canonical no-extension name). Linux: `/opt/yt-dlp-ui/ffmpeg`. macOS: `yt-dlp-ui.app/Contents/Resources/ffmpeg` (lipo-merged universal). Windows: `<InstallDir>\ffmpeg`. Resolved via `bundled_ffmpeg_path() -> Result<PathBuf, PathError>` mirroring `bundled_yt_dlp_path()`. Source posture is per-OS (LGPL-only): BtbN/FFmpeg-Builds release tags for Linux + Windows; built from upstream FFmpeg source on the GHA macOS runners. ADR 0010 records the rationale; `THREATS.md` § T13 covers the supply chain.

`app` exposes a single `bundled_yt_dlp_path() -> PathBuf` function that encapsulates the per-OS logic (cfg-gated). `yt-dlp-bridge` accepts the path as a constructor argument so it can be tested with a fake binary. The `ad-window` binary lives in the same directory as the `app` binary; no special handling.

### Workspace crate dependency graph

| Crate | Internal deps | Third-party deps |
|---|---|---|
| `app` | `yt-dlp-bridge` | `slint`, `tokio` (full), `rusqlite` (with `bundled` feature for vendored SQLite), `directories`, `tracing`, `tracing-subscriber`, `tracing-appender`, `serde`, `serde_json`, `thiserror` (or `anyhow`), **`rfd` (added by UC 01 for the Settings panel destination chooser)** |
| `ad-window` | (none) | `wry`, `tao`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `thiserror` (or `anyhow`) |
| `yt-dlp-bridge` | (none) | `tokio` (process + io + sync features only, NOT `full`), `serde`, `serde_json`, `tracing`, `thiserror`, **`nix` (Unix-only, added by UC 02 for the two-stage `SIGTERM` → grace → `SIGKILL` cancel body)** |

- `yt-dlp-bridge` is deliberately UI-free and async-runtime-minimal, so it can be unit-tested in isolation and reused for future tooling.
- `rusqlite` uses the `bundled` feature so a known SQLite version ships regardless of OS — avoids signing/notarization issues with system-libsqlite linking on macOS.
- `app` does not depend on `wry` / `tao` directly — that dependency surface lives in `ad-window` only.

### Async workloads / scheduled jobs

- Per-download tokio tasks (covered above).
- Periodic "check for upstream yt-dlp release" job — out of scope for MVP per `define-overview`.
- No cron, no scheduled jobs, no system services.

### Communication summary

| From | To | Protocol |
|---|---|---|
| `app` | `ad-window` | stdin/stdout newline-delimited JSON |
| `app` | `yt-dlp` child | command-line args + parsed stdout/stderr |
| `ad-window` | Internet (ad endpoints) | HTTPS via system WebView |
| `app` | Internet (donation links, future updates) | HTTPS via `reqwest` (rustls) |

## Quality & Standards

### Code style

Rust standard idioms enforced by `rustfmt` + `clippy`. No external company style guide applies.

### Linting & formatting

- **Format:** `cargo fmt --all`. **No `rustfmt.toml`** — defaults only. Cleanest contract; revisit if a real preference emerges.
- **Lint:** `cargo clippy --workspace --all-targets -- -D warnings`. Treats every clippy lint as an error.
- **Workspace `[lints]` table** in the root `Cargo.toml`:
  ```toml
  [workspace.lints.rust]
  unsafe_code = "forbid"

  [workspace.lints.clippy]
  all = "warn"
  pedantic = "warn"
  module_name_repetitions = "allow"
  must_use_candidate = "allow"
  ```
  `unsafe_code = "forbid"` is a deliberate posture: the dependencies (`slint`, `wry`, `tao`, `rusqlite`, `reqwest`, `tokio`) encapsulate `unsafe` internally; app/UI code should never need it. Forbidding at the workspace root catches accidents at compile time.

### Testing

- **Unit tests:** inline `#[cfg(test)] mod tests { ... }` blocks alongside production code, plus `*_test.rs` files in `crates/*/src/**` when a test surface gets large.
- **Integration tests:** `crates/*/tests/` — Cargo's standard integration-test location. Each file is a separate binary linking the crate as an external library, forcing public-API testing.
- **Smoke binary test:** a CI test that spawns the `app` binary, sends a JSON IPC command, and asserts a clean exit. Cheap insurance against "the binary doesn't even start on this OS."
- **No true UI automation at MVP.** Slint headless component tests, screenshot diffing, and `enigo`-based synthetic-event tests are all deferred to production maturity. Manual smoke per OS at release time fills the gap.
- **Frameworks / libraries:**
  - Built-in `cargo test` is the runner.
  - `pretty_assertions` — better diffs on assertion failure.
  - `insta` — snapshot tests for JSON / parser output.
  - **`proptest`** — property-based testing for the `yt-dlp-bridge` parser. The parser eats arbitrary stdout/stderr from yt-dlp; any crash there is a download UX bug, so generative testing is high-value here.
  - `tokio::test` macro — async test attributes (already comes with `tokio`).
- **Coverage target:** **60% at MVP, ratcheting to 80% at production**, measured via **`cargo-llvm-cov`**. UI code drags the average down; the bridge crate and the download manager are expected to carry most of the coverage weight.

### Security

- **`cargo-audit`** in CI: known-CVE detection on transitive dependencies. Runs on every PR and a weekly scheduled workflow.
- **`cargo-deny`** for license + dependency policy:
  - **Denied SPDX identifiers** (transitive presence would force open-sourcing or violate the source-available posture): `GPL-2.0-only`, `GPL-2.0-or-later`, `GPL-3.0-only`, `GPL-3.0-or-later`, `LGPL-2.0-only`, `LGPL-2.0-or-later`, `LGPL-2.1-only`, `LGPL-2.1-or-later`, `LGPL-3.0-only`, `LGPL-3.0-or-later`, `AGPL-1.0-only`, `AGPL-1.0-or-later`, `AGPL-3.0-only`, `AGPL-3.0-or-later`, `SSPL-1.0`. (LGPL is denied because everything in a Rust app is statically linked, which makes LGPL effectively GPL.)
  - **Allowed:** `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `MPL-2.0`, `Unicode-3.0`, `Unicode-DFS-2016`, `Zlib`, `CC0-1.0`, `Unlicense`, `0BSD`, `BSL-1.0`.
  - `cargo-deny` also flags duplicate dependency versions and bad sources; default policies for those are sane.
- **Dependabot** configured for `cargo` and `github-actions` ecosystems. Free, GitHub-native.
- **No additional SAST** beyond clippy at MVP. Tools like Semgrep have a poor signal-to-noise ratio for Rust relative to clippy's own security lints.
- **No secrets management at MVP** — there are no auth tokens, server-side API keys, or other secrets. The eventual ad-network SDK key (when a vendor is picked) will be embedded in the binary; the app is local-only and there is no server to hold it.
- **`THREATS.md`** is a hard MVP deliverable (not deferred). It must cover at minimum:
  1. **Untrusted user-supplied URLs** — passed to `yt-dlp` as positional args via `Command::new`; never shelled out via `sh -c`.
  2. **Untrusted ad creative** — sandboxed inside the system WebView in the `ad-window` process; click-through events are intercepted and opened in the user's default browser.
  3. **Untrusted third-party ad-SDK code** — collects telemetry by design; user-disclosed via the first-launch consent flow; runs only inside the `ad-window` process, never inside `app`.
  4. **Supply-chain risk via the bundled `yt-dlp` binary** — pinned upstream version; signature verification at build time if upstream provides one (yt-dlp publishes detached signatures).
  5. **Local DB tampering** — out of scope. A single-user desktop app cannot defend against the user's own filesystem write access; documenting this is the appropriate response.
  6. **Crate supply-chain risk** — mitigated by `cargo-audit` (CVEs) and `cargo-deny` (license / source policy).
  7. **Bundled-crate license drift** — mitigated by `cargo-deny`'s license policy enforced in CI.
  8. **`app` ↔ `ad-window` IPC channel** — stdin/stdout pipes; kernel-enforced parent/child boundary; no third process can attach. No auth needed.

### Accessibility

**WCAG 2.1 AA** as a project goal — color contrast 4.5:1, keyboard navigation works, every interactive element has an accessible name. Manual checklist at release time, not a CI gate (desktop a11y testing tools are weak). Slint exposes OS-native a11y APIs (UI Automation on Windows, AT-SPI on Linux, NSAccessibility on macOS), which gives screen-reader compatibility largely for free if widgets are labeled correctly.

### Performance budgets

Locked targets (some hard, some aspirational — recorded with their character):

- **Idle RAM (no ad visible):** target **30–60 MB**.
- **App startup:** under **1 second** on a mid-range laptop (target, not a hard gate).
- **Bundle size per OS installer:** under **100 MB compressed** (yt-dlp ~15 MB + ffmpeg LGPL-static ~25–35 MB + Rust app + Slint runtime ~10 MB + deno + assets). **(UC 17, 2026-05-07)** raised from 50 MB to absorb the bundled LGPL-only ffmpeg binary. **Revert clause:** if a later compression sweep brings the measured per-OS installer back under 50 MB compressed across all four formats (.dmg, NSIS .exe, .deb, .rpm), revert this ceiling and re-record the original 50 MB target.
- **Cold DB-open latency:** under **50 ms** for opening `db.sqlite` on app start.
- **UI event-to-paint latency:** under **16 ms** (60 fps).
- **Memory growth under load:** during a 50-URL queue at concurrency 3, total RSS (app + ad-window + yt-dlp children combined) does not exceed **250 MB**.

**Stretch directive: aim for the smallest bundle possible.** Concrete sub-budgets / build settings:

- `[profile.release]` flags in the workspace `Cargo.toml`:
  - `strip = true` — strip debug symbols.
  - `lto = "fat"` — full link-time optimization.
  - `codegen-units = 1` — slower compiles, smaller and faster output.
  - `panic = "abort"` — saves ~5 % binary size; loses backtraces, acceptable for a desktop app.
- `--no-default-features` on heavy crates where features are opt-in (already specified for `reqwest`).
- `cargo-bloat` run periodically (locally; not gated in CI) to identify the largest dependencies.
- Installer compression notes (the per-OS deployment skill pins exact settings):
  - `.deb` / `.rpm`: payloads compressed with **xz**.
  - macOS `.dmg`: HFS+/APFS native compression.
  - Windows `.msi`: cabinet (CAB) compression.

### Documentation

- **`README.UI.md`** — new file at repo root. Lightweight quick-start: what the project is, how to run from source, link to upstream yt-dlp. **Do not overwrite the existing `README.md`** (upstream's 178 KB).
- **`docs/adr/`** — MADR-style lightweight ADRs, one decision per file. Seeded with:
  - `0001-language-and-ui-framework.md` — Rust + Slint + spawned `wry` ad window (rationale: leanness).
  - `0002-license.md` — PolyForm Noncommercial 1.0.0 (rationale: non-commercial intent + upstream Unlicense compatibility).
  - `0003-monetization-model.md` — donations + third-party ad SDK (rationale: revenue + telemetry tradeoff explicit).
  - `0004-ad-window-process-isolation.md` — separate process for ad WebView (rationale: lean idle, killable on minimize).
  - `0005-yt-dlp-bundling.md` — bundle upstream single-file binary; never modify upstream tree.
  - `0006-storage.md` — SQLite via `rusqlite` `bundled` feature.
- **`CONTRIBUTING.UI.md`** — new file. Covers the read-only upstream tree rule, the PolyForm Noncommercial license, code structure (the three workspace crates and their responsibilities), and where new code goes (`crates/*`). **Do not overwrite the existing upstream `CONTRIBUTING.md`.**
- **No formal docs site** at MVP.

### Observability

- **Logging library:** `tracing` + `tracing-subscriber` + `tracing-appender`.
- **Rotation:** daily.
- **Retention:** 7 days. Older daily rolls are deleted.
- **Format split:**
  - **Dev (debug builds):** `tracing_subscriber::fmt` pretty format to stdout, level controlled via `RUST_LOG`.
  - **Production (release builds):** JSON to the rotated log file at `<app-data>/logs/yt-dlp-ui.log.YYYY-MM-DD`, plus a less-verbose pretty stream to stdout for users running from terminal.
- **Levels:** DEBUG and below stay in dev only; INFO/WARN/ERROR ship to the file in production.
- **No metrics, no distributed tracing, no remote log shipping at MVP.** Local file logs only.

## Profiles

None apply to this stack. The profiles available in the session workspace today (`profile-java-server-architecture`, `profile-java-database-access`, `profile-aws-deployment`) target Java + Spring Boot servers and AWS deployments — neither matches Rust + Slint + local-only desktop. `profiles` is set to `[]` in the frontmatter.

If a Rust- or desktop-oriented profile is added to the session workspace later (e.g., `profile-rust-desktop`, `profile-cargo-workspace`), it can be opted into via the `/revise-brief` flow without rerunning the full define cycle.

## Deployment

A desktop app does not have "hosting" in the SaaS sense. This section covers the **release pipeline** (build, sign, package, publish) and the **local development environment**.

### Production — release pipeline

- **Hosting target:** N/A (desktop app). The user installs the binary; nothing runs server-side.
- **Installer formats:**
  - **macOS:** `.dmg` (drag-to-Applications).
  - **Windows:** `.exe` via **NSIS**. Cross-platform CI runner support, signing via `signtool`, common in the Rust desktop ecosystem.
  - **Linux:** `.deb` + `.rpm` + `.snap`.
    - `.deb` covers Debian, Ubuntu, and derivatives.
    - `.rpm` covers Fedora, RHEL, openSUSE, and derivatives.
    - **`.snap`** publishes through the Snap Store (Canonical-hosted). MVP path:
      - Register the `yt-dlp-ui` snap name early via Ubuntu One (first-come-first-served).
      - `snapcraft.yaml` confinement strategy: **`classic`** is the realistic target because the app spawns subprocesses (`yt-dlp`), writes to `~/Downloads`, and the `wry` ad-window helper needs WebKit runtime access — `strict` confinement will likely break these. `classic` requires manual Canonical review (turnaround days to weeks). `devmode` is the fallback for unreviewed releases.
      - **`cargo-dist` does not generate snap artifacts.** A separate GitHub Actions step using `snapcore/action-build` and `snapcore/action-publish` is required. **Recorded scaffolding gap.**
    - **AppImage is not chosen.** If snap-publishing overhead becomes too high in practice, `.AppImage` is the easy fallback (zero submission process, single self-contained file). Not added now; noted as the standard escape hatch.
  - **(UC 06, 2026-04-27)** `.dmg`, NSIS `.exe`, `.deb`, `.rpm` are wired via cargo-dist 0.31.0's `global-artifacts-jobs` splice point. Per-format toolchain: **`nfpm`** (`.deb` + `.rpm`), **`makensis`** (NSIS `.exe`), **`hdiutil`** + **`lipo`** (`.dmg`, post-`.app`-synthesis). macOS ships a single **universal-binary** `.dmg` (lipo-merged x86_64 + aarch64 — UC 01's non-technical-user audience does not differentiate Apple Silicon vs Intel). Per-format install layout: see `installer/` config files. Both bundled binaries (`yt-dlp` and `deno`) are placed at the per-OS path `crates/app/src/paths.rs` expects. `.snap` remains a separate scaffolding-gap-tracked workstream (UC 07 candidate).
  - **(UC 17, 2026-05-07)** Bundled-binary set extends from `{yt-dlp, deno}` to `{yt-dlp, deno, ffmpeg, ffmpeg-LICENSE.txt}`. Per-OS source posture: BtbN/FFmpeg-Builds LGPL-only static prebuilt for Linux + Windows (SHA256-pinned in `scripts/runtime-deps-pins.env`); built from upstream FFmpeg source on the GHA macOS runner with locked-in LGPL-only configure flags. macOS ffmpeg is lipo-merged across both arches. Bundle-size budget raised from 50 MB → 100 MB to absorb the new binary (revert clause noted in § Performance budgets). ADR 0010 records the rationale; `THREATS.md` § T13 covers the supply chain.
- **Build tooling:** **`cargo-dist`** orchestrates archives + the global-artifacts-jobs splice; **`nfpm`** produces `.deb` + `.rpm`; **`makensis`** + **NSIS** produce the Windows `.exe`; **`hdiutil`** produces `.dmg`. Hand-rolled GHA workflow snippet still pending for `.snap`.
- **CI provider:** **GitHub Actions** (free macOS / Windows / Linux runners; integrates natively with `cargo-dist`).
- **Code signing — Posture 3 (skip all signing at MVP).** Deliberate, time-bounded decision:
  - Signing is deferred until the project demonstrates real-world interest. Re-evaluate Posture 1 (sign everything, ~$220–500/yr) once any of the following triggers fires:
    - 100+ GitHub stars, OR
    - 1000+ cumulative installer downloads across releases, OR
    - 6 months of active maintenance.
  - **Mandatory MVP-blocker for usability:** `README.UI.md` must include a concise *"Why is my OS warning me?"* section covering:
    - macOS: right-click `yt-dlp-ui.app` → **Open** → confirm in the Gatekeeper dialog.
    - Windows: the SmartScreen *"More info → Run anyway"* path.
    - Linux: no warnings; nothing to bypass.
  - **`THREATS.md` consequence:** unsigned binaries cannot be cryptographically tied to this project's identity. Users downloading from a non-official source could receive a tampered binary. **Mitigation:** distribute exclusively from GitHub Releases on the canonical repo URL, and publish the SHA256 hash for each release alongside the binaries.
- **yt-dlp binary supply chain:**
  - Fetch upstream's official prebuilt standalone binary from the upstream GitHub release matching the pinned `YT_DLP_VERSION` (per-OS asset: `yt-dlp` for Linux, `yt-dlp_macos` for macOS, `yt-dlp.exe` for Windows).
  - **GPG-verify** the binary's detached signature using the existing `public.key` file at the repo root (upstream's public key, already present and untouched).
  - **SHA256 hash check** against upstream's published `SHA2-256SUMS` for the same release.
  - Pin via `YT_DLP_VERSION` env var in the workflow; bumping is a manual PR for now (auto-bump deferred to production maturity).
  - Copy the verified binary into the build artifact tree at the per-OS bundled location (per Architecture § Bundled-binary path).
- **Artifact hosting:** **GitHub Releases**, attached to git tags (`v0.1.0`, `v0.2.0`, …). SHA256s published per asset.
- **Auto-update:** Out of scope for MVP. Users update by downloading a newer installer. Future option (`tauri-updater`-style polling, Sparkle / WinSparkle) deferred to production maturity.
- **Environments / build channels:** `[development, ci, release]`.
  - `development` — local `cargo run`, debug builds, no installer, no signing.
  - `ci` — every PR builds + tests on all three OSes; no installers produced (saves CI minutes); `cargo test` + `cargo clippy` + `cargo audit` + `cargo deny`.
  - `release` — triggered by a git tag (`v*`); builds, packages, publishes installers (unsigned for now per Posture 3) to GitHub Releases.
- **Secrets management:** **GitHub Actions encrypted secrets** at the repo level. Stores future ad-vendor SDK keys, eventual signing credentials when Posture 1 is adopted, and any other build-time credentials. No external Vault / SOPS / Doppler at MVP.
- **Production observability:** N/A for the application (local file logs only — see Quality & Standards § Observability). For the build pipeline, the GitHub Actions UI is the observability surface; failed builds notify via GitHub email.
- **Backup / DR:** No project-level commitment. Application data backup is the user's responsibility (Time Machine, OneDrive, rsync, etc.). CI/build DR is implicit via the git repo as source of truth — any tag can be re-built.

### Development — local environment

- **Toolchain:** `rustup` with `rust-toolchain.toml` at the repo root pinning channel `1.95.0` and components `["rustfmt", "clippy", "llvm-tools-preview"]`. `llvm-tools-preview` enables `cargo-llvm-cov`. CI installs per-OS toolchains directly on each runner.
- **Per-OS native dependencies (documented in `CONTRIBUTING.UI.md`):**
  - **Linux (Debian / Ubuntu):** `build-essential`, `pkg-config`, `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `libssl-dev`. Equivalent `dnf install` for Fedora-family distros. Required for the `ad-window` crate's `wry` dependency.
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`). System WebKit is used automatically.
  - **Windows:** WebView2 Runtime (ships with modern Edge / Windows 11; on older Windows 10 install the Evergreen redistributable). Visual Studio Build Tools 2022 for the MSVC toolchain (`rustup` prompts during install).
- **Containerization:** Not used. Cross-OS native deps make a single Docker dev environment infeasible; native toolchain on each OS is the right answer.
- **Hot reload / fast feedback:**
  - `cargo run --bin app` — debug-build incremental rebuilds.
  - `slint-viewer` — live preview of `.slint` files without rebuilding the Rust binary.
  - `cargo watch -x 'run --bin app'` — optional auto-rebuild on file change.
- **Sample data:**
  - **`examples/sample-urls.txt`** — a handful of Creative Commons / public-domain URLs (Big Buck Bunny, Sintel, NASA archives, Internet Archive items) for manual QA without copyrighted content.
  - **`examples/fake-ad-server.rs`** — small `axum`-based example binary that serves a static sponsor placeholder image plus a fake click-through URL. Exercised via `cargo run --example fake-ad-server`. `axum` lives in `[dev-dependencies]` only; never ships in release builds.
- **Database migrations:** Hand-rolled `schema_version` table with one migration function per version, called at app startup (per Architecture § SQLite). No external migration tool.
- **Common-commands runner:** **`Justfile`** at the repo root (`just` is cross-platform, cleaner than Make). Recipes:

  ```
  default: lint test
  run: cargo run --bin app
  test: cargo test --workspace
  lint: cargo clippy --workspace --all-targets -- -D warnings
  fmt: cargo fmt --all
  audit: cargo audit
  deny: cargo deny check
  coverage: cargo llvm-cov --workspace --html
  adwin: cargo run --bin ad-window
  fake-ad-server: cargo run --example fake-ad-server
  ```

## Use Cases

Use cases are captured individually under `use-cases/` and indexed in `USE_CASES.md`. The dev-team orchestrator picks one up per `develop` run and updates the ledger's `Status` / `Updated` columns; new use cases are added via the `define-use-case` skill from the root session.

## Scaffolding Plan

This plan enumerates every file and directory I intend to create in `/Users/hhormaechea/Projects/yt-dlp-ui`, every shell command I intend to run, and what I will deliberately NOT do. **No file in the upstream tree is touched.** No `git init`, `git branch`, `git remote add`, `git commit`, or `git push` is run — VCS is already initialized (`master`, remote `git@github.com:HaroldHormaechea/yt-dlp-ui.git`).

### Files & directories to CREATE (all new, no overwrites)

**Workspace root:**

- `Cargo.toml` — workspace manifest. Members: `crates/app`, `crates/ad-window`, `crates/yt-dlp-bridge`. Includes the `[workspace.lints]` table (`unsafe_code = "forbid"`, clippy `all` + `pedantic` warn, two pedantic lints allowed) and the `[profile.release]` shrink settings (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`).
- `rust-toolchain.toml` — pinned to `1.95.0` with components `["rustfmt", "clippy", "llvm-tools-preview"]`.
- `Justfile` — recipes per Deployment § Development.
- `deny.toml` — `cargo-deny` policy: deny GPL/LGPL/AGPL/SSPL SPDX list, allow MIT/Apache/BSD/ISC/MPL/Unicode/Zlib/CC0/Unlicense/0BSD/BSL.
- `LICENSE.UI.md` — PolyForm Noncommercial 1.0.0 license text. **WebFetch is denied in this environment**, so I cannot fetch the canonical text. The file will be created with a clearly-marked TODO header instructing the user to paste the official text from https://polyformproject.org/licenses/noncommercial/1.0.0/. This is the **single manual follow-up** needed after scaffolding.
- `CONTRIBUTING.UI.md` — covers the read-only upstream tree rule, the PolyForm NC license, the three workspace crates and their responsibilities, where new code goes (`crates/*`), per-OS native deps for `wry`. Does NOT overwrite the existing upstream `CONTRIBUTING.md`.
- `THREATS.md` — eight threat categories per Quality & Standards § Security: untrusted URLs, untrusted ad creative, untrusted ad-SDK code, yt-dlp supply chain, local DB tampering (out of scope), crate supply chain, license drift, IPC channel. Includes the unsigned-binaries note from the signing posture.

**Workspace crates:**

- `crates/app/Cargo.toml` — depends on `yt-dlp-bridge` (path), `slint`, `tokio` (full), `rusqlite` (with `bundled` feature), `directories`, `tracing`, `tracing-subscriber`, `tracing-appender`, `serde`, `serde_json`, `thiserror`. `axum` in `[dev-dependencies]` for the fake ad server example.
- `crates/app/src/main.rs` — minimal stub: initializes `tracing_subscriber::fmt`, logs `"yt-dlp-ui starting"`, returns. No UI yet.
- `crates/ad-window/Cargo.toml` — depends on `wry`, `tao`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `thiserror`. No internal crate deps.
- `crates/ad-window/src/main.rs` — minimal stub: logs `"ad-window starting"`, exits.
- `crates/yt-dlp-bridge/Cargo.toml` — depends on `tokio` (`process`, `io-util`, `sync` features only — NOT `full`), `serde`, `serde_json`, `tracing`, `thiserror`. No internal deps.
- `crates/yt-dlp-bridge/src/lib.rs` — minimal stub: `pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }` plus a `#[cfg(test)] mod tests` smoke test.

**Examples:**

- `examples/sample-urls.txt` — a handful of Creative Commons / public-domain URLs (Big Buck Bunny on archive.org, Sintel mirror, NASA archives) with a header comment explaining intended use.
- `examples/fake-ad-server.rs` — `axum`-based dev example that serves a static sponsor placeholder PNG and a `/click` redirect endpoint. Run via `cargo run --example fake-ad-server` (declared in `crates/app/Cargo.toml` `[[example]]`).
- `examples/placeholder-ad.png` — created as a small text-format placeholder file with a TODO note (no actual PNG bytes; the user can drop in a real placeholder image when convenient). Alternative if you'd prefer: omit this and have the example serve an inline base64 1x1 PNG. **Will ask in confirmation.**

**ADRs (`docs/adr/`):**

- `docs/adr/README.md` — explains MADR convention and the index of ADRs.
- `docs/adr/0001-language-and-ui-framework.md` — Rust + Slint + spawned `wry` ad window.
- `docs/adr/0002-license.md` — PolyForm Noncommercial 1.0.0.
- `docs/adr/0003-monetization-model.md` — donations + third-party ad SDK.
- `docs/adr/0004-ad-window-process-isolation.md` — separate process for ad WebView.
- `docs/adr/0005-yt-dlp-bundling.md` — bundle upstream single-file binary; never modify upstream tree.
- `docs/adr/0006-storage.md` — SQLite via `rusqlite` `bundled` feature.

**`.gitignore` (APPEND, not overwrite):**

The existing `.gitignore` at the repo root is upstream's (Python-tooling oriented). I will read it, then append a `# yt-dlp-ui (Rust workspace)` block with:
- `/target/`
- `*.profraw`
- `.cargo/config.toml.local`
- `examples/placeholder-ad.png` (only if the user wants it git-ignored — will ask)

This is the only edit to a pre-existing repo-root file. It is an append, not an overwrite, and adds no Python-tree entries.

### Files / directories I will DELIBERATELY NOT TOUCH

The entire upstream yt-dlp tree is read-only at the source level:
`yt_dlp/`, `bundle/`, `devscripts/`, `test/`, `pyproject.toml`, `uv.lock`, `Makefile`, `Changelog.md`, `CONTRIBUTORS`, `CONTRIBUTING.md`, `Maintainers.md`, `supportedsites.md`, `THIRD_PARTY_LICENSES.txt`, `LICENSE`, `README.md`, `yt-dlp.cmd`, `yt-dlp.sh`, `public.key`, `.pre-commit-config.yaml`, `.pre-commit-hatch.yaml`, `.editorconfig`, `.gitattributes`, `.github/`.

I will NOT create `README.UI.md` during scaffolding either. The `write-readme` skill is offered separately at the end of scaffolding, with an explicit warning that the existing `README.md` is upstream's 178 KB README and must NOT be overwritten.

### Shell commands

A single `mkdir -p` to create empty directory shells the `Write` tool cannot create on its own (specifically just `crates/yt-dlp-bridge/tests` and `crates/app/tests` and `crates/ad-window/tests` — the leaf directories that need to exist for Cargo's integration-test convention even when empty; each gets a `.gitkeep`). `Write` creates parents implicitly for files, so other directories (`crates/*/src`, `examples`, `docs/adr`) are created as side effects of writing files into them.

```
mkdir -p \
  /Users/hhormaechea/Projects/yt-dlp-ui/crates/app/tests \
  /Users/hhormaechea/Projects/yt-dlp-ui/crates/ad-window/tests \
  /Users/hhormaechea/Projects/yt-dlp-ui/crates/yt-dlp-bridge/tests
```

No `git init`, `git branch`, `git remote add`, `git commit`, `git push`, `cargo build`, `cargo test`, or any other side-effecting command runs during scaffolding.

### Known scaffolding gaps (recorded for follow-up)

1. **`LICENSE.UI.md` content** — TODO header with paste-from-URL instruction (WebFetch denied in this environment).
2. **Snap publishing GHA workflow** — `cargo-dist` does not generate snap artifacts; a separate workflow with `snapcore/action-build` + `snapcore/action-publish` will be required when the `release` channel is wired up. Not in MVP scaffold; recorded here.
3. **`cargo-dist` (v0.31.0) initialized and wired up post-scaffold (2026-04-25).** `dist init --yes` created `dist-workspace.toml` and added `[profile.dist]` to the root `Cargo.toml`. `dist` 0.31.0 hardcodes the GHA workflow path to `.github/workflows/release.yml`, which is also the path of upstream yt-dlp's existing reusable release workflow. **Resolved by renaming upstream's file:** `git mv .github/workflows/release.yml .github/workflows/release-upstream.yml` and updating all four `uses:` references in `release-master.yml` and `release-nightly.yml`, plus a comment reference in `test-workflows.yml`. `dist generate --mode ci` then wrote our release workflow at the freed path. The dual workflow scheme is now: `release-upstream.yml` is upstream's reusable workflow (still called by `release-master.yml` / `release-nightly.yml` for upstream yt-dlp Python releases); `release.yml` is dist-managed and triggers on tags matching the dist contract. The user authorized touching upstream files when required for build / dist / GitHub Actions setup. Note: cargo-dist 0.31.0 ships as a standalone `dist` CLI, not a `cargo dist` subcommand.
4. **`examples/placeholder-ad.png`** — the actual PNG bytes are not generated by this scaffold; either a TODO text file is dropped or the example serves an inline base64 1x1 PNG. Decision pending in confirmation.

### Two minor decisions I need from you before executing

1. **Placeholder PNG:** create a text-format `examples/placeholder-ad.png.TODO` with a note (cleaner — the user adds a real PNG later), OR have `examples/fake-ad-server.rs` serve an inline base64 1x1 transparent PNG (no extra file, slightly uglier code)?

2. **`examples/placeholder-ad.png` git-ignore:** if you choose option (a) above, no ignore entry. If you choose (b), nothing to ignore. So this question collapses into question 1.

**Please confirm the plan and answer the placeholder-PNG question.** I'll then execute exactly what's listed above — no more, no less. If anything is missing, I'll stop, update this section, and reconfirm before continuing.
