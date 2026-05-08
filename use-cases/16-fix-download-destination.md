# Use Case 16: Respect the configured download destination

## Summary
The configured download destination is not respected: on a fresh install (no custom folder ever picked in Settings), files land in the *app working directory* instead of the per-OS default `~/Downloads/yt-dlp-ui/` declared in PROJECT_BRIEF.md § Storage. The most likely root cause is that the `crates/yt-dlp-bridge` argv builder does not pass any `--paths` / `-o` argument to `yt-dlp`, so the binary falls back to its own default (cwd). The fix is to compute the active destination at **download-spawn time** — read `settings.download_dir` from the SQLite KV table, or fall back to the per-OS default via the `directories` crate — and pass it on every `yt-dlp` invocation. Reading at spawn time (not enqueue) means `queued` items pick up subsequent setting changes; `in_flight` items keep their pre-existing destination because `yt-dlp` is already running. Fix surface: `crates/yt-dlp-bridge` argv builder, the `crates/app` queue-manager spawn site that calls into it, and possibly the settings read helper. No DB-schema changes, no new dependencies.

## Acceptance Criteria
1. On a fresh install (no custom destination ever set), queued downloads land in the per-OS default per PROJECT_BRIEF.md § Storage — *not* in the app working directory.
2. Choosing a destination via Settings, then queuing a new download, results in the finished file landing under that folder.
3. The destination persists across app restarts (saved in the SQLite `settings` KV table).
4. The Settings panel re-displays the persisted destination after restart.
5. Mid-queue change: items in `queued` status pick up the new destination when their `yt-dlp` subprocess spawns; items already `in_flight` keep their original destination until they finish.
6. Paths with spaces, Unicode, and emoji are handled correctly (no shell-quoting / argv-passing regression).
7. Per-OS path separators handled correctly (POSIX forward-slash; Windows backslash).
8. If the destination folder does not exist or is not writable at spawn time, the item moves to `error` status with a user-visible message; no silent fall-back, no auto-mkdir.
9. No regression to other UC 09 settings (concurrency cap, ad consent, focus mode).
10. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.
11. No new third-party dependencies.

## Potential Pitfalls & Open Questions
- **Implementation choice** — Which `yt-dlp` argv approach to use: `--paths home:<dir>` (modern, slot-aware) or `-o <template-with-dir>` (older, full template). The analyst should pick during analysis; both are equivalent for this fix.
- **Edge case** — `rfd` may return paths with trailing separators or with `~` not expanded; these need normalisation before persistence and before being passed to `yt-dlp`.
- **Edge case** — Permission / IO errors after spawn (mid-stream write failures) surface through `yt-dlp`'s own error stream and are out of scope for this fix; AC #8 only covers the *spawn-time* check.

## Original Description
The target folder for downloads isn't being respected.

## Clarifications
- Q: Where do files actually land today when you've set a custom destination?
  A: App working directory (cwd) — no destination is being passed at all, so yt-dlp falls back to its own default.
- Q: Have you actually changed the destination in Settings, or never set it at all?
  A: Never changed it; fresh install. The per-OS default isn't being applied either, which means this is more fundamental than a Settings-panel persistence bug.
- Q: Mid-queue destination change — what's the expected behavior?
  A: Apply to queued items too, leave in_flight alone. Implies destination is read at spawn time, not snapshotted at enqueue.
- Q: If the chosen folder doesn't exist or isn't writable at download time, what should happen?
  A: Error — surface to user, do not download. No silent fall-back, no auto-mkdir.
