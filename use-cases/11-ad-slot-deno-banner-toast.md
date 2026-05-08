# Use Case 11: Ad slot, deno banner, and Toast component

## Summary

Three independent UI surfaces consumed across the main window. (1) A 64-px-tall bottom-strip ad slot pinned below the footer, with a vertical "AD" label, a dashed-bordered diagonal-stripe placeholder, and a "Focus" button that flips `focus_mode` on. The actual `ad-window` helper-process integration is out of scope; this UC ships the visual region as a placeholder. (2) A dismissible deno-missing banner under the add bar in `warning-soft` palette with an inline mono `brew install deno` snippet, shown only when the UC 05 deno probe failed. Banner dismissal is session-only — reappears next launch if deno is still missing. (3) A reusable Toast component in UC 07's `components.slint` with three variants (info / warning / danger), stack-vertical layout (max 3 visible at once), 3-second auto-dismiss, and the design's 200-ms enter animation. UC 11 wires the Toast to the existing Cancel-all and add-failure handlers; future UCs reuse the same component. The new ad-slot palette colors (`ad-slot-bg`, `ad-slot-stripe-a`, `ad-slot-stripe-b`) are added to UC 07's `DesignTokens` global rather than left as one-off literals.

## Acceptance Criteria

1. **Ad slot** is a 64-px-tall region at the bottom of the main window, below the footer, hidden when `focus_mode` is on (UC 09's KV key).
2. Background uses the new `DesignTokens.ad-slot-bg` property (light: `#eaeaee`, dark: `#0d0d0f`); 1-px `border` top edge.
3. Layout: vertical "AD" label on the left (rotated, 9 px JetBrains Mono uppercase, `text-3`, 0.06 letter-spacing); a 48-px-tall flex-grow placeholder rectangle in the middle with dashed `border-strong` border, 4 px radius, and a diagonal-stripe background using new `DesignTokens.ad-slot-stripe-a` and `ad-slot-stripe-b` properties (light: `#d8d8de` / `#d2d2d8`, dark: `#1a1a1d` / `#18181b`); centered mono text "ad slot · WebView render area · 728×48" in `text-3` 11 px; a "Focus" small ghost button on the right (eye icon + "Focus" label) that flips `focus_mode` to `true` when clicked.
4. **DesignTokens extended** — UC 07's `tokens.slint` adds `ad-slot-bg`, `ad-slot-stripe-a`, `ad-slot-stripe-b` properties with light and dark variants per AC#2 and AC#3. The hex literals appear only inside `tokens.slint`; consumer markup reads only from the global.
5. **Deno banner** is a thin strip below the add bar, shown only when the UC 05 deno probe found neither bundled deno nor PATH deno.
6. Padding 7 × 14, `warning-soft` background, `warning-text` foreground, 1-px `border` bottom edge, 11.5 px font.
7. Layout: info icon, message "Some YouTube downloads may require Deno. `brew install deno` (or platform equivalent).", small `×` ghost button on the right.
8. The `brew install deno` portion is rendered in JetBrains Mono with a `rgba(0,0,0,0.06)` background, 1 × 5 px padding, 3 px radius.
9. **Banner dismissal**: clicking `×` hides the banner for the rest of the current session. State is held in memory only; the banner reappears on next launch if the deno probe still fails. No new KV key is added.
10. **Toast component** is added to `crates/app/ui/design/components.slint` (UC 07's primitives folder) for reuse by UC 02, UC 11 itself, and any future UC.
11. The Toast component exposes a `kind` enum property with three variants:
    - `info` (default) — `text` background, `bg` foreground, matching the design's neutral toast.
    - `warning` — `warning-text` foreground on a `warning-soft` background.
    - `danger` — `danger-text` foreground on a `danger-soft` background.
12. Toast layout: absolute-positioned, bottom 80 px from the window edge (above the ad slot), centered horizontally, 8 × 14 padding, 6 px radius, 12 px / 500 weight, `shadow-md`.
13. Toast entrance animation: 200 ms cubic ease-in, opacity 0→1 + translateY(+8 → 0). Exit animation mirrors entrance.
14. Toast auto-dismisses after 3 seconds.
15. **Toast queueing** — toasts stack vertically, with the newest at the bottom, max 3 visible at once. If a 4th toast fires while 3 are visible, the oldest is dismissed immediately to make room. Each toast tracks its own 3-second timer independently.
16. **Toast trigger — Cancel-all**: when the existing `Cancel all` footer button (UC 01 / UC 08) is clicked, an `info`-kind toast displays "Queue cancelled." Wiring is added to the existing handler.
17. **Toast trigger — Add-failure**: when the URL-add path (UC 01) reports an error (one or more URLs failed to add), a `danger`-kind toast displays "Failed to add URL(s)."
18. No `Settings saved.` toast — the settings panel persists silently per UC 09 AC 17.
19. **Theme correctness** — every visual element uses UC 07's `DesignTokens` (including the three new ad-slot properties). All three surfaces re-theme correctly when `dark-mode` flips.
20. **Z-order** — Toasts render above the ad slot but below any open settings panel (UC 09) or modal (UC 10). Document the layering convention in a comment near the overlay region.
21. Tests — unit / model tests for: banner-visibility logic (deno-found vs. not), banner-session-dismissal state, toast queueing (≤ 3 visible, oldest evicted on overflow), ad-slot visibility vs. focus_mode. UC 01, 05, 07, 08, 09 tests still pass. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` pass.

## Potential Pitfalls & Open Questions

- **Risk** — Slint absolute-positioned overlays competing with UC 09's panel and UC 10's modal. Z-order via Slint's draw order must be consistent: ad slot < toasts < panel/modal < toast-on-modal? AC#20 picks "panel and modal beat toasts"; revisit if a use case needs a toast on top of a modal.
- **Risk** — Diagonal-stripe background in Slint. `repeating-linear-gradient` is CSS-only. Implementation options: a tiled `Image` of one stripe period (a 17-px bitmap), a `Path` with parallel lines, or a Slint shader if the rendering backend supports it. Analyst picks during proposal.
- **Risk** — Vertical "AD" label using `writing-mode: vertical-rl` + transform rotate. Slint supports `rotation-angle` on elements, but text + layout interaction may need a fixed-size container so layout doesn't shift.
- **Risk** — Stack-vertical toast layout with independent timers requires a small state machine (active toasts list, each with its own timer). Slint can model this with a model + animation per element. Confirm `Timer` API in Slint 1.16.1 is up to the task.
- **Edge case** — The brief's `ad-window` helper-process integration is its own deferred concern. UC 11 explicitly does NOT spawn the helper. When the helper is later wired up, replace the placeholder region's contents with the helper's WebView render but keep the surrounding chrome and the Focus button.
- **Edge case** — If the deno probe transitions between "found" and "not found" mid-session (highly unlikely), the banner state could go stale. Acceptable: probe is one-shot at startup, banner state inherits.
- **Edge case** — A 4th toast eviction while the user is reading the oldest: brief flash of the oldest disappearing. Acceptable; alternative (pause oldest if user is hovering it) is overkill for MVP.
- **Risk** — Slint global hot-swap caveat from UC 07 carries over: bind the new ad-slot properties directly to the global so theme-toggle re-themes the slot live.

## Original Description

> **Use case: Ad slot, deno banner, and reusable Toast.**
>
> Source of truth: `<TARGET_DIR>/design/project/app.jsx` lines 237-267 (banner), 331-388 (ad slot), 446-468 (Toast).
>
> Three independent surfaces, all bottom-of-window or transient:
>
> **1. Bottom-strip ad slot** — a 64-px-tall region pinned to the bottom of the main window, below the footer. Hidden when `focus_mode` is on (the UC 09 setting). Contents: a vertical "AD" label on the left (rotated, 9-px mono uppercase, `text-3`), a 48-px-tall placeholder rectangle in the middle with dashed `border-strong` border and diagonal-stripe background (a different stripe color per theme), centered mono text "ad slot · WebView render area · 728×48", and a "Focus" `Button` (icon + label) on the right that flips `focus_mode` to true. Background color: `#0d0d0f` in dark theme, `#eaeaee` in light. The actual `ad-window` helper-process integration is OUT of scope; this UC ships the visual region as a placeholder. When a real ad vendor is wired up later, this region's contents will be replaced with the helper's WebView render.
>
> **2. Deno-missing banner** — a thin warning strip below the add bar, shown only when the deno probe at startup (UC 05) found neither bundled deno nor PATH-deno. Padding 7×14, `warning-soft` background, `warning-text` foreground. Layout: info icon, message "Some YouTube downloads may require Deno. `brew install deno` (or platform equivalent).", small × dismiss button. The `brew install deno` is rendered in JetBrains Mono with a subtle dark-tinted background (`rgba(0,0,0,0.06)`). The banner is dismissible: clicking × hides it for the rest of the session. See clarifications for whether dismissal persists across restarts.
>
> **3. Toast component** — a small reusable absolute-positioned notification that appears bottom-center of the main window (above the ad slot), with the design's animation (opacity 0→1 + translateY 8→0 over 200 ms cubic ease-in). Background `text`, foreground `bg`, 8×14 padding, 6 px radius, 12 px / 500 weight. Auto-dismisses after 3 seconds. The Toast component lives in `crates/app/ui/design/components.slint` (UC 07's primitives folder) for reuse by UC 02 and any future UC.
>
> **Toast triggers in this UC**:
> - "Queue cancelled." — fired by the existing UC 01 Cancel-all handler, OR added as a stub if the handler doesn't exist yet (UC 02 will fully wire Cancel-all). UC 11 ships the toast component AND the wiring to fire on the existing Cancel-all path.
> - "Failed to add URL(s)." — fired when add-time fails (some URLs invalid). The UC 01 add path already handles errors; UC 11 wires it to the toast.
> - "Settings saved." — DEBATABLE. The settings panel persists on every change with no Save button (UC 09 AC 17), so a per-change toast might be noisy. Default: don't.
>
> **Toast queueing**: If a second toast fires while the first is still on-screen, options: replace, stack, queue. Settle in clarification.
>
> Out of scope:
> - Ad-window helper-process spawn / IPC / WebView render — those live in the architecture spec but are deferred until an ad vendor is selected.
> - First-launch ad-consent dialog (separate flow per the brief's monetization).
> - Banner triggers other than the deno probe (e.g., update-available banner) — out of scope; the banner mechanism is generic enough to extend.
>
> Dependencies / interactions:
> - Hard-blocked by UC 07 (palette, Button primitive).
> - Touches `crates/app/ui/main_window.slint` to add the bottom regions.
> - Touches `crates/app/ui/design/components.slint` to add the Toast component.
> - Reads `focus_mode` KV key from UC 09 (or directly from settings if UC 09 isn't shipped yet — minor coupling).
> - Reads the deno-probe state from UC 05 (already in `crates/app/src/lib.rs` or similar).

## Clarifications

- Q: Deno banner dismissal persistence?
  A: Session-only. Banner reappears next launch if deno is still missing. Avoids adding a new KV key; user who installed deno won't see the banner again because the probe will then succeed.
- Q: Toast queueing strategy when a second toast fires while the first is on-screen?
  A: Stack vertically, max 3 visible at once. Newest at bottom; if a 4th toast fires while 3 are visible, the oldest is dismissed immediately to make room. Each toast tracks its own 3-second timer independently.
- Q: Ad-slot color literals — extend DesignTokens or keep as one-off literals?
  A: Extend `DesignTokens` with three new properties: `ad-slot-bg`, `ad-slot-stripe-a`, `ad-slot-stripe-b`. Single source of truth; consumer markup never carries hex literals.
- Q: Should the Toast component support a `kind` prop for variant styling (info / warning / danger) now or later?
  A: Add info / warning / danger variants now. Modest scope addition; future UCs that need warning or danger toasts won't need to retrofit.
