# Use Case 02: Cancel, remove, and restart queue items

## Summary

Add cancellation, removal, and restart to the queue UI in `crates/app`. Replace the single-shot SIGKILL in `crates/yt-dlp-bridge/src/download.rs` with a two-stage SIGTERM → 2 s grace → SIGKILL body using the `nix` crate (added as a Unix-conditional dep to the bridge and recorded in `PROJECT_BRIEF.md`'s `## Workspace crate dependency graph`). Per-row Cancel buttons appear on every row whose status is `queued` or `in_flight`; clicking transitions the row to `cancelled` after the bridge confirms termination. Cancelling a row whose `title_status = fetching` also kills the running metadata subprocess immediately, via a new `HashMap<i64, Arc<Notify>>` for metadata cancel-tokens parallel to the existing download cancel-token map. A footer "Cancel all" action cancels every queued/in-flight row at once. Per-row Remove buttons delete the row from DB and UI; queued/in-flight rows are first cancelled then deleted; the row's partial `.part` file is also deleted from disk on Remove, while Cancel alone preserves the file so Restart's `--continue` still works. The partial-file path is captured by the bridge from yt-dlp's `[download] Destination:` stderr line and persisted to a new `partial_file_path` column added in migration 2. Cancelled rows display a separate Restart button that re-queues the row using yt-dlp's `--continue` against the existing `.part` file at the row's snapshotted `dest_dir`. UC 02 reuses UC 01's download cancel-token plumbing; the new pieces are the metadata cancel map, the two-stage kill body, the four UI handlers (Cancel, Cancel-all, Remove, Restart), the `partial_file_path` column + DAO method, and migration 2.

## Acceptance Criteria

1. Each row with `status ∈ {queued, in_flight}` displays a Cancel button.
2. Cancelling a `queued` row removes any pending start-signal and transitions `queued → cancelled` immediately (no subprocess to kill).
3. Cancelling an `in_flight` row sends SIGTERM to the yt-dlp child via `nix::sys::signal::kill` on Unix (or `child.start_kill()` immediate equivalent on Windows), waits up to 2 s, then SIGKILL if still alive. Status flips to `cancelled` after the bridge reports termination.
4. Cancelling a row whose `title_status = fetching` ALSO kills the running metadata subprocess immediately, before transitioning the row to `cancelled`.
5. The single-shot SIGKILL in `crates/yt-dlp-bridge/src/download.rs` is replaced with the two-stage body. The `// TODO(uc-02)` marker is removed.
6. Footer "Cancel all" button cancels every row in `queued` or `in_flight`.
7. Rows in `done`, `cancelled`, or `error` have no Cancel button. Cancel-all does not affect them.
8. Cancelled rows are visually distinguishable from `done` and `error` rows in the UI (greyed-out style or status badge — exact UX is a developer decision).
9. Last-known progress percentage on a cancelled in-flight row is preserved on the row (not zeroed).
10. Each row displays a Remove button. Removing a `queued` or `in_flight` row first cancels it (per AC#3 / AC#4), then deletes the row from `queue_items`, deletes the row's partial `.part` file from disk if `partial_file_path` is set and the file exists, and removes the row from the UI.
11. Removing a `done`, `cancelled`, or `error` row deletes the DB row, deletes the `.part` file from disk if one exists (only relevant for `cancelled` and `error`), and removes the row from the UI. The finished media file of a `done` row is NEVER deleted — only `.part` files are app-owned scaffolding.
12. `crates/yt-dlp-bridge/Cargo.toml` adds `nix` under `[target.'cfg(unix)'.dependencies]` (Unix-only). The brief's `## Workspace crate dependency graph` is amended to record the addition.
13. Cancelled rows display a Restart button (separate from Download — exact placement is a developer decision). Clicking Restart re-queues the row; yt-dlp's `--continue` flag (already part of the existing arg set) resumes from the existing `.part` file at the row's snapshotted `dest_dir`.
14. Cancel alone does NOT delete the `.part` file. It is preserved as yt-dlp resume scaffolding so Restart's `--continue` works.
15. During the `cancelling` transient state (between Cancel-click and bridge-confirms-killed), the row's Cancel, Remove, and Restart buttons are disabled to prevent double-cancel and remove-during-cancel races.
16. UI stays responsive (no main-thread blocking) throughout cancel + remove operations.
17. Migration 2 adds a `partial_file_path TEXT NULL` column to `queue_items`. The bridge captures the partial file path from yt-dlp stderr (matching `[download] Destination: <path>`) and forwards it via the existing event channel; the app persists it via a new `db::queue::update_partial_path` DAO method.
18. Remove on a `done` row deletes the DB row and removes it from the UI but does NOT delete the finished media file (which lives at the row's destination directory under its real name, not under `.part`).

## Potential Pitfalls & Open Questions

- **Risk** — Windows path: `nix` is Unix-only. The Windows fallback for the cancel body is `child.start_kill()`, which calls `TerminateProcess` immediately — equivalent to SIGKILL with no grace period. The two-stage SIGTERM→grace→SIGKILL pattern is therefore Unix-only behavior. Cross-platform behavior must be documented in code comments alongside the cancel implementation.
- **Risk** — yt-dlp on SIGTERM may complete its current chunk before exiting, especially on large multi-segment downloads. The 2-second grace may be tight; user may see longer-than-expected cancel latencies on big files. Worth a brief note in `README.UI.md`.
- **Edge case** — Cancel-all when the queue has thousands of items. The cancel `notify_one` calls are O(n) but cheap; the DB writes for status transitions are also O(n). Keep all status-update DB writes inside a single SQLite transaction.
- **Risk** — Two cancel-token HashMaps in `DownloadManager` (download + metadata). Lock-ordering risk if a single Cancel needs to read both. Mitigation: Cancel checks the metadata-map first; if present, fires that notify and returns; otherwise falls through to the download-map. No nested locking.
- **Risk** — The partial-file path is captured by parsing yt-dlp stderr (`[download] Destination: …`). The message format is stable but version-sensitive — a future yt-dlp change could break the capture, in which case `partial_file_path` stays NULL and partial-file deletion silently no-ops (the file leaks but nothing else breaks). Worth a parser unit test pinned against the current format.
- **Edge case** — Remove during the `cancelling` transient must wait for the cancel to confirm before deleting the `.part` file (otherwise yt-dlp may still be writing to it). AC#15's button-disable handles this UI-side; the manager-side flow must enforce the same ordering — Remove on a `cancelling` row queues until the row reaches `cancelled`, then proceeds.
- **Assumption** — Restart button label is literally "Restart". Could equally be "Retry"; developer's call.
- **Edge case** — Cancel-all confirmation dialog: not in scope for this UC. If user feedback later requests it, add as a follow-up.
- **Assumption** — yt-dlp's `--continue` only resumes from `.part` files at the SAME destination directory. The row's snapshotted `dest_dir` (set at add-time per UC 01) is used on Restart, so resume works against the original dest even if Settings has changed since. Worth a code comment.

## Original Description

> Add cancellation to the queue. Two-axis scope:
>
> 1. Per-item cancel — every row in the list, regardless of whether it's currently queued or in-flight, must have a Cancel button. Clicking it stops the download for that row (or removes it from the queue if it never started) and transitions the row to the 'cancelled' status. The brief's success criteria call this out as a hard requirement.
>
> 2. Whole-queue cancel — a single 'Cancel all' button in the footer (next to the existing 'Start all queued' button) that cancels every in-flight or queued row in one go. Items already in 'done' or 'error' state are not affected.
>
> Implementation must replace the single-shot SIGKILL currently in crates/yt-dlp-bridge/src/download.rs with the two-stage cancel behavior the brief mandates in Architecture § Cancellation: send SIGTERM to the yt-dlp child, wait up to 2 seconds for it to exit cleanly, then SIGKILL if it's still alive. UC 01 left a // TODO(uc-02) marker at the call site for exactly this. The two-stage path requires adding either nix or libc to crates/yt-dlp-bridge dependencies — neither is present today, so this is a brief amendment to § Workspace crate dependency graph.
>
> The cancel-token plumbing is already wired by UC 01: DownloadManager keeps a HashMap<i64, Arc<Notify>> per row, and download::start uses tokio::select! over cancel.notified() and the child's exit. UC 02 only needs to (a) call cancel.notify_one() from new UI handlers, (b) implement the two-stage kill body, and (c) ensure the row's status flips to 'cancelled' (not 'error' or 'done') when cancellation completes.
>
> Rows in 'cancelled' status should be visually distinguishable from completed and errored rows (greyed out, or a small 'Cancelled' badge — UX detail for the dev-team to decide). Partial .part files left on disk by yt-dlp on SIGTERM are NOT cleaned up by the app — they're yt-dlp's resume scaffolding, and the next 'Download' click on the same row should benefit from yt-dlp's --continue. This matches the R2 resume strategy chosen in UC 01.

## Clarifications

- Q: Does UC 02 include REMOVING rows from the queue, or only cancelling?
  A: Cancel + Remove. Per-row Remove button on every row; queued/in-flight rows are cancelled first, then deleted from DB and UI.
- Q: Bridge dep choice for SIGTERM (Unix path)?
  A: `nix`. Idiomatic Rust Unix wrapper; clean ergonomics; cargo-deny clean. Added as a Unix-conditional dep so Windows builds don't pull it.
- Q: Cancelling a row whose `title_status = fetching` (metadata subprocess running) — what should happen?
  A: Cancel both — kill the metadata subprocess immediately. Add a second `HashMap<i64, Arc<Notify>>` for metadata fetches in `DownloadManager`. More responsive UX.
- Q: Re-enabling Download on a cancelled row — same Download button or a separate Restart action?
  A: Separate Restart / Retry button. Cancelled rows display a Restart button instead of Download. Semantically clearer.
- Q: Partial `.part` file deletion policy.
  A: Cancel preserves the `.part` file (so Restart's `--continue` works); Remove deletes both the DB row AND the `.part` file from disk (the row is gone, the resume scaffolding is no longer useful). The bridge captures the partial-file path from yt-dlp stderr and persists it to a new `partial_file_path` column added in migration 2.
