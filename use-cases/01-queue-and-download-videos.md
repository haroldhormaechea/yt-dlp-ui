# Use Case 01: Queue and download videos

## Summary

The first user-facing flow for `crates/app`: a desktop UI that lets the user paste one or more video/playlist URLs into a text input, sees each URL appear as a queued row whose title is fetched via `yt-dlp` (single-video URLs use `--get-title`; playlist URLs are expanded with one batched `yt-dlp --flat-playlist --dump-json` call that returns all entries with titles populated), and starts downloads either per-row or via a "Start all queued" batch action. Multi-line paste auto-splits on newlines into N add operations. A Settings panel introduced by this use case exposes a global format preference (Best video / Best audio MP3 / Best audio Opus, default yt-dlp's `bestvideo+bestaudio/best` heuristic) and a download destination directory chooser (default `~/Downloads/yt-dlp-ui/` on macOS/Linux, `%USERPROFILE%\Downloads\yt-dlp-ui\` on Windows). Items, fetched titles, statuses, and partial progress persist in the SQLite store from `define-architecture` and survive app restart. Downloads run as tokio-supervised `yt-dlp` subprocesses through `crates/yt-dlp-bridge`, capped by the user-configurable concurrency limit (default 3, range 1–10). The `ad-window` helper is not exercised in this flow; the UI runs as a single process during this use case.

## Acceptance Criteria

1. The user can paste a URL into a text input on the main window and add it to the queue via Enter or an "Add" button.
2. Multi-line pastes (newline-separated URLs) auto-split into N add operations, one per non-empty line.
3. Single-video URLs produce one row immediately with a placeholder title (e.g. "Fetching…").
4. Playlist URLs are expanded into N rows via a single `yt-dlp --flat-playlist --dump-json` invocation; rows are inserted with titles already populated and no second metadata fetch is required for that path.
5. For single-video URLs, the placeholder title is replaced with the real title via `yt-dlp --get-title`. Failure leaves the row in the queue with an error indicator and a tooltip explaining the failure.
6. The queue persists across app restarts: closing and reopening the app shows the same items with their fetched titles, statuses, and any partial progress.
7. Each row has a "Download" button that starts a `yt-dlp` download for that item; the row's status transitions from `queued` to `in_flight`.
8. A "Start all queued" action starts every row currently in `queued` status, subject to the concurrency cap.
9. The download concurrency cap (default 3, range 1–10) is enforced. Items beyond the cap remain `queued` and are promoted to `in_flight` as earlier ones complete.
10. A Settings panel exposes a global format preference (minimum options: "Best video", "Best audio MP3", "Best audio Opus") and a download destination directory chooser. Changes apply to subsequent downloads only, not to items already `in_flight`.
11. The destination directory defaults to `~/Downloads/yt-dlp-ui/` on macOS/Linux and `%USERPROFILE%\Downloads\yt-dlp-ui\` on Windows. Changing it via Settings persists across restarts.
12. The format default is yt-dlp's `bestvideo+bestaudio/best` heuristic. Changing it via Settings persists across restarts.
13. Adding a URL already in the queue (deduplicated on the source URL after playlist expansion) is rejected with user-visible feedback or no-ops on the second add — never produces duplicate rows.
14. The `ad-window` helper is NOT spawned during this flow; the app runs as a single process.
15. The UI stays responsive (no main-thread blocking) while title fetches and downloads are in flight.

## Potential Pitfalls & Open Questions

- **Edge case** — Removing or cancelling individual queue items is a hard scope item per `PROJECT_BRIEF.md` (Overview § Success criteria) but is NOT covered by this use case. Capture as the immediate next use case so it is not forgotten.
- **Assumption** — The placeholder shown before the single-video title fetch resolves is the literal text "Fetching…". Could equally be a spinner, the bare URL, or both. UX detail for the developer to settle consistently across rows.
- **Risk** — `yt-dlp --flat-playlist --dump-json` returns titles but not richer metadata such as duration, uploader, or thumbnail. If the UI later wants to display these, a second fetch (per row) will be needed. Out of scope for this use case; recorded as a future enhancement signal.
- **Risk** — Non-YouTube playlist semantics vary across yt-dlp extractors. The fallback for sites where `--flat-playlist` does not apply is to treat the URL as a single video and fetch via `--get-title` (covered by AC#5). The dev-team should explicitly test at least one non-YouTube source (e.g. SoundCloud, Bandcamp) for the playlist-vs-single decision branch.
- **Edge case** — Settings changes during in-flight downloads. AC#10 / AC#11 specify "subsequent downloads only", but the dev-team should confirm what happens to items already in `queued` status when the user changes destination or format. Recommended approach: snapshot the relevant Settings values into the queue row at add-time, so each row's behavior is deterministic from creation regardless of later Settings changes.

## Original Description

> The UI that allows the user to add url's, shows which were added (grabbing the name of the video from the youtube page via some web fetch), and offers the possibility of starting/downloading

## Clarifications

- Q: How should yt-dlp-ui fetch the video title for each added URL?
  A: yt-dlp `--get-title` (or `--dump-json --no-playlist`). The user's free-form mention of "web fetch" was clarified to use the bundled yt-dlp binary's metadata extraction, which works across all yt-dlp-supported sites and handles auth walls / age gates / layout changes that direct YouTube scraping would not.
- Q: How should the user trigger downloads in this use case?
  A: Both — per-row Download button AND a "Start all queued" batch action. Architecture wants both eventually; implementing both now avoids rework.
- Q: Is video format / quality selection part of this use case?
  A: Include — global format default in Settings (not per-row). Minimum options: Best video / Best audio MP3 / Best audio Opus.
- Q: How should playlist URLs be handled?
  A: Expand to N rows automatically.
- Q: Single use case or split?
  A: Keep as one use case. Pieces are tightly coupled (Settings feeds format into downloads; playlist expansion feeds the queue UI; persistence underpins all of it). Dev-team will plan internal milestones.
- Q: Bulk paste of newline-separated URLs?
  A: Auto-split on newlines.
- Q: Playlist title fetching strategy?
  A: Single `yt-dlp --flat-playlist --dump-json` call when expanding the playlist — one Python startup cost regardless of playlist size. Rows are inserted with titles already populated.
- Q: Should Settings also expose the download destination directory?
  A: Yes — destination chooser in Settings, alongside the format preference.
