# Use Case 17: Bundle ffmpeg to merge YouTube audio + video streams

## Summary
yt-dlp downloads YouTube audio and video as separate streams when the requested quality only exists in DASH/adaptive form. Without ffmpeg available at runtime, downloads either produce split files or fail. The fix is to bundle a static **LGPL-only** ffmpeg binary alongside the existing yt-dlp binary, parallel to the bundling pattern from UC 03 (dev workflow) and UC 06 (release installers), and pass `--ffmpeg-location <bundled-path>` to yt-dlp on every spawn. ffprobe is **not** bundled in this use case — only ffmpeg; if yt-dlp surfaces a missing-ffprobe error during analysis or QA, the developer adds ffprobe in the same patch using the same fetch / verify protocol. ffmpeg is sourced from a third-party LGPL-only build (BtbN/FFmpeg-Builds LGPL variants, evermeet.cx for macOS, johnvansickle.com for Linux — concrete source picked during analysis), pinned by version, and SHA256-verified at fetch time. Bundle-size impact is deferred: the analyst lands the LGPL-only binary first, measures the per-OS installer delta, and only revisits the 50 MB budget in PROJECT_BRIEF.md § Performance budgets if the measured size exceeds it. `crates/app/src/paths.rs` extends to expose `bundled_ffmpeg_path()` mirroring `bundled_yt_dlp_path()`. If ffmpeg is missing at spawn time, the item moves to `error` with a user-visible message — no silent fall-back.

## Acceptance Criteria
1. Downloading a YouTube URL that requires merging (e.g. 1080p video + opus audio) produces a single muxed file (mp4 or mkv) with both tracks correctly synced.
2. yt-dlp is invoked with `--ffmpeg-location <bundled-path>` pointing at the bundled ffmpeg on every spawn.
3. The dev workflow (UC 03 parallel) auto-fetches ffmpeg into `runtime-deps/` so `cargo run` works without a system-wide install.
4. Release installers (UC 06 parallel) include ffmpeg at the per-OS bundled-binary path documented in PROJECT_BRIEF.md § Architecture.
5. `crates/app/src/paths.rs` exposes `bundled_ffmpeg_path()` mirroring `bundled_yt_dlp_path()`.
6. License posture documented in a new ADR (`docs/adr/0007-ffmpeg-bundling.md`) and `THREATS.md`; the bundled ffmpeg is LGPL-only (no x264 / x265 / libfdk_aac) and the source / build provenance is recorded.
7. Per-OS bundled binaries are pinned by version (`FFMPEG_VERSION` env var) and SHA256-verified at fetch time, parallel to UC 06's upstream-yt-dlp supply-chain protections.
8. macOS universal-binary parity: ffmpeg is lipo-merged x86_64 + aarch64 to match the app binary policy from UC 06.
9. If ffmpeg is missing at spawn time, the item moves to `error` status with a user-visible message — no silent fall-back, no auto-install.
10. ffprobe is *not* bundled by default. If during analysis or QA yt-dlp errors on a missing ffprobe for a YouTube merge, ffprobe is added to the same patch using the identical fetch / verify / path-resolver protocol as ffmpeg (no separate use case).
11. Bundle-size delta is measured during QA; if it exceeds the 50 MB per-OS installer budget, the use case explicitly records the new target and PROJECT_BRIEF.md § Performance budgets is updated.
12. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.
13. No new third-party Rust crates (Rust-side change is path-resolver + argv-builder only); ffmpeg itself is bundled, not linked.
14. No DRM circumvention beyond what yt-dlp already supports — non-goal restated.

## Potential Pitfalls & Open Questions
- **Implementation choice** — Concrete source per OS:
  - macOS: evermeet.cx LGPL builds (universal or per-arch + lipo) vs. BtbN/FFmpeg-Builds.
  - Linux: johnvansickle.com static LGPL vs. BtbN/FFmpeg-Builds LGPL.
  - Windows: BtbN/FFmpeg-Builds LGPL win64 build.
  - Analyst picks during analysis based on stability, signature availability, and architecture coverage.
- **Edge case** — Format selector tuning: should the default `-f` selector prefer progressive (single-file) formats over DASH when available, to reduce merge frequency? Quality vs. simplicity tradeoff. Out of scope for this UC unless analysis shows merge failures dominate the reported bug rate.
- **Edge case** — Linux distro policies (snap, deb/rpm) sometimes prefer system ffmpeg over bundled binaries. Snap publishing is already a separate workstream (UC 06's recorded scaffolding gap); same caveat applies here without changing scope.
- **Assumption** — Auto-update of ffmpeg is out of scope, parallel to yt-dlp's deferred auto-update per PROJECT_BRIEF.md. Bumping `FFMPEG_VERSION` is a manual PR, like `YT_DLP_VERSION`.

## Original Description
When we download anything from YT, we download audio and video separate. We need to merge them using ffmpeg (which should be a dependency of the project)

## Clarifications
- Q: How should ffmpeg be a dependency — bundled (like yt-dlp), or required from the user's PATH?
  A: Bundled into installer (like yt-dlp). Self-contained app for non-technical users.
- Q: License posture for the bundled ffmpeg — critical for PolyForm Noncommercial compliance?
  A: Use a third-party LGPL-only build (BtbN/FFmpeg-Builds LGPL variant, evermeet.cx, johnvansickle.com). SHA256-pinned. No GPL components.
- Q: Should ffprobe be bundled alongside ffmpeg?
  A: No — only if/when needed. If yt-dlp errors on missing ffprobe during analysis or QA, the developer adds ffprobe in the same patch using the same protocol.
- Q: If the LGPL-only ffmpeg pushes a per-OS installer over the 50 MB budget, what's the right call?
  A: Defer until measured. Try the LGPL-only build first, see actual size, decide then.
