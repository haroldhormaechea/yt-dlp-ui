# Use Cases

Status ledger for use cases under `use-cases/`. Machine-maintained — the `define-use-case` skill appends rows; the dev-team orchestrator updates the `Status` and `Updated` columns as it works. Do not hand-edit those two columns unless you know why; edit the use-case file or re-run the skill instead.

Statuses:
- `pending` — saved but not yet picked up by the dev-team
- `in-progress` — the dev-team has started analysis
- `done` — implementation and tests completed
- `blocked` — the dev-team escalated (6-round cap hit, user abort, or infeasibility)

| # | File | Title | Status | Updated |
|---|------|-------|--------|---------|
| 01 | [use-cases/01-queue-and-download-videos.md](use-cases/01-queue-and-download-videos.md) | Queue and download videos | done | 2026-04-26 |
| 02 | [use-cases/02-cancel-remove-and-restart.md](use-cases/02-cancel-remove-and-restart.md) | Cancel, remove, and restart queue items | done | 2026-05-03 |
| 03 | [use-cases/03-bundle-yt-dlp-for-dev.md](use-cases/03-bundle-yt-dlp-for-dev.md) | Auto-bundle yt-dlp for dev workflow | done | 2026-04-26 |
| 04 | [use-cases/04-fix-single-video-add.md](use-cases/04-fix-single-video-add.md) | Fix single-video URL add (PlaylistEntry deserialization) | done | 2026-04-27 |
| 05 | [use-cases/05-youtube-bot-check-recovery.md](use-cases/05-youtube-bot-check-recovery.md) | YouTube bot-check recovery (cookies-from-browser + deno bundling) | done | 2026-04-27 |
| 06 | [use-cases/06-bundle-binaries-in-installers.md](use-cases/06-bundle-binaries-in-installers.md) | Bundle yt-dlp and deno into release installers | done | 2026-04-27 |
| 07 | [use-cases/07-design-system-foundation.md](use-cases/07-design-system-foundation.md) | Design system foundation | done | 2026-04-30 |
| 08 | [use-cases/08-reskin-main-shell-and-rows.md](use-cases/08-reskin-main-shell-and-rows.md) | Re-skin main shell and queue rows | done | 2026-05-01 |
| 09 | [use-cases/09-settings-slide-in-panel.md](use-cases/09-settings-slide-in-panel.md) | Settings slide-in panel | done | 2026-05-03 |
| 10 | [use-cases/10-bot-check-modal-ui.md](use-cases/10-bot-check-modal-ui.md) | Bot-check modal UI | done | 2026-05-03 |
| 11 | [use-cases/11-ad-slot-deno-banner-toast.md](use-cases/11-ad-slot-deno-banner-toast.md) | Ad slot, deno banner, and Toast component | done | 2026-05-03 |
| 12 | [use-cases/12-remove-all-queue-items.md](use-cases/12-remove-all-queue-items.md) | Remove all queue items | done | 2026-05-10 |
| 13 | [use-cases/13-icon-fidelity-fix.md](use-cases/13-icon-fidelity-fix.md) | Icon fidelity fix (sizing + centering, app-wide) | done | 2026-05-06 |
| 14 | [use-cases/14-start-all-resume-and-retry.md](use-cases/14-start-all-resume-and-retry.md) | Broaden Start all to also resume cancelled and retry errored rows | done | 2026-05-08 |
| 15 | [use-cases/15-fix-video-list-scrolling.md](use-cases/15-fix-video-list-scrolling.md) | Fix scrolling on the video list | done | 2026-05-07 |
| 16 | [use-cases/16-fix-download-destination.md](use-cases/16-fix-download-destination.md) | Respect the configured download destination | done | 2026-05-07 |
| 17 | [use-cases/17-merge-audio-and-video-with-ffmpeg.md](use-cases/17-merge-audio-and-video-with-ffmpeg.md) | Bundle ffmpeg to merge YouTube audio + video streams | done | 2026-05-07 |
| 18 | [use-cases/18-about-dialog-version-and-licenses.md](use-cases/18-about-dialog-version-and-licenses.md) | About dialog — version + bundled-software licenses | done | 2026-05-08 |
| 19 | [use-cases/19-audio-only-vs-audio-video-toggle.md](use-cases/19-audio-only-vs-audio-video-toggle.md) | Per-URL audio-only vs. audio+video toggle on AddBar | done | 2026-05-08 |
| 20 | [use-cases/20-extract-to-standalone-repo.md](use-cases/20-extract-to-standalone-repo.md) | Extract fork into a standalone GitHub repo | done | 2026-05-09 |
| 21 | [use-cases/21-fix-fetch-deno-v2-sha256sum-parser.md](use-cases/21-fix-fetch-deno-v2-sha256sum-parser.md) | Fix fetch-deno scripts for deno v2.x .sha256sum format | done | 2026-05-09 |
| 22 | [use-cases/22-fix-linux-release-build.md](use-cases/22-fix-linux-release-build.md) | Fix Linux Release build (gdk-3.0 + WebKit deps in dist-build hook) | done | 2026-05-09 |
| 23 | [use-cases/23-fix-macos-x86_64-release-build.md](use-cases/23-fix-macos-x86_64-release-build.md) | Fix macOS x86_64 Release build (nasm for ffmpeg cross-build) | done | 2026-05-09 |
| 24 | [use-cases/24-fix-windows-release-build.md](use-cases/24-fix-windows-release-build.md) | Fix Windows Release build (modern gpg via winget) | done | 2026-05-09 |
| 25 | [use-cases/25-trim-release-assets-to-installers.md](use-cases/25-trim-release-assets-to-installers.md) | Trim release-pipeline assets to installers + provenance only | pending | 2026-05-10 |
| 26 | [use-cases/26-fix-macos-arm64-launch-failure.md](use-cases/26-fix-macos-arm64-launch-failure.md) | Fix macOS arm64 launch failure (Dock-bounce-and-die on macOS 26.x) | pending | 2026-05-10 |
| 27 | [use-cases/27-instant-skeleton-rows-on-add.md](use-cases/27-instant-skeleton-rows-on-add.md) | Instant skeleton rows on Add (optimistic placeholder cards) | pending | 2026-05-10 |
| 28 | [use-cases/28-bundle-ffprobe-and-verify-ffmpeg.md](use-cases/28-bundle-ffprobe-and-verify-ffmpeg.md) | Bundle ffprobe (and verify ffmpeg) for audio-only post-processing | pending | 2026-05-10 |
| 29 | [use-cases/29-fix-addbar-url-input-clipping.md](use-cases/29-fix-addbar-url-input-clipping.md) | Fix AddBar URL input text clipping | pending | 2026-05-10 |
