# 0001 — Language and UI framework

- **Status:** accepted
- **Date:** 2026-04-25

## Context

`yt-dlp-ui` is a cross-platform desktop GUI wrapping the upstream `yt-dlp`
CLI. Hard constraints:

- Must run on Linux, macOS, and Windows; per-OS installers required.
- Must be **very lean** in idle RAM — the main UI is up while a queue runs,
  potentially for hours, and may also display ads.
- Must integrate a third-party ad-network SDK, which is JavaScript-based.
- Maturity target: MVP, evolving toward production.

The framework choice has the biggest downstream impact on bundle size,
idle-RAM footprint, install UX, and engineering cost. Six candidates were
evaluated honestly: Tauri 2, Electron, PySide6 + PyInstaller, Flutter
desktop, Wails v2, .NET MAUI / Avalonia.

## Decision

Use **Rust** (edition 2024, toolchain `1.95.0`) with **Slint** as the UI
framework. The ad slot is a separate child process built on **`wry`**
(system WebView) + **`tao`** (window/event-loop), spawned only when an ad
should be visible and killed (process exit) when the main window is
minimized, the user is in focus mode, or settings disable ads.

## Consequences

**Positive:**
- Lowest idle-RAM footprint of any credible option (~30–60 MB main app
  alone). Hits the "very lean" requirement.
- Native rendering via Slint — small, GPU-accelerated, OS-native a11y APIs
  for free.
- Out-of-process ad slot: when minimized or in focus mode, the ad WebView is
  a killable separate binary. The 80–200 MB cost of a WebView only exists
  when an ad is actually visible.
- Single language for all three crates (`app`, `ad-window`, `yt-dlp-bridge`).
  No JS / Python / Dart split.
- Static linking + `panic = "abort"` + `strip = true` + `lto = "fat"` keeps
  binary size small.

**Negative:**
- Rust onramp is real for an engineer new to it. Estimated weeks for an
  experienced engineer; tolerable for a small project but explicit cost.
- Slint has a smaller ecosystem than React/Tauri or Qt — fewer drop-in
  components, fewer Stack Overflow hits.
- Two binaries to sign per OS (when signing is adopted) — `app` and
  `ad-window`.
- Ad-window IPC is custom code we own; no off-the-shelf framework.

## Alternatives considered

- **Tauri 2** — strong default, but the WebView is the entire app, so the
  idle floor is ~80–200 MB. Doesn't hit "very lean."
- **Electron** — easiest ad-SDK story, largest ecosystem, but 200–400 MB
  idle is far above the budget.
- **PySide6 + PyInstaller** — would match upstream's language, but ad-SDK
  integration requires `QWebEngineView` (Chromium under the hood, ~60 MB
  alone), which negates the "small native bundle" advantage.
- **Flutter desktop** — Google Mobile Ads SDK is mobile-only; you'd embed a
  webview anyway. Smaller desktop community.
- **Wails v2 (Go)** — same architectural shape as Tauri, smaller community.
  No advantage over Tauri for this use case.
- **.NET MAUI / Avalonia** — Avalonia is fine, MAUI is weak on Linux.
  Neither offers a leaner footprint than the chosen stack and the C#
  ecosystem fit is no better than Rust's.

## References

- PROJECT_BRIEF.md § Technologies
- PROJECT_BRIEF.md § Architecture
