# Use Case 28: Bundle ffprobe (and verify ffmpeg) for audio-only post-processing

## Summary

UC 17 bundled `ffmpeg` (LGPL-only static build) into all three OS installers and pinned the explicit follow-up: *"ffprobe is not bundled by default. If during analysis or QA yt-dlp errors on a missing ffprobe, the developer adds ffprobe in the same patch using the identical fetch / verify / path-resolver protocol as ffmpeg."* That trigger has now fired — an audio-only download on macOS 26.3.1 (arm64) produced:

```
WARNING: CzG5E2NiZO8: writing DASH m4a. Only some players support this container.
         Install ffmpeg to fix this automatically.
ERROR: Postprocessing: ffprobe and ffmpeg not found.
       Please install or provide the path using --ffmpeg-location
```

yt-dlp's `"ffprobe and ffmpeg not found"` text fires when **either** is missing — they're treated as a pair by `FFmpegPostProcessor` — so the error alone does not tell us whether ffmpeg is also unfound or only ffprobe is. The accompanying DASH-remux warning ("Install ffmpeg to fix…") is a separate code path that specifically signals yt-dlp couldn't find ffmpeg either, raising the possibility that the macOS arm64 build is either not staging ffmpeg at all, staging it at a path the runtime resolver doesn't return, or that `--ffmpeg-location` isn't being passed correctly on the audio-only path.

This UC therefore (1) verifies ffmpeg's actual presence at runtime on macOS as the first diagnostic step, and (2) lands ffprobe-bundling across all three OSes for parity regardless of the (A) only-ffprobe vs (B) both-missing outcome — the fetch scripts on Linux and Windows currently extract only `ffmpeg` from BtbN archives that already contain `ffprobe`, and `build-ffmpeg-macos.sh` builds ffprobe alongside ffmpeg but doesn't stage it. The work mirrors UC 17 line-for-line: extend the three fetch/build scripts, add `bundled_ffprobe_path()`, thread the path into `yt-dlp-bridge`, extend the bats / PowerShell test fixtures, update `THREATS.md` § T1/T13 and the About-dialog (UC 18) bundled-software list. Code signing on macOS now applies to two binaries — connects to UC 26 signing work.

## Acceptance Criteria

1. **Diagnostic step (must run first during analysis):** inspection of the reporter's `/Applications/yt-dlp-ui.app/Contents/Resources/` confirms whether `ffmpeg` is or is not present at the canonical bundled path, and (via `file <path>` and `lipo -info <path>`) whether it is the correct architecture. The result decides whether this UC also includes a fix to the **ffmpeg** staging path on macOS in addition to the ffprobe bundling work.
2. `ffprobe` is present in every per-OS installer at the same canonical path pattern as `ffmpeg`: Linux `/opt/yt-dlp-ui/ffprobe`, macOS `yt-dlp-ui.app/Contents/Resources/ffprobe` (lipo-merged universal), Windows `<InstallDir>\ffprobe.exe`.
3. `ffprobe` is fetched / built / staged using the **identical** SHA256 verification, source pinning, and license-bookkeeping protocol as `ffmpeg`: BtbN LGPL builds for Linux + Windows (extracting both `ffmpeg` and `ffprobe` from the same archive that's already downloaded); built from upstream FFmpeg source on the GHA macOS runners by `build-ffmpeg-macos.sh` (ffprobe is produced by the same `make` already running, so the change is staging-only).
4. `crates/app/src/paths.rs` exposes `bundled_ffprobe_path() -> Result<PathBuf, PathError>` mirroring `bundled_ffmpeg_path()` exactly — same `cfg`-gated per-OS resolution, same error type.
5. `yt-dlp-bridge` passes ffprobe's location to yt-dlp on every spawn — either via `--ffmpeg-location <dir>` pointing at the directory holding both binaries (yt-dlp discovers ffprobe in the same dir automatically), or via an explicit additional flag if behavior testing shows directory-discovery is unreliable on any platform. The spawn invocation is confirmed to apply equally to audio-only, video-only, and audio+video paths — not just one of them.
6. An audio-only download on macOS 26.3.1 (arm64) succeeds end-to-end without the "ffprobe and ffmpeg not found" error and without the "writing DASH m4a" warning. Produces a clean audio file at the configured download destination.
7. Audio-only downloads succeed on Linux (.deb / .rpm / AppImage) and Windows (.msi / NSIS .exe) installers as a **release gate** for this UC — confirming the parity fix and surfacing any platform-specific staging or path-resolver bugs that have been latent.
8. `scripts/fetch-ffmpeg.sh` and `scripts/fetch-ffmpeg.ps1` both extract and stage `ffprobe` alongside `ffmpeg` from the same BtbN archive (no second download). The candidate-path search loop is extended for `ffprobe` and `ffprobe.exe`.
9. `scripts/build-ffmpeg-macos.sh` copies the built `ffprobe` binary alongside `ffmpeg` to its `OUTPUT_DIR`, with the existing configure-line lint extended to cover ffprobe's `--version` output too.
10. `scripts/tests/` (bats + PowerShell) gain ffprobe-presence and SHA-verification coverage mirroring the existing ffmpeg tests.
11. `THREATS.md` § T1 (bundled-binary supply chain) and § T13 are updated so ffprobe appears explicitly in the bundled-binary inventory and trust posture.
12. The About dialog (UC 18) bundled-software / license list adds ffprobe alongside ffmpeg pointing at the same LGPL-2.1+ text — no separate license file (ffprobe ships under the same FFmpeg distribution).
13. On macOS, the bundle's codesign / Mach-O inventory now covers both `Contents/Resources/ffmpeg` and `Contents/Resources/ffprobe`. Connects to UC 26's signing work; if UC 26 lands first, the codesign step already iterates nested Mach-Os and only needs the additional file to exist.
14. Bundle-size impact is measured and recorded. Adding ffprobe is likely ~25–35 MB additional per installer. If any per-OS installer exceeds the 100 MB compressed ceiling from PROJECT_BRIEF.md § Performance budgets, the existing revert-clause stays in force.

## Potential Pitfalls & Open Questions

- **Ambiguity (resolved into a diagnostic step)** — The "ffprobe and ffmpeg not found" error alone is insufficient to know which binary is actually missing. The combined warning + error suggests ffmpeg may also be unfound on macOS. Criterion 1 makes this an explicit pre-fix diagnostic so the dev team doesn't fix ffprobe in isolation only to discover ffmpeg was also broken.
- **Risk** — "Only on macOS" framing is likely observation bias. The fetch scripts for Linux + Windows only stage `ffmpeg`, so Linux + Windows audio-only flows should error in the same way unless `ffprobe` happens to be on the user's PATH from a system install. Criterion 7 enforces explicit QA across all three OSes.
- **Risk** — macOS code signing (UC 26 territory). Adding a second nested Mach-O multiplies the signing surface by one. If UC 26 has not introduced recursive deep-signing yet by the time this UC lands, this UC must, otherwise the new ffprobe binary trips the same `AppleSystemPolicy` enforcement that is keeping UC 26 alive.
- **Edge case** — Some yt-dlp post-processing paths invoke ffprobe directly without ffmpeg for metadata probing. Bundling ffprobe must work whether or not ffmpeg is invoked in the same operation — don't accidentally couple their lifetimes.
- **Edge case** — macOS `build-ffmpeg-macos.sh` configure flags. The script should NOT disable ffprobe (`--disable-ffprobe`). Quick `grep` shows no such flag today, but the configure line should be sanity-checked during analyst review.
- **Edge case** — macOS lipo-merged universal binary. UC 17's macOS path produces a universal ffmpeg via `lipo`. ffprobe staging must lipo-merge arm64 + x86_64 ffprobe variants symmetrically.
- **Edge case** — Dev workflow. UC 03 set up auto-bundle yt-dlp for dev; the ffmpeg/ffprobe dev-mode equivalent needs checking. If `cargo run` paths fetch ffmpeg via the same script, ffprobe is picked up for free; if dev mode has a separate path, it needs the same staging.

## Original Description

I got an error only in mac about ffmpeg and ffprobe not being present. I'm unsure if it is really both are missing or only one. It happened downloading audio-only.

(yt-dlp error captured during clarification round:)

```
yt-dlp existed with error (code:Some(1)):
WARNING: CzG5E2NiZO8: writing DASH m4a. Only some players support this container.
         Install ffmpeg to fix this automatically.
ERROR: Postprocessing: ffprobe and ffmpeg not found.
       Please install or provide the path using --ffmpeg-location
```

## Clarifications

- Q: Do you have the exact error text yt-dlp produced?
  A: Yes — pasted above under Original Description.
- Q: Should ffprobe be bundled on all three OSes, or scoped to macOS only?
  A: All three OSes for parity.
- Q: Should QA explicitly exercise audio-only on Linux and Windows as part of this UC?
  A: Yes — audio-only on all three OSes is a release gate.
