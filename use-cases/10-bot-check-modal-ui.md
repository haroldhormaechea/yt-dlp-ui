# Use Case 10: Bot-check modal UI

## Summary

Replace UC 05's existing bot-check dialog with a centered modal matching `design/project/bot-check-modal.jsx`. The modal is 440 px wide with a dark translucent backdrop, centered in the main window via the same layered-Rectangle approach as UC 09. Header carries a warning-soft shield circle and a two-line message that pluralizes when multiple rows are affected. The browser picker shows one row per detected browser (filtered against `browsers.rs` from UC 05) with abstract gradient glyphs (brand-accurate colors, no trademarked logos) rendered as pure Slint gradients. Footer has a "Remember this choice" checkbox plus Cancel and primary "Use <browser>" buttons. ESC key and backdrop click both invoke the Cancel flow, matching common modal UX. The default selected browser on the first open of a session is the first detected in the canonical order; subsequent opens within the same session pre-select the last browser the user picked. UC 05's detection, retry semantics, multi-row batching, and zero-browsers fallback are all consumed unchanged; this UC swaps the visual layer only.

## Acceptance Criteria

1. Modal centered in the main window at 440 px wide, with `rgba(10,10,15,0.42)` solid backdrop covering the rest of the window. No `backdrop-filter` blur (Slint lacks an equivalent).
2. Entrance animation: 180 ms cubic ease-in, opacity 0→1 + translate(-50%, -46%) → translate(-50%, -50%).
3. Modal panel: 10 px border-radius, `surface` background, 1-px `border`, `shadow-lg`.
4. **Header block** (18 px / 20 px / 14 px padding): a 32×32 `warning-soft` circle with a shield icon (`warning-text` foreground), then the title "YouTube needs cookies to verify you're not a bot." (14.5 px, 600 weight) and a sub-paragraph (12 px, `text-2`, 1.5 line-height) "Pick a browser you're signed into YouTube with. yt-dlp-ui will read its cookies (locally, just once) and retry the download." When affected-row count > 1, append " This applies to <N> queued items." with N bold in `text` color.
5. **Browser picker block** (margin 0 / 20 px / 0 / 20 px): a `surface-2` rounded box with 1-px `border` and 8 px radius, containing one row per detected browser, separated by 1-px `divider`.
6. Each browser row is a clickable button row (9×12 px padding, 11 px gap): a 28-px gradient-glyph circle on the left (radial gradient using the browser's brand color with inset highlight + shadow), the browser name (13 px, 500 weight), and a right-aligned 16×16 radio indicator. Selected row has `accent-soft` background and a 5-px solid `accent` ring; unselected indicator is a 1.5-px `border-strong` ring.
7. The browser list is filtered against `browsers.rs` (UC 05) — only detected browsers appear. The order is Brave > Chrome > Chromium > Edge > Firefox > Opera > Safari > Vivaldi, restricted to detected entries.
8. Browser glyphs are abstract gradient circles rendered in pure Slint (Rectangle/Path with radial-gradient + inset highlight). No bundled bitmaps, no trademarked logos. Brand colors: Brave `#fb542b`, Chrome `#4285f4`, Chromium `#4587f4`, Edge `#0078d4`, Firefox `#ff7139`, Opera `#ff1b2d`, Safari `#1f8df8`, Vivaldi `#ef3939`.
9. **Footer block** (14 px / 20 px padding): a "Remember this choice" checkbox (default unchecked, `text-2` 12 px) on the left, then a flex spacer, then a Cancel button and a primary "Use <browser>" button. The primary button label updates as the selected browser changes.
10. **On `Use <browser>` click**: invoke existing UC 05 logic — if "Remember" is checked, persist `cookies_browser`; then trigger the retry of all `waiting_on_user` rows with `--cookies-from-browser <browser>`.
11. **On Cancel click, ESC key, or backdrop click**: all three invoke the same existing UC 05 cancel logic — batched rows transition to `error` with the existing tooltip ("YouTube blocked this download. Set a Cookies source in Settings to retry.").
12. **Default selected browser**: on the first open of a session, defaults to the first detected browser in the canonical order. On subsequent opens within the same session, defaults to the last browser the user picked (whether or not Remember was checked). The session-default is held in memory only; app restart resets to first-detected.
13. **Zero-browsers case** is unchanged from UC 05 — the modal is never shown; a toast appears instead.
14. **Theme correctness** — every visual element uses UC 07's `DesignTokens`; the modal re-themes correctly when `dark-mode` flips, including backdrop opacity if the design specifies a different value per theme.
15. **Tests** — model-level tests covering: affected-count copy (1 vs N), browser-filter ordering (detected-only list), Remember-checked-vs-unchecked persistence, session-default pre-selection logic. The existing UC 05 tests keep passing. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` pass.
16. Layout matches `bot-check-modal.jsx` 1:1 in dimensions and spacing where Slint allows; minor adjustments for native-looking controls (checkbox, primary button) are acceptable as long as the overall composition is preserved.

## Potential Pitfalls & Open Questions

- **Risk** — UC 05 may currently render the dialog via `slint::PopupWindow`; replacement to a layered overlay may require non-trivial refactor in `bot_check.rs`. Analyst plans the swap.
- **Risk** — Reuse the layered-overlay composition from UC 09's slide-in panel if a shared component exists by then; otherwise duplicate the pattern (keep both consistent).
- **Risk** — ESC + backdrop both wired to Cancel: the analyst must register a key listener at the top-level main window that consumes ESC when the modal is open, so it does not bubble to other key handlers (e.g., closing the settings panel).
- **Risk** — Pure-Slint radial gradients are doable but the inset-highlight + center white dot from `bot-check-modal.jsx` lines 12-24 may need a stacked Rectangle composition. Analyst confirms feasibility during proposal.
- **Edge case** — Brand colors for Chromium / Opera / Vivaldi are not in the design's hard-coded list; the values above (`#4587f4`, `#ff1b2d`, `#ef3939`) are reasonable picks. If the analyst finds more authoritative values, they can substitute and document.
- **Edge case** — Affected-count copy on exactly 2 rows reads fine; never reaches 0 (modal would not be shown).
- **Risk** — Slint global hot-swap caveat from UC 07: bind colors directly to `DesignTokens` so theme-toggle re-themes the modal live.

## Original Description

> **Use case: Replace the bot-check dialog UI with the Claude Design modal.**
>
> Source of truth: `<TARGET_DIR>/design/project/bot-check-modal.jsx`. The modal is centered in the main window (440 px wide), with a dark translucent backdrop. It replaces whatever current bot-check dialog implementation exists in `crates/app/src/bot_check.rs` (UC 05) — the trigger logic, multi-row batching, browser detection, and post-pick retry semantics from UC 05 stay. This UC swaps the visual layer only.
>
> Layout (top to bottom):
> - **Header block** (18 px / 20 px / 14 px padding) — a 32×32 warning-soft circle with a shield icon, then a title "YouTube needs cookies to verify you're not a bot." (14.5 px, 600 weight) and a sub-paragraph (12 px, `text-2`, 1.5 line-height): "Pick a browser you're signed into YouTube with. yt-dlp-ui will read its cookies (locally, just once) and retry the download." When more than one row is affected, append " This applies to <N> queued items." (with the count bolded in `text`).
> - **Browser picker block** (margin 0 20 px) — a 1-px-`border` rounded box (8 px radius), `surface-2` background, with one row per detected browser. Each row is a button: 9×12 px padding, gap 11 px, 28-px gradient glyph circle (radial-gradient with each browser's brand color, inset shadow), browser name (13 px, 500 weight), and a 16×16 radio indicator on the right (5-px ring when selected, 1.5-px ring when not). Selected row has `accent-soft` background.
> - **Footer block** (14 px / 20 px padding) — a "Remember this choice" checkbox (default unchecked) on the left, then a flex spacer, then a Cancel button and a primary "Use <browser>" button. The primary button label updates as the user picks rows.
>
> Behavior, all reused from UC 05:
> - Trigger: a queue row hits `BridgeError::AuthRequired` and the persisted `cookies_browser` setting is `None`. Other rows hitting bot-check while the modal is open are held in `waiting_on_user` state and batched with the same pick.
> - On `Use <browser>`: persist the browser if "Remember" was checked, then retry all `waiting_on_user` rows with `--cookies-from-browser <browser>`.
> - On Cancel: all batched rows go to `error` with the existing UC 05 tooltip ("YouTube blocked this download. Set a Cookies source in Settings to retry.").
> - Backdrop click does NOT dismiss — the design wraps the backdrop's `onClick` to close, but UC 05's behavior is "user MUST act (pick or cancel)". Backdrop click should be a no-op or be replaced with a Cancel-equivalent — settle in clarification.
>
> Browser list:
> - The design hard-codes 5 (Brave / Chrome / Firefox / Safari / Edge). UC 05's detection covers Brave / Chrome / Chromium / Edge / Firefox / Opera / Safari / Vivaldi. The modal should show ONLY the detected browsers (filtered list), with the design's gradient-glyph treatment per detected browser. Each browser glyph color: brand-accurate but rendered as an abstract gradient (no trademarked logo bitmap), which sidesteps trademark concerns.
> - Zero-browsers-detected case: per UC 05, the modal is NOT shown — a toast appears instead. UC 10 inherits this contract; the modal is only built to render when at least one browser is detected.
>
> Animation:
> - 180-ms cubic ease-in entrance: opacity 0→1, translate(-50%, -46%) → translate(-50%, -50%).
> - No backdrop blur (Slint has no equivalent of `backdrop-filter: blur(1px)`); the backdrop opacity is the only effect.
>
> Out of scope:
> - The bot-check detection logic, retry semantics, multi-row batching, browser detection, settings KV interactions — all UC 05 territory; consumed unchanged.
> - Settings panel "Cookies source" dropdown (UC 09).
> - Deno banner (UC 11).
>
> Dependencies / interactions:
> - Hard-blocked by UC 07 (palette + primitives).
> - Soft-blocked by UC 09 (the cookies setting it consumes is read-from / written-to via UC 09's panel; not a hard dep, since UC 05 already wrote `cookies_browser`).
> - Touches `crates/app/src/bot_check.rs` (replaces dialog rendering; logic stays).
> - Touches `crates/app/ui/main_window.slint` to host the modal as a layered overlay (similar to UC 09's panel mechanism).
> - Touches `crates/app/src/ui_bridge.rs` for any new property bindings.

## Clarifications

- Q: ESC key behavior?
  A: ESC = Cancel. Pressing ESC dismisses and triggers the existing UC 05 cancel flow (batched rows → `error`).
- Q: Backdrop click behavior?
  A: Backdrop click = Cancel. Matches the design's `onClick={onClose}` wiring and UC 05's "user must act" contract (clicking IS an act).
- Q: Default selected browser on subsequent opens within the session?
  A: Remember the last browser the user picked (in memory, regardless of Remember checkbox). App restart resets to first-detected.
- Q: Browser glyph rendering?
  A: Pure Slint gradient (Rectangle/Path with radial gradient + inset highlight). No bundled bitmaps; consistent with the rest of the UI.
