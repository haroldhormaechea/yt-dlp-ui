# Use Case 14: Broaden Start all to also resume cancelled and retry errored rows

## Summary

Broaden the existing "Start all queued" footer button so it also resumes `cancelled` rows and retries `error` rows in addition to starting `queued` ones — no separate "Resume all" button. Each row goes through the same per-row start handler used today by Download (queued) and Restart (cancelled / error); yt-dlp's `--continue` covers `.part`-file resume identically to per-row Restart, so `crates/yt-dlp-bridge` doesn't need a new code path. Genuinely-broken errors (deleted video, geoblock) just re-error; the user dismisses those via per-row × beforehand. Active states (`in_flight`, `cancelling`, `waiting_on_user`) are untouched. Concurrency cap is honored — excess rows stay `queued` and promote as slots free. Mid-batch the button is disabled (UC 12-style re-entry gate). The button is renamed to "Start all" and shows a hover tooltip with the resumable breakdown using UC 09's `Tooltip` primitive. No confirmation modal — Start all is non-destructive.

## Acceptance Criteria

1. Footer button in `crates/app/ui/footer.slint` is renamed to "Start all". Existing play icon kept.
2. Enable predicate widens from `queued-count > 0` to `queued-count + cancelled-count + error-count > 0`.
3. Clicking the button iterates every row in `{queued, cancelled, error}` and puts each through the existing per-row start handler.
4. `in_flight`, `cancelling`, `waiting_on_user` rows are NOT touched.
5. `.part`-file resume on `cancelled` rows uses yt-dlp's `--continue` (same as per-row Restart). No new path in `crates/yt-dlp-bridge`.
6. Concurrency cap (default 3) honored — excess rows stay `queued` and promote as slots free.
7. While the batch loop is in flight, the button is disabled (UC 12 mid-flight gate, same disabled-while-batch-op pattern as Cancel all / Remove all).
8. Footer counts strip continues to update live as rows transition.
9. No change to per-row Download / Cancel / Restart / × actions.
10. No change to `queue_items` status enum or DB schema.
11. Hover tooltip on the button uses the existing `Tooltip` primitive from `crates/app/ui/design/components.slint` (UC 09). Text: `"<N> queued, <M> cancelled, <K> error"`. Each segment is omitted when its count is zero (e.g. only queued + error → `"3 queued, 2 error"`). When the button is disabled (no resumable rows), the tooltip is suppressed or reads `"Nothing to start"` — implementer's call, but consistent with the disabled state.
12. Tests:
    - Unit/model: mixed-state queue → only the resumable subset transitions; `in_flight` / `cancelling` / `waiting_on_user` / `done` rows unchanged.
    - Unit: enable predicate matrix (empty / `done`-only / `cancelling`-only / `waiting`-only → `false`; any `queued` / `cancelled` / `error` row present → `true`).
    - Unit: tooltip text rendering — each (N, M, K) zero-vs-nonzero combo produces the right segment-omission output.
    - Integration: cap=2, 5 resumable rows of mixed state → exactly 2 spawn immediately, 3 queue and promote as slots free.
    - `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` all pass.
13. `CONTRIBUTING.UI.md` manual smoke addendum: button label change, enable matrix, mixed-state queue start (verify one .part-file resume + one error retry + one fresh queued), concurrency cap honored, tooltip hover content correct.

## Potential Pitfalls & Open Questions

- **Risk** — Non-retryable errors (deleted video, geoblock) waste a slot and re-error. Accepted; users dismiss via × per row.
- **Edge case** — `cancelled` row whose `.part` file was deleted/moved between sessions. yt-dlp `--continue` falls back to fresh download silently. No special handling.
- **Edge case** — `cancelling` rows are excluded from Start all to avoid racing the cancel pipeline.
- **Open question** — Disabled-state tooltip behavior (suppress vs `"Nothing to start"`). Minor; implementer's call.
- **Risk** — Mass-retry of error rows may produce noisy yt-dlp logs. Acceptable; per-row error UI already surfaces them individually.

## Original Description

> I want a "Resume all" button, alongside "Start all" and "Cancel all". This would be to continue stopped elements or retry failed ones. On a second though we may reuse Start all queued for this purpose. Ideas?
>
> [Direction confirmed in chat]: Yeah, broaden it.

## Clarifications

- Q: What's the final button label?
  A: "Start all" — short, accurate, covers queued/cancelled/error uniformly. Existing play icon kept.
- Q: Should the button show a confirmation modal before firing?
  A: No confirm — non-destructive action; worst case is failed retries. Matches today's Start-all-queued behavior.
- Q: Should the button surface a hover tooltip with the resumable breakdown?
  A: Yes, add tooltip with breakdown using UC 09's `Tooltip` primitive. Text format: `"<N> queued, <M> cancelled, <K> error"`, with each segment omitted when its count is zero.
