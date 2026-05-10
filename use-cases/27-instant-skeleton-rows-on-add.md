# Use Case 27: Instant skeleton rows on Add (optimistic placeholder cards)

## Summary

When the user pastes a URL into the AddBar and clicks Add, the UI currently waits for yt-dlp to fetch the video title (and other metadata) before rendering anything in the queue — producing a multi-second perceived freeze where the app appears to ignore the click. This use case introduces an immediate optimistic-render path: the moment the user adds a URL, an empty placeholder card appears at the bottom of the queue showing the URL (design-system-truncated) in the title slot and skeleton loaders for thumbnail, duration, and other metadata fields. For playlist URLs, a single "Loading playlist…" placeholder appears immediately and is then replaced by N video placeholders the moment yt-dlp's enumeration returns; each video row then independently fills in its own metadata as it arrives. As each metadata fetch returns, the card is progressively filled in-place without re-rendering or reordering. If metadata is still loading after 5 s, the skeleton is swapped for a "Still fetching info…" affordance with a small spinner. If the user clicks Start on a placeholder before metadata resolves, the start intent is queued and the download begins automatically the moment metadata is known. The change is UX-layer — the queue lifecycle, download flow, and `app ↔ yt-dlp-bridge` event contracts for already-resolved rows are unchanged, but the bridge grows two new fast paths (`fetch_metadata(url)` and `enumerate_playlist(url)`) so the optimistic UI has something concrete to await.

## Acceptance Criteria

1. Clicking Add on a single-video URL produces a visible queue row within **<100 ms** of the click (aligned with the existing 16 ms UI-event budget plus a single synchronous SQLite insert).
2. The newly rendered row shows the URL (design-system-truncated if it exceeds the title slot) in the title position; thumbnail, duration, format/quality, and other metadata fields render as design-system skeleton loaders.
3. The card stays in the same position in the queue once metadata fills in — it does not visibly reorder, jump, or pop when the title arrives.
4. Title, thumbnail, duration, and other metadata fields fade in (or replace their skeletons in place) as soon as the corresponding data is available from yt-dlp.
5. If metadata fetch fails (invalid URL, network error, yt-dlp parse error, bot-check trigger, etc.), the card transitions to an error state in place, keeping the URL visible and offering retry / remove actions. It does not silently disappear.
6. The Add interaction itself (clearing the AddBar input, returning focus, dispatching the row) happens immediately on click, not after the metadata fetch completes.
7. Adding multiple URLs in quick succession (paste-spam) renders each card immediately and in input order; metadata fetches run concurrently with a bounded cap (e.g. 3–5) without blocking other adds or the UI thread.
8. The `queue_items` SQLite row is created synchronously at the same moment the placeholder card renders, with `status = queued` and `title = null` (URL used as display fallback). The row is updated when metadata arrives. A crash between add and metadata-fetch does not lose the user's intent.
9. Clicking Add on a playlist URL produces a single "Loading playlist…" placeholder card within **<100 ms**, showing the playlist URL truncated in the title slot.
10. Once yt-dlp's playlist enumeration returns, the single placeholder is replaced by N video skeleton rows in playlist order, each showing its individual URL and skeleton fields. Each then fetches its own metadata independently and fills in per criteria 2–4.
11. If playlist enumeration fails, the single playlist placeholder transitions to an error state in place, keeping the URL visible and offering retry / remove. It does not silently disappear.
12. Clicking **Start** on a placeholder (single video or playlist video) gives immediate visual feedback (button state change to "starting…") and queues the start intent; the download begins the moment metadata is known. Clicking **Remove** on a placeholder cancels its in-flight metadata fetch (mirroring the two-stage cancel semantics introduced for downloads in UC 02) and removes the row.
13. If metadata or playlist enumeration is still loading after **5 s**, the affected skeleton is swapped for a "Still fetching info…" affordance with a small spinner. Applies uniformly to single-video metadata, playlist enumeration, and per-video metadata inside a playlist expansion.
14. The change does not regress UC 01 (queue + download), UC 04 (single-video add), UC 14 (start-all-resume-and-retry), UC 19 (audio-only toggle), or UC 05 (bot-check recovery) — existing event-flow contracts for already-resolved rows are preserved.

## Potential Pitfalls & Open Questions

- **Missing input** — Slint's current data-binding shape for queue rows. If rows are bound to a fully-populated struct, the row model probably needs a `LoadingState` discriminator (`Skeleton { url } | Ready { … } | Error { url, message }` plus a playlist-specific `EnumeratingPlaylist { url }`) so a single row template can render all states without recreation. Dev team needs to inspect `crates/app/ui/` to confirm.
- **Risk** — DB-write ordering. Cleanest path: synchronous SQLite insert on the UI thread before the row appears (the brief's 50 ms cold-DB-open budget says a single insert on an already-open connection is well under that). If insert is moved to a background task, a crash between render and insert loses the intent — bad. Lock this down explicitly during analyst/developer work.
- **Risk** — The bridge crate today is download-oriented. Two new public functions are needed: `fetch_metadata(url) -> Result<VideoMetadata, BridgeError>` (invokes `yt-dlp --dump-single-json --no-download`) and `enumerate_playlist(url) -> Result<Vec<PlaylistEntry>, BridgeError>` (invokes `yt-dlp --flat-playlist --dump-single-json` or similar). Both must be cancellable to satisfy criterion 12. The bridge crate must remain UI-free per the brief.
- **Edge case** — Very large playlists (1000+ items). Expanding the single placeholder into 1000 skeleton rows synchronously could itself stutter the UI. Options: virtualize the queue list, or batch-insert rows in chunks. Worth confirming whether the existing queue UI already virtualizes (UC 15 fixed scrolling on the video list, which suggests at least basic list virtualization may already be in place).
- **Edge case** — Partial playlist enumeration. yt-dlp may return fewer entries than the playlist page advertises (private / deleted / region-blocked items). Show what yt-dlp returns and trust it. Surface a small "(N items, M unavailable)" affordance only if yt-dlp's output exposes that count.
- **Edge case** — Multiple playlist URLs added in rapid succession. Each goes through enumeration → expansion. The expansion order in the queue must match Add order, not enumeration-complete order — otherwise a fast-enumerating second playlist may "leap-frog" a slow-enumerating first one.
- **Edge case** — Bot-check recovery (UC 05) is triggered during metadata fetch. The skeleton row needs to play well with the bot-check modal UI (UC 10) — clarify whether the bot-check modal is per-row or app-global, and whether resolving it retries the metadata fetch in place vs. spawning a new one.
- **Edge case** — Network goes offline between Add and metadata fetch. The metadata fetch fails with a network error and the row goes to error state. Retry should re-attempt the metadata fetch (not the download), which is a new code path distinct from the existing download-retry.

## Original Description

When we add items to download, we should first create the placeholder card with the url instead of title, and everything as a skeleton, instead of just waiting until we get the name from youtube to render that card. That way the app is going to seem faster instead of seemingly freeze or ignore the "add" operation for a few seconds.

## Clarifications

- Q: Are playlist URLs in scope for this use case?
  A: Both single videos and playlists.
- Q: How should the URL be displayed while the title is loading?
  A: Raw URL in the title slot, design-system-truncated if too long.
- Q: What happens if the user clicks Start on a placeholder before metadata resolves?
  A: Queue the start intent; download begins the moment metadata arrives.
- Q: What should happen if metadata is still loading after a long pause (e.g., 10 s)?
  A: After 5 s, swap the skeleton for a "Still fetching info…" affordance with a small spinner.
