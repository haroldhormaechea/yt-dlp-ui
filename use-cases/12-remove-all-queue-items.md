# Use Case 12: Remove all queue items

## Summary

Add a "Remove all" button to the main-window footer alongside the existing "Cancel all" (UC 01 / UC 08), with a centered confirmation modal that reuses UC 10's layered-Rectangle composition and a danger-palette primary button. Today the only batch action is Cancel all, which transitions in-flight `yt-dlp` children to `cancelled` state but leaves every row in the queue — the user has to delete each row one by one via the per-row × button after a long session. Remove all clears the entire queue in one click after a confirmation that names how many rows are about to be deleted and how many of those are still in flight. The action is destructive and irreversible, so the confirm step is mandatory. In-flight rows are cancelled via the existing per-row cancel pipeline (Unix 2-s `SIGTERM` grace; immediate on Windows) before their DB rows are deleted, so partial `.part` files end up in the same state they would after a normal user-initiated cancel. While either Cancel all or Remove all is mid-flight, both footer buttons are disabled to keep the model simple.

## Acceptance Criteria

1. "Remove all" button added to `crates/app/ui/footer.slint`, next to "Cancel all", same spacing and primitives.
2. Both "Remove all" and "Cancel all" are disabled when the queue is empty (`queued + active + waiting + cancelled + done + error == 0`) AND while either batch op is mid-flight.
3. Clicking "Remove all" opens a centered confirmation modal reusing UC 10's layered-Rectangle composition (no `PopupWindow`).
4. The existing top-level `FocusScope` ESC routing is extended so the confirm modal closes first; precedence is confirm modal > bot-check modal > settings panel.
5. Modal title reads "Remove all queue items?". Body reads "This will remove <N> item(s) from the queue. <K> are still in flight and will be cancelled first. This cannot be undone." The second sentence is omitted when `K == 0`.
6. Modal footer has a ghost Cancel button and a danger-palette primary button labelled "Remove <N>". Cancel button, ESC key, and backdrop click all dismiss the modal without changes.
7. On primary-button click, in-flight rows are cancelled first via the existing per-row cancel pipeline (Unix 2-s `SIGTERM` grace; immediate `start_kill` on Windows), then every row is deleted from the SQLite `queue_items` table inside one transaction, and the Slint queue model is cleared.
8. After completion, the modal closes and an `info`-kind Toast (UC 11) reads "Queue cleared (<N> item(s)).".
9. `BotCheckCoordinator::withdraw(id)` is invoked for every removed `waiting_on_user` row before its DB delete, so no `oneshot::Sender`s leak.
10. The `history` table is NOT touched by Remove all — completed-download history is append-only by contract.
11. Bot-check modal interlock: when UC 10's modal is open, its translucent backdrop already intercepts footer clicks, so Remove all cannot fire while a bot-check is pending. No new guard is needed; this is documented as the contract.
12. Theme correctness — every visual element in the confirm modal binds to UC 07's `DesignTokens`; both themes render correctly when `dark-mode` is toggled mid-confirm.
13. Z-order — the confirm modal stacks above `SettingsPanel` (UC 09) and `BotCheckModal` (UC 10). UC 11's z-order block comment in `main_window.slint` is updated to add the new layer.
14. Tests:
    - Unit/model: removal pipeline empties the queue model regardless of starting row-state mix.
    - Unit: count copy "<N> item(s)" pluralizes for N=1 and N>1; the in-flight call-out renders only when K>0.
    - Unit: footer button enable predicate is `false` for an empty queue and `true` otherwise.
    - Headless smoke: confirm modal mounts via `MainWindow::new()` + property toggle without panic, in both theme polarities.
    - `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` all pass.
15. Manual smoke addendum in `CONTRIBUTING.UI.md` covering: button enable/disable, confirm modal copy with mixed-state queues, ESC + backdrop dismiss, full Remove flow with in-flight rows present, toast feedback, theme flip mid-confirm, and the bot-check-modal interlock contract.

## Potential Pitfalls & Open Questions

- **Risk** — `crates/app/src/download_mgr.rs` may not expose a `remove_all` operation; the analyst will need to add one or compose existing per-row deletes inside a single SQLite transaction. Handler design (one big `DELETE FROM queue_items` vs. per-row delete with per-row `coordinator.withdraw`) is an implementation detail for the analyst.
- **Edge case** — yt-dlp child process mid-`SIGTERM`-grace when its DB row is deleted: the per-task channel may try to write a final state to a row that no longer exists. The existing per-row cancel path may already handle this gracefully; the analyst confirms during proposal.

## Original Description

> Add a "remove all", not only cancel all, or we may get a bunch of downloads pending that have to be removed one by one. Add a confirmation dialog before commiting to remove them
>
> [Skill argument expansion]: Add a "Remove all" footer button alongside the existing "Cancel all" button. Currently only Cancel all exists, which transitions in-flight rows to cancelled state but leaves them visible in the queue. If the user has a large queue (e.g. dozens of cancelled, errored, or done rows), they have to remove each row one by one via the per-row remove action. Remove all should clear the entire queue (or at minimum all non-active rows) in one click, with a confirmation dialog beforehand to prevent accidental destruction. The confirmation dialog should match the visual language established by UC 10 (centered modal, layered Rectangle, ESC + backdrop click = cancel) and reuse those primitives where possible.

## Clarifications

- Q: Scope of "Remove all" — entire queue including in-flights, only non-active, or per-state granular buttons?
  A: Everything, in-flights cancelled first via the existing per-row cancel pipeline. One button, one confirmation, one operation.
- Q: How should the confirmation modal show the impact?
  A: Total count plus an in-flight call-out, with the call-out rendered only when K>0.
- Q: What happens if Remove all is clicked while Cancel all is mid-flight (or vice versa)?
  A: Both footer buttons are disabled while either op is mid-flight. Simplest model, no race conditions.
- Q: What if the bot-check modal (UC 10) is open when Remove all would otherwise fire?
  A: UC 10's translucent backdrop already prevents the click; this is documented as the contract, no new guard or interlock is added.
