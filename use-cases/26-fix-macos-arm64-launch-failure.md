# Use Case 26: Fix macOS arm64 launch failure (Dock-bounce-and-die on macOS 26.x)

## Summary

On macOS 26.3.1 (Apple Silicon, M-series), launching the latest GitHub-released `.dmg` produces a Dock icon that bounces briefly and then disappears, with no main window appearing. The user reproduced this both running the app directly from the mounted DMG and after copying it to `/Applications`, and in both cases explicitly granted Gatekeeper's "open anyway" override for the unsigned binary — yet the bounce-and-die persists. Because the user-override clears Gatekeeper *assessment* but not macOS 26.x's AMFI / dyld-load signature enforcement, the most likely root causes are: (a) the user downloaded the `x86_64-apple-darwin` `.dmg` from the release page and is running it on arm64 hardware without Rosetta — which produces exactly this symptom; (b) the binary is killed at dyld load by AMFI because of missing/invalid hardened-runtime + Developer ID signing on macOS 26.x; or (c) a startup crash inside `app` before window creation (panic, dyld load of a bundled framework, Info.plist mismatch, missing bundled-binary at expected path). The release pipeline does target `aarch64-apple-darwin` via cargo-dist (`dist-workspace.toml:13`), so an arm64 .dmg should be present in the latest release; if it is, the failure is signing/runtime, not asset selection. Investigation must (1) confirm which `.dmg` artifact was downloaded and that the arm64 build exists and launches, (2) collect `log show --last 5m --predicate 'process == "yt-dlp-ui"'` for the failed launch, and (3) deliver a fix covering both the install UX (so a non-technical user cannot easily grab the wrong-arch asset) and the technical launch failure on macOS 26.3.1.

## Acceptance Criteria

1. The latest release lists the macOS arm64 `.dmg` as the prominent / default macOS download asset, with naming that a non-technical user can read (e.g. `yt-dlp-ui-vX.Y.Z-macos-arm64.dmg` and `…-macos-intel.dmg`, not raw `aarch64-apple-darwin` triples).
2. On a clean macOS 26.3.1 arm64 install with **no Rosetta**, downloading the macOS arm64 `.dmg` from the latest release and either (a) launching the app from inside the mounted DMG or (b) dragging it to `/Applications` and launching from there results in the main Slint window appearing within the existing **<1 s startup target**.
3. The bounce-and-die symptom no longer reproduces on macOS 26.3.1 arm64 with the latest release.
4. The downloaded arm64 `.dmg` passes `codesign --verify --deep --strict --verbose=2 <app>`, `spctl --assess --verbose=4 --type execute <app>`, and `stapler validate <app>` on macOS 26.3.1, with a valid Developer ID signature and stapled notarization ticket.
5. No "Apple cannot check this for malicious software" / "is damaged and cannot be opened" / "from an unidentified developer" dialog appears for the latest-release arm64 `.dmg` on macOS 26.3.1.
6. The bundled `yt-dlp` and `ffmpeg` binaries inside `Contents/Resources/` resolve and execute from the running app — i.e., they survive Gatekeeper translocation / quarantine attributes, are codesigned and notarized as part of the parent bundle, and `bundled_yt_dlp_path()` / `bundled_ffmpeg_path()` resolution works post-install.
7. If the root cause turns out to be a runtime crash inside `app` rather than packaging, the failure is (a) written to `<app-data>/logs/yt-dlp-ui.log.YYYY-MM-DD` via `tracing` and (b) surfaced to the user in some discoverable way — silent exit before the main window is not acceptable.
8. A regression check is added: either an automated CI smoke that validates the macOS arm64 bundle structure + codesign + notarization staple on every release build, or an explicit manual smoke step recorded in the macOS release runbook (verifying `spctl` / `codesign` / `stapler` against the produced artifact).
9. The Intel `.dmg` (`x86_64-apple-darwin`) either (a) remains shipped and is given a separate, clearly-labeled download with Rosetta-requirement guidance, or (b) is dropped from the release if Intel support is no longer in scope. Project decision recorded in `docs/adr/`.
10. The root cause, the fix, and the install-UX changes are recorded in `docs/adr/` (new ADR, or amendment to an existing macOS-relevant one).

## Potential Pitfalls & Open Questions

- **Missing input** — Console / `log show` output for the failed launch is the single highest-signal diagnostic and has not been collected. The dev team will need the reporter to run `log show --last 5m --predicate 'eventMessage CONTAINS "yt-dlp-ui"'` (or `process == "yt-dlp-ui"`) and attach the output.
- **Missing input** — Which exact `.dmg` filename did the user download from the release page? `x86_64-apple-darwin` vs `aarch64-apple-darwin` decides whether the root cause is "wrong-arch asset" or "actual launch failure on the correct arch." This must be checked before the dev team picks a fix path.
- **Missing input** — Is Rosetta installed on the user's machine? `/usr/bin/pgrep -lf oahd` will show. If Rosetta is absent and the downloaded asset is x86_64, the symptom is fully explained.
- **Assumption** — Apple Developer ID credentials and notarization tooling (`notarytool`, `codesign`) are not currently wired up in the GitHub Actions release pipeline. If true, properly fixing the signing path is a multi-step release-pipeline change that may need its own ADR and secrets setup (App Store Connect API key in GitHub Secrets). This is implied scope of this use case unless explicitly split out.
- **Risk** — Reproducing requires macOS 26.3.1 specifically. The dev team likely does not have that OS available; reproduction confirmation will round-trip through the reporter. Mitigation: build a self-test the reporter can run that produces the same diagnostics regardless of macOS version.
- **Risk** — A startup crash inside `app` (case c) cannot be ruled out without log output. If the arm64 binary still bounce-and-dies after signing is fixed, this becomes the residual cause and needs a separate investigation pass (panic in DB open, missing dylib, Slint runtime mismatch, etc.).
- **Edge case** — macOS 26.x has tightened AMFI enforcement of hardened-runtime requirements. Even with Developer ID signing, missing entitlements (e.g., `com.apple.security.cs.disable-library-validation` if the app loads non-Apple-signed dylibs from `Contents/Resources/`) can cause silent kills. The bundled `yt-dlp`, `ffmpeg`, and `deno` binaries are subprocess executables not dylibs, so they're less likely to trip this, but cross-arch fat binaries or Mach-O quirks could.
- **Edge case** — If the user already attempted to launch the app multiple times before allowing Gatekeeper, macOS may have cached a kill decision via `quarantine` xattrs. `xattr -dr com.apple.quarantine <app>` on the reporter's machine clears it for a clean retry.

## Original Description

I need to review a bug: When attempting to install in a MacOS 26.3.1, after clicking on the .dmg, an icon shows in the bottom launch bar and begins bouncing, until it stops bouncing but does nothing. The app isn't installed or any window shown.

## Clarifications

- Q: Where was the app launched from when the icon bounced and disappeared?
  A: When double clicked directly, even after allowing running unsigned applications, the icon in the launch bar bounces until it stops but does nothing. After moving it to the Applications folder, and opening it there (again allowing unsigned apps), the same happens.
- Q: Which architecture is the affected Mac running?
  A: Apple Silicon (arm64).
- Q: Which release artifact reproduces this?
  A: Latest GitHub release.
- Q: What's the scope of this use case?
  A: Root-cause launch failure AND fix install UX.

## Investigation notes — `log show` analysis (2026-05-10)

Reporter captured a unified-log dump (`log-app.txt`, ~5 MB / 31 k lines) during a reproduction. Two consecutive launch attempts are visible, both from `/Applications/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui`, bundle id `com.haroldhormaechea.yt-dlp-ui`. The bundle was installed (`MobileInstallation` "Built bundle record for app" at line 14165), and macOS spawned the process both times via `launchd`.

### What the log actually shows

1. **First launch — pid 5414.** Spawned cleanly (no AMFI fast-kill). Lived for **113,474 ms (~113 s)** in state `running-active-**NotVisible**`. User finally force-quit from the Dock (`Dock: [com.apple.dock:tile] Calling force quit asn=(0x0 0x9b09b)` at line 7273). At the moment of termination, the kernel logged:
   ```
   kernel: (AppleSystemPolicy) ASP: Sleep interrupted: ref 34, signal 0x4000, pid: 5414
   kernel: (AppleSystemPolicy) ASP: Security policy would not allow process: 5414, /Applications/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui
   launchd: ... exited due to SIGTERM | sent by Dock[626], ran for 113474ms
   loginwindow: ... kLSNotifyApplicationAbnormalDeath, app must have crashed
   ```

2. **Second launch — pid 5455** (≈7 s after the force-quit). Also spawned cleanly. Reached state `running-active-NotVisible`, then transitioned to `UserInteractiveNonFocal` at 20:05:12. The log file ends ~28 s later with the process still in this state — i.e., the symptom reproduces.

### Corrected hypothesis

The "Dock bounces then stops" symptom is **not** an AMFI / Gatekeeper fast-kill at exec time — the parent binary executes and stays alive. The actual failure mode is:

- The process reaches `running-active-NotVisible` and **never connects a window to WindowServer**, so no UI ever appears.
- The Slint main window is never created or never made visible during startup, despite the process being alive.
- `AppleSystemPolicy` denies *something* during execution (note the message fires at termination, not at exec — suggesting the policy denial applied to an in-process operation such as a subprocess `exec()` or a TCC-gated resource access, and was logged when the process was finally reaped).

The most credible chain given the architecture documented in `PROJECT_BRIEF.md` (`app` spawns `ad-window` as a child process built on `wry` + `tao`; `app` also opens SQLite, may request access to user data directories on first launch):

- **Hypothesis H1 — subprocess-exec denial.** `app` spawns `ad-window` at startup and blocks on the IPC `ready` event. AppleSystemPolicy denies the `exec()` of the unsigned/ad-hoc-signed child binary inside `Contents/MacOS/` (macOS 26.x library-validation rules are stricter). `app` waits forever on the pipe, never reaches the "show window" call. Matches the 113 s hang and the `ASP: Security policy would not allow process` message.
- **Hypothesis H2 — TCC / file-system access prompt that never appears or is silently denied.** First-launch access to `~/Library/Application Support/yt-dlp-ui/` for `db.sqlite` could trip a TCC prompt that for some reason is not surfaced to the user (e.g., because the app has no visible window to attach the prompt to). The SQLite open call hangs; the window is never created.
- **Hypothesis H3 — Slint / `wry`-`tao` initialization deadlock under macOS 26.** Less likely but possible — a runtime change in macOS 26's Cocoa AppKit interaction with Slint or with non-`NSApplicationMain` startup could deadlock the main thread.

### What the log does NOT tell us

- Architecture of the running binary (Mach-O `cputype`). `lipo -info /Applications/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui` on the reporter's machine is the next test. The user confirmed Apple Silicon hardware, but we have not confirmed which `.dmg` was downloaded.
- Signature state. `codesign -dvv /Applications/yt-dlp-ui.app` would show whether the bundle is ad-hoc, Developer-ID signed, or unsigned, and the entitlements.
- Whether `ad-window` (or any other child binary inside `Contents/MacOS/` or `Contents/Resources/`) is independently signed.
- Whether the app log itself (`~/Library/Application Support/yt-dlp-ui/logs/yt-dlp-ui.log.*`) was created — if it exists, it tells us how far into the Rust `main()` we got before hanging.

### Updated next steps for the dev team

1. Confirm Mach-O arch + signature posture on the reporter's machine:
   ```
   lipo -info /Applications/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui
   codesign -dvv /Applications/yt-dlp-ui.app
   codesign --verify --deep --strict --verbose=2 /Applications/yt-dlp-ui.app
   spctl --assess --verbose=4 --type execute /Applications/yt-dlp-ui.app
   stapler validate /Applications/yt-dlp-ui.app
   ls -la /Applications/yt-dlp-ui.app/Contents/MacOS/ /Applications/yt-dlp-ui.app/Contents/Resources/
   codesign -dvv /Applications/yt-dlp-ui.app/Contents/MacOS/ad-window
   ```
2. Check whether the Rust-side log was created and how far init progressed:
   ```
   ls -la ~/Library/Application\ Support/yt-dlp-ui/
   cat ~/Library/Application\ Support/yt-dlp-ui/logs/yt-dlp-ui.log.*
   ```
3. Run with `sample` attached to the bouncing process to capture a stack trace of the hang point:
   ```
   open -a /Applications/yt-dlp-ui.app
   sleep 5
   sample yt-dlp-ui 10 -file /tmp/yt-dlp-ui-sample.txt
   ```
   The stack will show whether the main thread is parked in (a) child-process IPC, (b) SQLite open, (c) Slint/AppKit init, or (d) something else.
4. Disable the ad-window child-spawn at startup (build flag, or by deleting `Contents/MacOS/ad-window`) and retry. If the main window appears, H1 is confirmed.
5. Acceptance Criteria 4–6 (codesign / spctl / stapler / Developer ID + notarization) remain in scope regardless of which hypothesis wins — macOS 26.x will continue to escalate enforcement and the project commits to signed artifacts as MVP scope.

### Raw log location

The full `log show` capture is at `/workspace/log-app.txt` on the sandbox (≈5 MB, 31093 lines). It is **not** committed to the repo — the dev team can read it from there during analysis. If a sanitized excerpt should travel with the use case, copy only the relevant ranges (lines 7270–7360 for the first force-quit, 14220–14360 for the second launch, plus the AppleSystemPolicy lines 7295–7296).

## Investigation notes — `sample` analysis (2026-05-10, second pass)

Reporter ran `sample` against a bouncing instance of the app launched from the mounted DMG. Capture saved to `/workspace/yt-dlp-ui-sample.txt` on the sandbox. Header excerpt:

```
Process:         yt-dlp-ui [5739]
Path:            /Volumes/VOLUME/*/yt-dlp-ui.app/Contents/MacOS/yt-dlp-ui
Code Type:       ARM64
OS Version:      macOS 26.3.1 (25D2128)
Physical footprint:         96K
Physical footprint (peak):  96K
Idle exit:                  untracked
```

Stack (every sample, 8733 of them across 8.7 s at 1 ms intervals):

```
8733 Thread_400237: Main Thread   DispatchQueue_<multiple>
  8733 _dyld_start  (in dyld) + 0  [0x1018f89c0]

Binary images description not available
```

### Conclusive interpretation — H4 supersedes H1/H2/H3

The main thread is parked at **offset 0 of `_dyld_start`** — the kernel's very first instruction into the dynamic linker after exec. The process has allocated 96 KB total (dyld stub only); no shared libraries have loaded, `main()` has not been entered, no Rust code has executed. `Binary images description not available` confirms dyld itself has not progressed past initial setup.

Combined with the earlier `kernel: (AppleSystemPolicy) ASP: Security policy would not allow process` from `log show`, the failure mode is now unambiguous:

**macOS 26.3.1 suspends the process at dyld startup pending an `AppleSystemPolicy` decision; `syspolicyd` returns a deny verdict for the binary; the kernel keeps the process in suspended-at-dyld_start until the user force-quits.** The Gatekeeper "allow unsigned" override in System Settings → Privacy & Security clears Gatekeeper's *assessment* but does **not** bypass `AppleSystemPolicy` enforcement at exec on macOS 26.x — the kernel-level policy gate is independent.

H1 (subprocess-exec denial of `ad-window`), H2 (SQLite / TCC), and H3 (Slint init deadlock) are all moot. The Rust process never runs.

### The fix is in the release pipeline, not the code

Required changes:

1. **Developer ID Application signature** for every executable Mach-O inside the app bundle:
   - `Contents/MacOS/yt-dlp-ui` (parent)
   - `Contents/MacOS/ad-window` (child helper)
   - `Contents/Resources/yt-dlp` (bundled subprocess)
   - `Contents/Resources/ffmpeg` (bundled subprocess, lipo-merged universal on macOS)
   - `Contents/Resources/deno` (if present)
   - Any embedded frameworks / dylibs

   Codesign with `--options runtime` (hardened runtime), `--timestamp`, and the appropriate entitlements file. Sign nested binaries first, then the outer bundle (deep order matters).

2. **Notarize the signed `.app`** via `xcrun notarytool submit --apple-id … --team-id … --wait`. Requires an Apple Developer Program account ($99/yr) and an App Store Connect API key stored in GitHub Actions Secrets.

3. **Staple the notarization ticket** to the `.app` (and to the `.dmg`) via `xcrun stapler staple`. This embeds the ticket so first launch does not need to hit Apple's servers.

4. **Sign and notarize the `.dmg`** itself (separate `codesign` + `notarytool submit` + `stapler staple` pass).

Required new GitHub Actions infrastructure (per the cargo-dist workflow already present in `dist-workspace.toml`):

- Repository secrets: `APPLE_ID_USERNAME`, `APPLE_TEAM_ID`, `APP_STORE_CONNECT_API_KEY_ID`, `APP_STORE_CONNECT_API_KEY_ISSUER_ID`, `APP_STORE_CONNECT_API_KEY_P8` (base64-encoded `.p8`), `MACOS_CERTIFICATE` (base64-encoded `.p12` Developer ID Application cert), `MACOS_CERTIFICATE_PASSWORD`, `MACOS_KEYCHAIN_PASSWORD`.
- Workflow step that imports the cert into a temporary keychain, runs `codesign` on the produced bundle, submits to `notarytool`, polls until accepted, staples, and packages the final `.dmg`.
- An ADR (`docs/adr/0011-macos-codesigning-and-notarization.md` or similar) capturing the decision, the secret inventory, and the workflow shape.

### Acceptance Criteria 1 ("clearer asset naming") is now lower priority

The user downloaded the correct-architecture `.dmg` (`Code Type: ARM64` confirms the binary is arm64, on arm64 hardware) — install UX confusion is **not** what's blocking them. Criterion 1 is still worth doing as a UX polish (and worth doing if Intel support is being dropped), but it is no longer load-bearing for the bug fix. The signing/notarization work in criteria 4–6 is the entire critical path.

### Acceptance Criterion 7 (in-app crash log) is now moot for this bug

The Rust process never runs, so `tracing` cannot log anything. Criterion 7 remains valid as a general-purpose hardening for *future* startup failures, but it does not apply to the macOS-26-AMFI case. Note this when implementing.

### Raw sample location

The full sample dump is at `/workspace/yt-dlp-ui-sample.txt` on the sandbox (32 lines, 949 bytes). Not committed.
