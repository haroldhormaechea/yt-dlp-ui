# Use Case 15: Fix scrolling on the video list

## Summary
The main app window's queue (video list) does not scroll at all: no scrollbar is rendered, the mouse wheel has no effect, and rows beyond the viewport are clipped with no way to reach them. Reproduces on any OS once the queue holds ~10 or more rows. Fix is in the Slint UI for the main shell (`crates/app` `.slint` files): the queue container that hosts the row components introduced in UC 08 needs a working vertical scroll wrapper. Whether a `ScrollView` is already present and mis-configured, or absent entirely, is an investigation step in the analyst phase. Scope is presentation-only — no data-model, queue-manager, or `yt-dlp-bridge` changes — and is bounded to the main queue list (Settings panel, modal, ad-banner, and per-row internals are not touched).

## Acceptance Criteria
1. With ~10+ queue rows on any supported OS, the user can scroll the full list using the mouse wheel.
2. A visible vertical scrollbar appears next to the list when content overflows the viewport, can be dragged, and reflects the current scroll position.
3. Up / Down arrow keys scroll the list by one row at a time when focus is in the queue area.
4. Resizing the window so all rows fit removes the scrollbar; shrinking the window restores it without breaking layout mid-resize.
5. Adding a new row while scrolled does not jump the viewport (current scroll position preserved).
6. Removing rows does not leave the list permanently scrolled past content (auto-clamps to the new content height).
7. No regression to: row hover, action buttons, Settings slide-in (UC 09), bot-check modal (UC 10), Toast / ad-banner (UC 11), icon fidelity (UC 13).
8. Queue row dimensions, alignment, and design-system spacing (UC 07/08) remain visually unchanged.
9. No new third-party dependencies — Slint's own scroll primitives are used.
10. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.

## Potential Pitfalls & Open Questions
- **Investigation** — Whether a `ScrollView` already wraps the queue today is unknown; the analyst must check `crates/app/src/ui/*.slint` first. The fix is either a structural insert (no wrapper) or a configuration tweak (wrapper present but mis-sized / mis-bound to the model).
- **Risk** — Per existing project memory, plain `height:` on a Slint layout child is a hint; the wrapper's child likely needs `min-height` / `max-height` plus `stretch: 0` (or equivalent constraint) so the inner content height is actually constrained and the scroll viewport works.
- **Assumption** — Fix lives only in `.slint` files. If the row list is not already a model-backed `for`-iteration, a small Rust-side adapter on the row model may be needed.
- **Edge case** — Page Up / Page Down / Home / End are explicitly out of scope for this fix (arrow keys only) and may be picked up later as an accessibility-pass use case if desired.

## Original Description
The scroll isn't working for the video list.

## Clarifications
- Q: What is the actual failure mode you're seeing on the queue list?
  A: No scroll at all (mouse wheel does nothing, no scrollbar appears, rows past the viewport are clipped).
- Q: Do you know if a ScrollView already wraps the queue list in the .slint files today?
  A: Don't know — let the dev team check during the analyst phase.
- Q: Should keyboard scrolling be in scope for this fix, or is mouse-wheel + scrollbar enough?
  A: Add arrow keys (Up/Down arrow scrolls one row at a time when focus is in the queue).
- Q: What's the reproduction environment / row count?
  A: Any OS, ~10 or more rows.
