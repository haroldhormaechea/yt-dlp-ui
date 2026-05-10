# Use Case 29: Fix AddBar URL input text clipping

## Summary

The URL input field on the AddBar (`crates/app/ui/add_bar.slint` or its named equivalent) clips typed text vertically — only the upper ~2–3 px of each glyph is visible, so ascenders show as a thin horizontal strip and descenders / baselines / round-letter bottoms are entirely hidden. The screenshot (`29-fix-addbar-url-input-clipping.png`, captured by the reporter on macOS in dark theme) shows the typed text reduced to a magenta/violet horizontal sliver inside the input's visible rectangle; the input *container* looks tall enough for full glyphs, so the bug is the text-rendering region inside it being clipped or y-mispositioned, not the outer row being too short. The clipping reproduces on all three OSes (Linux + macOS + Windows), so the cause is in the Slint markup or the design-system token values, not in any per-OS text-rendering subsystem. Other text inputs in the app (Settings panel, etc.) are unaffected, which localizes the regression to the AddBar's specific layout / clip hierarchy — likely UC 08 (re-skin of main shell and queue rows) or UC 19 (audio-only toggle added to AddBar), since those are the last UCs that touched the AddBar. Fix is a small, targeted Slint markup / token adjustment — not a redesign — and must preserve the alignment of the UC 19 audio-only / audio+video toggle and the Add button next to the input.

## Acceptance Criteria

1. Typing into the URL input on the AddBar shows the entire glyph rectangle for every character: ascenders (h, k, l), descenders (g, j, p, q, y), and round-letter baselines (a, e, o, c) are all fully visible on all three OSes (Linux + macOS + Windows). Nothing is clipped at the top or bottom edge of the input.
2. The text cursor is rendered at full input height (or the design-system's specified cursor height), not clipped.
3. Placeholder text (the current "paste video URL" / equivalent copy) renders fully visible — same vertical envelope as live-typed text.
4. Text is vertically centered within the input's visible region (or aligned per the design-system's intended baseline anchor — confirmed against UC 07).
5. The input's height, padding, font-size, and line-height reconcile against the UC 07 design-system tokens — no orphan hard-coded values reintroduced. If the regression came from a missing token binding, the binding is added (no replacement hard-codes).
6. Both themes (light and dark, per UC 07) render the fix correctly. The visual fix is a layout/clip-correctness change, not a color change, so theme parity should be automatic — but explicit smoke is required.
7. The audio-only / audio+video toggle from UC 19 remains visually aligned with the URL input on the same baseline — the fix does not push the toggle out of vertical alignment.
8. The Add button next to the input remains aligned with the input row (same vertical-center or same baseline as before).
9. The fix is localized — only the affected `add_bar.slint` (or named component) and, if needed, the design-system token file are touched. No knock-on layout shifts elsewhere in the app shell.
10. A regression-resistance check is added: either an explicit unit/markup test against the input's effective text-rendering height vs. font line-height token, or a screenshot-diff smoke that would have flagged this clip. The same bug class should not re-emerge silently next time the AddBar markup is touched.
11. Manual smoke addendum in `CONTRIBUTING.UI.md` covering: typing the full alphabet (lower + upper) plus digits into the input across both themes on macOS / Linux / Windows; placeholder rendering when empty; cursor rendering on focus; IME-driven input (any CJK / accented Latin) renders without clipping.

## Potential Pitfalls & Open Questions

- **Edge case** — IME / multi-byte input. Tall-glyph scripts (CJK, Devanagari with stacked diacritics, accented Latin like "ÿ") must render correctly after the fix, not just baseline ASCII. The clip likely affects them at least as badly as Latin descenders.
- **Edge case** — Focus state vs. blur state. The bug may render differently with the field focused (live cursor + text) vs. blurred (placeholder). Spec covers both.
- **Edge case** — Magenta/violet color of the clipped strip in the screenshot. The dark-theme text color may itself be magenta/violet (probably a brand-accent token from UC 07), in which case the visible sliver is the actual text. Alternatively, the strip is the cursor or selection-underline color and the text is rendered slightly above/below. The dev team confirms during analyst review which interpretation matches.
- **Risk** — Whichever UC introduced the regression (08 or 19) had no test that would have caught it; criterion 10 forces the fix to add that missing check, otherwise this regression class will re-emerge the next time the AddBar markup is touched.
- **Risk** — A naive "increase input height" fix could push the AddBar row taller and disturb the alignment of the UC 19 toggle and the Add button. Criteria 7–8 explicitly guard against this.

## Original Description

The input for the url's shows the text heavily cut when typing into it. Only the top most part of every letter is seen.

## Visual evidence

See `29-fix-addbar-url-input-clipping.png` — dark theme, macOS. The AddBar appears just below the `yt-dlp-ui` title bar. The URL input's interior shows typed text as a thin (~2–3 px tall) magenta/violet horizontal strip occupying only the very top of the input's visible region. Letter shapes are not legible; only their topmost edges are rendered. The toggle/label visible to the right (likely the UC 19 audio-only / audio+video selector) and the queue row below ("SABATON – Bismarck (Official Music Video)") render at the correct text height — only the AddBar URL input is clipped.

## Clarifications

- Q: Which OS(es) show the URL-input clipping?
  A: All three OSes (Linux + macOS + Windows).
- Q: Are other text inputs in the app affected (Settings panel fields, etc.) or only the URL input on the AddBar?
  A: Only the URL input on the AddBar.
- Q: Do you have (or can you attach) a screenshot of the clipped state?
  A: Yes — attached as `29-fix-addbar-url-input-clipping.png` alongside this file.
