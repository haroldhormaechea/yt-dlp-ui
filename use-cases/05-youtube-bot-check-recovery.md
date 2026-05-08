# Use Case 05: YouTube bot-check recovery (cookies-from-browser + deno bundling)

## Summary

When yt-dlp returns the YouTube bot-check error during metadata fetch or download for any row in the queue, the app surfaces a single modal pop-up offering cookies from one of the user's installed browsers. The dialog enumerates installed browsers via per-OS heuristics (macOS / Linux / Windows) from the canonical yt-dlp set; if zero browsers are detected, the dialog is skipped in favor of a clear "no supported browser found" toast. The user picks one browser and optionally checks "Remember this choice"; the pick applies to ALL currently bot-checked rows in the batch (a second row hitting bot-check while the dialog is open is held in a `waiting_on_user` transient state, not surfaced as a separate error). The persisted choice (`cookies_browser=<browser>` in the SQLite settings KV table) eliminates future prompts unless the chosen browser later fails. The Settings panel gains a "Cookies source" dropdown to change or reset the choice. The bridge gets a typed `BridgeError::AuthRequired` variant and a stderr pattern matcher for "Sign in to confirm you're not a bot" + "Use --cookies-from-browser". Both metadata and download invocations forward `--cookies-from-browser <browser>` when the setting is non-`None`. Additionally, UC 05 ships deno bundling (the JavaScript runtime yt-dlp 2026.x needs for full YouTube extraction): for dev, deno is expected on PATH (`brew install deno`, etc.) and a startup probe warns clearly without blocking if it is missing; for release, a build/release-time hook fetches the upstream deno binary for the target platform, verifies SHA256, and places it at the per-OS bundled path next to yt-dlp. The bridge passes `--js-runtimes deno:<bundled-path>` when bundled, falling back to PATH lookup or yt-dlp's default warning when it is not.

## Acceptance Criteria

### Cookies / bot-check path

1. The bridge returns a typed error variant (e.g. `BridgeError::AuthRequired { stderr_tail }`) when yt-dlp's stderr matches the bot-check pattern. The variant is distinct from `BridgeError::ExitedWithError` so the UI can branch.
2. The bot-check matcher recognizes at minimum the canonical phrase fragments `"Sign in to confirm you're not a bot"` and `"Use --cookies-from-browser"` (case-insensitive substring match). On no-match, the bridge falls through to `ExitedWithError` so unknown failures still surface.
3. On `BridgeError::AuthRequired` during `add_url` (metadata fetch) AND when the persisted `cookies_browser` setting is `None`, the app surfaces a modal dialog before transitioning the row to `error`.
4. **Multi-row batching:** while the dialog is open, any other row that hits the bot-check is held in a `waiting_on_user` transient state (not transitioned to `error`, not displayed as a separate error). The user's choice — pick or cancel — applies to ALL currently bot-checked rows in the batch atomically.
5. The dialog enumerates installed browsers via per-OS heuristics. Coverage:
   - **macOS:** `/Applications/<Browser>.app` and `~/Applications/<Browser>.app` exist.
   - **Linux:** `which <browser-binary>` returns success, OR a known config dir exists (`~/.mozilla/firefox`, `~/.config/google-chrome`, `~/.config/BraveSoftware/Brave-Browser`, etc.).
   - **Windows:** registry under `HKLM\Software\Microsoft\Windows\CurrentVersion\App Paths\<Browser>.exe` OR `HKCU\...` returns a valid path, OR `%PROGRAMFILES%`/`%PROGRAMFILES(X86)%`/`%LOCALAPPDATA%` contains the install dir.
   - Browsers covered: Brave, Chrome, Chromium, Edge, Firefox, Opera, Safari (macOS only), Vivaldi.
6. **Zero installed browsers detected:** the dialog is NOT shown. Instead, a clear toast appears — "No supported browser detected; install Brave / Chrome / Firefox / etc. to use cookies." The row goes to `error` with a tooltip pointing at the same install guidance. The Settings "Cookies source" dropdown is disabled in this state. Detection re-runs on app launch.
7. The dialog includes a "Remember this choice" checkbox (default unchecked).
8. On user picks browser AND checks "Remember": the app persists `cookies_browser=<browser>` in the SQLite settings KV table. Future yt-dlp invocations from this session and future sessions forward `--cookies-from-browser <browser>` automatically (no further prompts unless the chosen browser later fails).
9. On user picks browser WITHOUT checking "Remember": the cookies are used for the immediate batch retry only. The persisted setting stays `None`, and the next bot-check on a future row will show the dialog again.
10. After the user picks, all batch rows are retried with `--cookies-from-browser <browser>` automatically. Successes proceed normally (transition to `queued` then `in_flight` per the existing flow). Failures surface the new error to the user (no infinite re-prompt for the same row in this attempt).
11. On user cancels: all batch rows transition to `error` with a tooltip — "YouTube blocked this download. Set a Cookies source in Settings to retry."
12. The Settings panel gains a "Cookies source" dropdown with options: None / installed-browsers list. Default value is None. Changes persist immediately to the settings KV table. The dropdown is disabled when zero browsers are detected; a small explanatory note explains why.
13. The cookies setting is forwarded by `DownloadManager` to BOTH metadata fetches AND download invocations. Bot-check can fire on either, so both paths must use the cookies when available.

### Dialog modality

The dialog is a standard modal: it dims the main window, focuses input on the dialog, and the user MUST act (pick or cancel) before continuing. Implemented via Slint's modal-window mechanism. The user's framing is "pop-up", and the modal style is the natural match.

### Deno path

14. App startup probes for deno in this priority order: (1) the per-OS bundled path (next to the bundled yt-dlp binary, computed via the same per-OS rules already used by `bundled_yt_dlp_path()`), (2) `which deno` (PATH lookup). The resolved path is stored in app state for the bridge to consume.
15. If deno is not found at either location, the app logs a WARN-level `tracing` message and shows a one-time non-blocking banner — "Some YouTube downloads may require Deno; install via `brew install deno` (or platform equivalent)." The banner is dismissible and does not block startup. The user can continue without deno; some YouTube videos may then partially fail with degraded format selection (yt-dlp's existing warning behavior).
16. Bridge invocations of yt-dlp pass `--js-runtimes deno:<resolved-path>` when a bundled deno is found. When only PATH-deno is available, the flag is omitted (yt-dlp finds deno on its own PATH). When neither is available, the flag is omitted and yt-dlp prints its default "no JS runtime" warning to stderr (which UC 05 does not promote to an error).
17. A new build/release-time hook (location chosen by the analyst — likely a script invoked from the release-channel GHA workflow alongside the existing yt-dlp fetch) downloads the upstream deno binary for the target platform from the official deno GitHub releases (https://github.com/denoland/deno/releases), verifies SHA256 against a pinned digest in the workflow, and places the binary at the per-OS bundled path next to yt-dlp. The hook is implemented and unit-testable; it is NOT yet wired into a fully functional release pipeline (release-pipeline completion is a separate open deployment-channel work item per `PROJECT_BRIEF.md` § Deployment).
18. For DEV, no bundling occurs. Developers install deno locally (documented in `README.UI.md` — see AC#20). The startup probe finds it via PATH.

### Cross-cutting

19. `README.UI.md` gains a "Cookies and YouTube bot-check" section: what `--cookies-from-browser` does, no exfiltration (cookies leave only via the request to YouTube — same as the user's own browser), the macOS Keychain prompt expectation for Chrome, the browser-DB-locked behavior on Linux Chrome, where to change the setting later in the Settings panel.
20. `README.UI.md` Requirements section adds Deno: `brew install deno` (macOS), `apt install deno` (Linux, when packaged) or upstream binary, equivalent on Windows. Marked as optional but recommended for YouTube users.
21. `THREATS.md` gets a new section on the cookies-from-browser feature: trust posture (cookies live and stay on the user's machine; they accompany requests to YouTube the same as the browser would), exfiltration boundary (no upload to any project-controlled server), browser-cookie-DB encryption considerations (Chrome on macOS triggers a Keychain prompt; that prompt is the OS asking the user, not the app).
22. `PROJECT_BRIEF.md` § Workspace crate dependency graph is amended if any small platform crate is judged necessary by the analyst (e.g. `winreg` for Windows registry reads). The team-lead adjudicates the amendment if the analyst proposes one.
23. New unit tests cover: the bot-check stderr pattern matcher (positive matches, no-match fall-through, unicode/whitespace variations), browser detection per OS (cfg-gated tests with mocked filesystem fixtures using `tempfile`), settings KV round-trip for the new `cookies_browser` key, deno-path resolution priority (bundled > PATH > none), bridge cookies-arg forwarding (the cookies arg appears in metadata AND download invocations), and the dialog state machine (where it is testable from non-UI code, e.g. the batch-state model).
24. Existing tests (UC 01 through UC 04) continue to pass unchanged.
25. All three gates pass: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`.

## Potential Pitfalls & Open Questions

- **Risk** — Scope is wide for a single UC: cookies dialog + multi-row batching + browser detection + Settings change + bridge integration + deno startup probe + deno bundling mechanism + README + THREATS. Comparable in size to UC 01. The analyst should plan internal milestones; the challenger should push back on any milestone that bloats the diff. Splitting deno into a future UC was offered but the user explicitly picked the wider scope.
- **Risk** — Deno bundling overlaps with the still-open release-pipeline scaffolding (`PROJECT_BRIEF.md` § Deployment). UC 05 implements the mechanism (fetch + verify + place); the actual GHA workflow that invokes it depends on the broader release-pipeline UC eventually being filed. The analyst should document this seam clearly: UC 05 ships a callable hook, NOT a working end-to-end release pipeline.
- **Edge case** — Multiple Chrome (or Brave / Edge / etc.) profiles on one machine. yt-dlp's default profile usually works; advanced users need `<browser>:<profile>` syntax. Out of scope for v1; profile selection becomes a follow-up UC if user feedback warrants it.
- **Edge case** — Browser cookie-DB locked. Chrome on Linux requires the browser to be closed before yt-dlp can read its cookie SQLite. Chrome on macOS triggers a Keychain prompt the first time. yt-dlp surfaces clear errors in those cases — UC 05 must pass those errors through to the UI without re-mapping or hiding.
- **Risk** — Bot-check error pattern in stderr is not a stable yt-dlp contract. Pin via unit test against the current pattern (the user's reported error message verbatim, plus the canonical `--cookies-from-browser` recommendation). Fall through to generic `ExitedWithError` if no recognized phrase matches. A future yt-dlp message rewording would silently regress UC 05 to "no dialog, generic error" — acceptable failure mode (regenerates the user's pre-UC-05 experience, not a worse one).
- **Edge case** — Region-locked content may STILL fail after cookies enable, with a different yt-dlp error. The UI must surface yt-dlp's actual error message rather than claim the cookies fixed the problem. The bot-check matcher must NOT match on region-locked errors.
- **Risk** — Deno SHA256 pinning means upgrading the bundled deno requires a manual digest bump, same pattern as yt-dlp version pinning per `PROJECT_BRIEF.md` § Deployment. Document the upgrade procedure in the release-channel runbook (or wherever the yt-dlp version is currently documented).
- **Risk** — UI thread vs tokio: the dialog must be triggered via `slint::invoke_from_event_loop` from the bridge-error event channel; the retry logic must spawn a new tokio task once the user has picked. The analyst's proposal must spell out the cross-thread coordination explicitly (which thread owns the dialog state, how the batch's "waiting_on_user" rows are tracked, how the user's pick triggers the retry across N rows).
- **Risk** — Deno's MIT license and yt-dlp's Unlicense are both cargo-deny clean per `deny.toml`. No license-policy work needed for the bundling.
- **Edge case** — Browser detection on Snap-installed Chrome on Linux puts cookies in `~/snap/chromium/common/chromium/` (or similar) instead of `~/.config/google-chrome/`. yt-dlp may or may not handle this correctly. Out of UC 05's detection scope; if yt-dlp fails to read snap Chrome cookies, surface its error.
- **Edge case** — Browser detection on macOS via the filesystem heuristic finds `.app` bundles but not Mac App Store-installed Safari (which is part of the OS, lives in `/Applications/Safari.app` always). Safari detection on macOS = "macOS host" essentially. Confirm in the proposal.

## Original Description

> "if we detect this kind of error we should prompt the user via pop-up to use their browser cookies, and then show the list of installed browsers so they can pick one. And offer with a checkbox the possibility of 'always select this one' while offering the opportunity to change it in the settings to another one or none"
>
> Triggered by the bot-check error from yt-dlp on a user-supplied YouTube URL:
>
>     Bridge(ExitedWithError {
>       code: Some(1),
>       stderr_tail: "WARNING: [youtube] No supported JavaScript runtime could be found. ... \nWARNING: [youtube] No title found in player responses; falling back to title from initial data. ...\nERROR: [youtube] B10ECkQXQtU: Sign in to confirm you're not a bot. Use --cookies-from-browser or --cookies for the authentication. ..."
>     })

## Clarifications

- Q: Multi-row queue behavior — N rows all hit bot-check at the same time. What should the user see?
  A: One global prompt; choice applies to all. While the dialog is open, other bot-checked rows are held in a `waiting_on_user` transient state. User's pick (or cancel) applies to ALL currently failed rows.
- Q: Zero browsers detected on the user's machine — what does the dialog do?
  A: Show error toast, no dialog. Settings dropdown stays disabled until detection succeeds (re-run on app launch).
- Q: Should UC 05 also include deno (JavaScript runtime) detection / install guidance, or split that out?
  A: Include deno bundling — ship deno alongside yt-dlp. UC 05 implements the bundling mechanism (fetch + verify + place at per-OS bundled path), the dev-time runtime probe + warning banner, and the bridge's `--js-runtimes deno:<path>` forwarding. Full release-pipeline wiring remains a separate work item.
- Q: Dialog modality — the cookies prompt blocks the whole app, or just the affected rows?
  A: Modal pop-up. Standard modal: dims main window, requires the user to pick or cancel before continuing.
