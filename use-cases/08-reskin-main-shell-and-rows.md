# Use Case 08: Re-skin main shell and queue rows

## Summary

Re-skin the main window shell (add bar, queue list container, footer) and the per-row layout to match `design/project/app.jsx` and `design/project/queue-row.jsx`, consuming the `DesignTokens` global and primitives shipped in UC 07. Each of the seven row states (`queued`, `in_flight`, `done`, `cancelled`, `error`, `waiting_on_user`, `cancelling`) renders with the correct badge, row treatment, sub-row, and state-dependent action buttons. Thumbnails render as deterministic gradient placeholders (with a source-icon glyph for YouTube / Vimeo / etc.) immediately on row add, and a background fetcher then downloads the real thumbnail via the yt-dlp bridge and swaps the gradient out when ready, with a small on-disk cache keyed by the row's URL. Action buttons emit named Slint signals (`start-clicked(id)`, `cancel-clicked(id)`, `remove-clicked(id)`, `restart-clicked(id)`) — UC 02 will wire them to real `DownloadManager` methods. The footer carries `Start all queued`, `Cancel all`, and a mono counts strip; the empty state matches the design. The `cancelling` transient state is rendered visually but never set in code (UC 02 introduces the transition). UC 01 functionality keeps working unchanged.

## Acceptance Criteria

1. **Add bar** matches `app.jsx` (~lines 187-235): link-icon prefix, placeholder copy "Paste a video or playlist URL — multiple lines supported", violet primary "Add" button (UC 07 `Button` primary), theme toggle (UC 07), gear icon button reserved for UC 09 (no-op handler permitted).
2. **Queue list** is a vertically scrolling region with surface treatment and 1-px divider between rows from UC 07's palette.
3. **Each row** renders with thumbnail block at left (104×58), flexible content column (title + badge inline, mono URL, optional sub-row), action column on right. Padding 14×16, gap 14, top-aligned per `queue-row.jsx`.
4. **Title row** shows the title (or italic "Fetching…" in `text-3` when `fetching=true`), the `Badge` from UC 07 with the correct status variant, and a small alert icon prefix on `error` rows.
5. **URL row** uses JetBrains Mono in `text-3`, single-line ellipsized, raw URL on tooltip.
6. **Progress sub-row** appears for `in_flight`, `cancelled`, `cancelling`. 4-px track in `surface-3`; fill `accent` for `in_flight`, dimmed for `cancelled`, `danger` for error states. Below the bar, a mono row shows `XX.X%` (bold for `in_flight`), `<downloaded> / <size>`, `<speed>` (in_flight only), `ETA <eta>` (in_flight only); for `cancelled`, `XX.X%` + italic `stopped`.
7. **Error variant** renders a `danger-soft` background block with `danger-text` foreground containing the error message.
8. **Waiting-on-user variant** renders a `warning-text` mono line "YouTube wants cookies — see dialog above." with an info icon.
9. **Done variant** renders a mono line with green check icon, file size, and "saved to <path>" using the row's snapshotted `dest_dir` with `$HOME → ~` substitution and middle-ellipsis when longer than ~40 chars.
10. **Cancelled rows** render at 0.62 opacity, title strikethrough in `text-3`.
11. **Row hover** changes background to `surface-2`.
12. **Action buttons by state** match `queue-row.jsx`'s `RowActions`:
    - `queued` → Download (primary, icon=download) + Cancel + Remove (icon-ghost, icon=x)
    - `in_flight` → Cancel (icon=stop) + Remove
    - `cancelling` → a single disabled `Cancelling…` button
    - `cancelled` → Restart (icon=rotate) + Remove
    - `done` / `error` → Remove only
    - `waiting_on_user` → Cancel + Remove
13. **Signal contract** — every button emits a named Slint signal regardless of whether the handler is wired this UC: `start-clicked(id)`, `cancel-clicked(id)`, `remove-clicked(id)`, `restart-clicked(id)`. UC 02 hooks these without renames.
14. **Footer** matches `app.jsx` (~lines 294-329): on the left, `Start all queued` (default `Button`) and `Cancel all` (`Button` danger variant), each disabled when its action would have no effect (`Start all` disabled when no `queued` items; `Cancel all` disabled when no `queued` / `in_flight` / `waiting_on_user` items). On the right, a mono stats strip showing `<active> active`, `<queued> queued`, `<done> done`, `cap <N>`.
15. **Empty state** matches the design: a centered 56×56 `surface-3` card with a download icon, "Queue is empty" in `text-2` 13.5 px 500-weight, then a centered hint paragraph in `text-3` 12 px.
16. **Layout split** — `main_window.slint` keeps the shell; the row component lives in `crates/app/ui/queue_row.slint`; the footer lives in `crates/app/ui/footer.slint`. Imports are explicit; no circular dependencies.
17. **UC 01 behavior preserved** — adding URLs, queue persistence across restart, downloads running, single-process operation, every UC 01 acceptance criterion still passes.
18. **Theme support** — every visual element uses UC 07's `DesignTokens` and re-themes correctly when `dark-mode` flips. No hard-coded color values.
19. **Tests** — model-level tests covering row-state → button-set mapping; rendering smoke for at least one row in each state; UC 01's existing tests still pass; `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` pass.
20. **Thumbnails — placeholder phase**: every row renders a deterministic gradient placeholder immediately on add, computed from a hash of the row's URL (same URL → same placeholder). A source-icon glyph is overlaid in a corner based on hostname.
21. **Thumbnails — background fetch phase**: after row insertion, a tokio task fetches the real thumbnail through the yt-dlp bridge, saves it to `<app-data>/thumbnails/<sha1(url)>.<ext>`, and the row swaps gradient → real image with a brief crossfade. Failure leaves the gradient in place and logs at WARN. Cache eviction is no-op for now (deferred follow-up).
22. **Source-icon glyph** detection: hostname matching for `youtube.com`, `youtu.be`, `vimeo.com`, `soundcloud.com`, `bandcamp.com`, with a generic-globe fallback for everything else.
23. **Shimmer overlay** — for `in_flight` rows, a diagonal-stripe Slint element (Rectangle/Image/Path with a tiled stripe pattern) translates horizontally on a 1.2 s loop, layered over the accent fill. Stripe color follows `accent-soft` so it remains subtle in both light and dark themes.
24. **Cancelling state** — the row layout renders correctly when `status="cancelling"` (disabled `Cancelling…` button + cancelling badge from UC 07), but no code path in this UC sets the status to `cancelling`. UC 02 introduces the transition.

## Potential Pitfalls & Open Questions

- **Missing input** — Real-thumbnail fetch path. Two viable approaches: (a) HTTP fetch on the thumbnail URL returned by `yt-dlp --dump-json` (faster, no per-URL Python startup), or (b) `yt-dlp --write-thumbnail` per row (consistent with rest of bridge, but slower). Analyst chooses during proposal; user wants background fetch but implementation is open.
- **Risk** — Thumbnail aspect ratios. Slot is 104×58 (~16:9). Most YouTube thumbs are 16:9; shorts are 9:16; some Vimeo are 4:3. Pick center-crop vs. letter-box and document.
- **Risk** — Crossfade across theme toggle. If a real thumb is mid-fade when theme flips, placeholder colors shift. Mitigation: pause/restart fade on theme change.
- **Risk** — Slint shimmer performance on the lowest-spec target. The tile-translate approach is faithful at typical viewing distances; confirm 60 fps still holds with N rows in flight.
- **Risk** — Action-button signal alignment with UC 02 (still `pending`). Names are locked here; rename later if mismatch.
- **Risk** — Slint global hot-swap caveats from UC 07 carry over: dynamic properties must bind directly to the global, not via cached values, or theme-toggle leaves stale colors.
- **Edge case** — Long title + badge on narrow window must ellipsize the title and keep the badge right-aligned (never wrap to a second line).
- **Edge case** — Footer counts must bind to the model's count properties so transitions don't flicker frames.
- **Edge case** — Cache directory permissions on Windows handled by `directories::ProjectDirs` (already in tree per UC 01); confirm during proposal.

## Original Description

> **Use case: Re-skin the main shell and queue rows to the Claude Design spec.**
>
> Source of truth lives in `<TARGET_DIR>/design/` (already saved). Specifically `design/project/app.jsx` (add bar, queue list, footer with batch actions and counts), `design/project/queue-row.jsx` (full row layout with thumbnail, title, mono URL, status badge, progress bar, action column), and `design/project/tokens.css` for any styling not already covered by UC 07's `DesignTokens` global.
>
> Scope:
> - **Add bar** — re-skin to the design's spec: link icon prefix, full-width URL input with placeholder copy ("Paste a video or playlist URL — multiple lines supported"), violet "Add" primary button, theme-toggle button (added in UC 07), gear button reserved for UC 09.
> - **Queue list** — restyle every row to match `queue-row.jsx`. Each row is a 14×16 padded flex strip with a 104×58 thumbnail block on the left, a flexible content column in the middle (title + status badge, mono URL, optional progress bar with shimmer-equivalent + speed/ETA mono row, error/done/waiting variants), and an action column on the right with state-dependent buttons.
> - **Row state machine** — implement the seven row states the design depicts: `queued`, `in_flight`, `done`, `cancelled`, `error`, `waiting_on_user`, `cancelling` (transient). Each state has the correct badge color (from UC 07's `Badge`), the correct row treatment (cancelled = 0.62 opacity + line-through title; muted text on done/error), and the correct action buttons.
> - **Action button layout** — buttons must be present per state, but their handlers can be no-ops or call existing UC 01/05 logic where it exists. Concretely: `queued` → Download (primary) + Cancel + Remove (icon-ghost); `in_flight` → Cancel (icon=stop) + Remove; `cancelling` → "Cancelling…" disabled; `cancelled` → Restart (icon=rotate) + Remove; `done` / `error` → Remove only; `waiting_on_user` → Cancel + Remove. The wiring of Cancel / Restart / Remove to real DownloadManager methods is UC 02's job — UC 08 only ensures the buttons exist with correct visual states.
> - **Footer** — restyle to match design: left side has "Start all queued" (primary) + "Cancel all" (danger) buttons, both disabled when no rows match the relevant set; right side shows mono counts ("N active · N queued · N done · cap N").
> - **Empty state** — design's empty state with a 56×56 surface-3 card containing a download icon, "Queue is empty" headline, and a centered hint paragraph.
> - **Hover** — rows highlight to `surface-2` on hover (matches design).
> - **Progress bar** — 4-px height with a 2-px radius track (`surface-3`), animated violet fill for in-flight, danger-red fill for error, dimmed fill for cancelled, shimmer overlay (or static stripes if shimmer is hard in Slint) for in-flight only.
> - **Thumbnails** — open question (see clarifications): generate a deterministic gradient placeholder per row, or use yt-dlp's `--write-thumbnail` to fetch a real thumb. UC 08 needs a decision before implementation.
>
> Out of scope:
> - The actual cancel/remove/restart implementation lives in UC 02 (still `pending`). UC 08 only places the buttons.
> - Settings panel (UC 09), bot-check modal (UC 10), ad slot + deno banner + toasts (UC 11) — none of those.
> - The theme toggle and the underlying palette / primitives (UC 07).
>
> Dependencies / interactions:
> - Hard-blocked by UC 07 (palette + primitives + Badge with seven variants).
> - Touches `crates/app/ui/main_window.slint` extensively; likely splits into `main_window.slint` (shell) + new `queue_row.slint` + new `footer.slint` and similar.
> - Touches `crates/app/src/ui_bridge.rs` for any new property bindings introduced by the re-skin.
> - Coordinates with UC 02 — UC 08 must define button signal names that UC 02 can hook into without renaming.

## Clarifications

- Q: Thumbnail strategy for queue rows?
  A: Hybrid — gradient placeholder rendered immediately on row add (deterministic from URL hash, with source-icon glyph), and a background tokio task fetches the real thumbnail and swaps it in with a crossfade. On-disk cache at `<app-data>/thumbnails/<sha1(url)>.<ext>`. Eviction is no-op for now (deferred).
- Q: Shimmer animation on the in-flight progress bar?
  A: Animated diagonal stripe overlay — Slint Image/Rectangle with a tiled stripe pattern, translate-X animated 0 → tile-width on a 1.2 s loop. No `mix-blend-mode` (Slint has no equivalent) but visually faithful at typical viewing distance. Stripe color follows `accent-soft` so it stays subtle in both themes.
- Q: How should the `cancelling` transient state be supported in this UC, given UC 02 hasn't shipped?
  A: Support visually only. Row layout handles `status="cancelling"` correctly (disabled `Cancelling…` button + cancelling badge), but no code path in this UC sets the status. UC 02 introduces the transition. Keeps this UC tight; status remains an unreachable enum value until UC 02.
- Q: Done-row "saved to" copy — dynamic path or hard-coded?
  A: Dynamic path from the row's snapshotted `dest_dir` (set at add time per UC 01), with `$HOME → ~` substitution and middle-ellipsis when longer than ~40 chars. Honest, matches what's actually there.
