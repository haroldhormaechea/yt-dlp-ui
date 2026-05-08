# 0004 — Ad-window process isolation

- **Status:** accepted
- **Date:** 2026-04-25

## Context

The project requires a **very lean** idle RAM footprint (target 30–60 MB
when no ad is visible). Web-based ad SDKs require a WebView — a typical
WebView idles at 80–200 MB. Inline-WebView frameworks (Tauri, Electron,
Wails) cannot release that memory while the app is running.

The user proposed an architecture in which the ad slot lives in a separate
window that can be killed when ads should not be visible. This decision
formalizes that approach.

## Decision

The ad slot is a **separate child process** built on `wry` (system WebView)
+ `tao` (window/event-loop). The main `app` process (Slint) spawns the
`ad-window` binary on demand and kills it when ads should not be visible.

**Spawn triggers:**
- Main window becomes visible (deminimized, focused, or shown).
- Ad-consent flag is true in settings.
- Timer-based rotation tick during long sessions.

**Kill triggers:**
- Main window minimized or hidden for more than 5 seconds (debounce).
- Focus mode toggled on.
- User disables ads in settings.
- App quit.

**IPC:** newline-delimited JSON over stdin/stdout pipes between `app` and
`ad-window`. Chosen for portability (zero per-OS code paths) and security
(kernel-enforced parent/child boundary).

**Crash recovery:** if `ad-window` dies unexpectedly, `app` does **not**
auto-respawn immediately. Exponential backoff: 5 s → 30 s → 5 min → give
up for the rest of the session. Ad failures **must never block downloads**
— the user came to download things, not to see ads.

## Consequences

**Positive:**
- Idle RAM target (30–60 MB) is achievable: the WebView's memory cost only
  exists when an ad is actually visible.
- Ad webview is sandboxed in a separate process — strong trust boundary
  against malicious ad creative.
- The ad SDK runs **only** inside `ad-window`, never inside `app`. If the
  user disables ads, no SDK code executes anywhere.
- IPC channel is small, well-defined, and doesn't require any third-party
  framework.

**Negative:**
- Two binaries to sign per OS (when signing is adopted).
- Lifecycle complexity: spawn / kill / crash-recovery is custom code that
  must be tested.
- The ad-window-on-visibility model is an MVP design subject to revision
  once real-world ad-vendor behavior is observed (creative load times,
  vendor SDK retry semantics, click-through expectations).

## Alternatives considered

- **Inline ads in the same WebView (Tauri / Electron / Wails)** — simplest
  to build, but pays the WebView memory cost continuously. Doesn't hit the
  lean idle-RAM requirement.
- **Native ad SDK in-process** — no maintained desktop ad SDK exists for
  any modern framework; you'd embed a webview anyway, defeating the point.
- **No ad slot at all** — would force a different monetization model.
  Already rejected at the monetization decision.

## References

- PROJECT_BRIEF.md § Architecture § Ad-window lifecycle
- THREATS.md § T2 (untrusted ad creative), § T8 (IPC channel)
