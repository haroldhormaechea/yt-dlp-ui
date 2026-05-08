# Use Case 09: Settings slide-in panel

## Summary

A 380-px settings panel slides in from the right edge of the main window, triggered by the gear icon in the add bar (placeholder from UC 08), with a translucent backdrop and three close affordances (backdrop click, close button, ESC). The panel is implemented as a layered Slint `Rectangle` with an animated `x` property plus a backdrop overlay (no `PopupWindow`). The folder picker uses `rfd::AsyncFileDialog` so the UI stays responsive while the OS picker is up. Three tabs (General / Cookies / Privacy & Ads) host all user-configurable settings, built from UC 07's primitives. General exposes existing settings (format, destination, concurrency cap with a custom stepper). Cookies wires the existing `cookies_browser` setting into a select filtered by `browsers.rs` detection, with a graceful empty state. Privacy & Ads adds two new KV keys (`focus_mode`, `ads_personalized`) plus a "How ads work here" disclosure block. Persistence is immediate on change. Active tab is remembered in memory across panel open/close within the session and resets to General on app restart. The theme toggle stays in the add bar header per UC 07 and is explicitly not duplicated here. Ad-SDK integration and first-launch consent dialog remain out of scope.

## Acceptance Criteria

1. Clicking the gear icon in the add bar (placeholder from UC 08) opens the panel.
2. Panel is a layered Slint `Rectangle` with absolute positioning, animated `x` property, and a separate backdrop overlay `Rectangle`. `PopupWindow` is NOT used.
3. Open dimensions: 380 px wide, full window height; `surface` background, 1-px `border` left edge, `shadow-lg`.
4. Backdrop fills the rest of the main window with `rgba(0,0,0,0.18)`.
5. Open and close animate over 200 ms with an ease-in cubic curve.
6. Backdrop click, the close (×) button, and the ESC key all close the panel. ESC handling is registered at the top-level main window and consumes the key event when the panel is open so it does not propagate to the main window.
7. Header shows "Settings" (14.5 px, 600 weight) and a ghost close button.
8. Three tabs (General / Cookies / Privacy & Ads) with a 2-px violet underline on the active tab; inactive tabs in `text-2`.
9. Active tab is remembered in memory across panel open/close within the same session; opening the panel after app restart shows General.
10. **General → Format preference** — `Select` (UC 07 primitive) with options "Best video (bestvideo+bestaudio/best)", "Best audio · MP3", "Best audio · Opus". Persists to existing `format_preference` KV key on change. Hint copy: "Applies to subsequent downloads."
11. **General → Download destination** — a mono read-only display of the current destination (with `$HOME → ~` substitution and middle-ellipsis when long), plus a `Choose…` button that opens the OS folder picker via `rfd::AsyncFileDialog` and persists the selection on success. The async picker does not block the main thread; the panel stays interactive while the picker is up.
12. **General → Concurrency cap** — custom stepper (`−` button, gradient-filled track with a 14×14 thumb, `+` button, mono numeric value), range 1–10. Persists to existing `concurrency_cap` KV key. Hint copy reflects the current value: "Up to N parallel downloads."
13. **Cookies → Cookies source** — `Select` listing detected browsers via `browsers.rs` (UC 05) plus "None" as the first option. Persists to existing `cookies_browser` KV key.
14. **Cookies → Empty state** — when zero browsers are detected, the select is disabled and the hint reads "No supported browsers detected on this machine." Otherwise the hint reads "Used only when YouTube asks for verification."
15. **Cookies → Info box** — an `accent-soft` block with shield icon and verbatim copy "Cookies stay on this machine. yt-dlp-ui never uploads them anywhere."
16. **Privacy & Ads → Focus mode toggle** — 32×18 pill toggle. Persists to a new `focus_mode` KV key (boolean, default `false`). Sub-label: "Ad slot hidden" / "Ad slot visible".
17. **Privacy & Ads → Ad personalization toggle** — same toggle component. Persists to a new `ads_personalized` KV key (boolean, default `true`). Sub-label: "Personalized ads on" / "Generic ads only".
18. **Privacy & Ads → Disclosure block** — `surface-2` background, 1-px `border`, heading "How ads work here" (12 px, 600 weight) and the verbatim copy from `settings-panel.jsx` (~lines 188-199). Includes an `accent-text`-styled "Read the vendor's privacy policy." link; clicking it opens the vendor URL in the OS default browser via `open` / `xdg-open` / `start`. When no vendor URL is configured, the link is shown but disabled with a tooltip "No ad vendor configured yet."
19. Every setting change writes to the KV store immediately on commit. No Save / Apply button.
20. Every visual element uses UC 07's `DesignTokens`; the panel re-themes correctly when `dark-mode` flips, including the backdrop opacity if the design specifies a different value per theme.
21. Tab content layout matches `settings-panel.jsx`'s `Field` component: 12-px 600-weight label, control, optional 11.5-px `text-3` hint.
22. The theme toggle is NOT duplicated in this panel — the moon/sun button in the add bar (UC 07) remains the sole theme switch.
23. Tests — KV round-trip tests for `focus_mode` and `ads_personalized`; model-level test for the empty-cookies-state branch; rfd async picker covered by an integration test or smoke test (or, if Slint+rfd is hard to drive headlessly, a manual smoke documented in `CONTRIBUTING.UI.md`); UC 01 + UC 05 + UC 07 + UC 08 tests still pass; `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` pass.

## Potential Pitfalls & Open Questions

- **Risk** — ESC-key handling. Slint focus + key listener model is per-window. The callback at the top-level `MainWindow` must close the panel when open and consume the event so it does not also close the main window.
- **Risk** — `rfd::AsyncFileDialog` requires a tokio runtime context; existing app already runs tokio multi-threaded so this is fine, but confirm the spawn site keeps the future running until the user picks (no premature drop).
- **Risk** — Concurrency-cap stepper visual: track fill + thumb position math must adapt to track width; animation between values is optional but desirable for polish.
- **Edge case** — Browser-list refresh: detection runs at app start (UC 05). If the user installs a browser while the app is running, the cookies dropdown won't reflect it until restart. Acceptable for MVP; worth a one-line code comment.
- **Edge case** — Two new KV keys (`focus_mode`, `ads_personalized`) added without a schema migration. Same shape as UC 07's `theme` addition; consistent with the brief's KV-table design.
- **Risk** — Disclosure-block link to vendor privacy policy is disabled until a vendor is picked. The disabled state must be visually distinct (greyed link, default cursor) and the tooltip must work on Slint's hover model.
- **Risk** — Slint global hot-swap caveat from UC 07: if any panel element binds to a non-reactive theme value, dark-toggle leaves stale colors. Bind directly to `DesignTokens` properties.

## Original Description

> **Use case: Settings slide-in panel.**
>
> Source of truth: `<TARGET_DIR>/design/project/settings-panel.jsx`. The panel slides in from the right edge of the main window (380 px wide, full window height), with a translucent backdrop covering the rest of the app. Backdrop click, the close (×) button in the header, and the ESC key all close it. Triggered by the gear icon in the add bar (the placeholder already added in UC 08). All controls are built from UC 07's primitives where possible.
>
> Three tabs, matching the design's tab strip with violet underline on the active tab:
>
> - **General**
>   - **Format preference** — `Select` with three options: "Best video (bestvideo+bestaudio/best)" (default), "Best audio · MP3", "Best audio · Opus". Persists to the existing `format_preference` KV key (already added in UC 01). Hint copy: "Applies to subsequent downloads."
>   - **Download destination** — a mono read-only `Input`-styled display showing the current destination (with `$HOME → ~` substitution and middle-ellipsis), plus a `Choose…` `Button` that opens the OS folder picker via `rfd` (already in UC 01 deps) and persists on selection. Per UC 01: changes apply to *new* rows only; in-flight rows keep their snapshotted `dest_dir`.
>   - **Concurrency cap** — a custom stepper matching the design (`−` button, gradient-filled track with a 14×14 thumb, `+` button, mono numeric value). Range 1–10. Persists to existing `concurrency_cap` KV key. Hint copy: "Up to N parallel downloads."
> - **Cookies**
>   - **Cookies source** — a `Select` with options `None / Brave / Chrome / Chromium / Edge / Firefox / Opera / Safari / Vivaldi`. The list is filtered to browsers actually detected by the existing `browsers.rs` code (UC 05). When no browsers are detected, the select is disabled and the hint reads "No supported browsers detected on this machine.". Persists to existing `cookies_browser` KV key. Hint when enabled: "Used only when YouTube asks for verification."
>   - **Privacy info box** — a small accent-soft block with the shield icon: "Cookies stay on this machine. yt-dlp-ui never uploads them anywhere." (verbatim from design)
> - **Privacy & Ads**
>   - **Focus mode toggle** — custom toggle (32×18 pill switch). Persists to a new `focus_mode` KV key (boolean). UC 11 will consume it for ad-slot hiding; today the toggle just persists. Sub-label reflects state ("Ad slot hidden" / "Ad slot visible").
>   - **Ad personalization toggle** — same toggle component. Persists to a new `ads_personalized` KV key. No real ad SDK is wired yet; this is the user's expressed preference for when the SDK lands. Sub-label: "Personalized ads on" / "Generic ads only".
>   - **"How ads work here"** disclosure block — `surface-2` background, 1-px border, with a heading "How ads work here" and a paragraph (verbatim from design): "yt-dlp-ui shows ads to keep development sustainable. The ad SDK collects device data (OS, locale, screen size) and behavioral signals to pick what to show. Read the vendor's privacy policy. You can turn off personalization above, or hide the slot entirely with Focus mode." The "Read the vendor's privacy policy" text is a link styled in `accent-text`; opens the vendor URL in the system browser. Vendor URL is unset today (no vendor picked yet) — use a placeholder URL or disable the link until UC's deferred ad-SDK integration.
>
> Out of scope:
> - The actual ad-window helper process integration (UC 11 / future).
> - First-launch ads-consent dialog (separate flow per the brief's monetization section).
> - Theme toggle (UC 07 already shipped it as a header button — explicitly not duplicated here).
> - Cookies / browser detection backend (UC 05; we only consume).
>
> Dependencies / interactions:
> - Hard-blocked by UC 07 (Button, Input, Select, primitives, palette).
> - Soft-blocked by UC 08 (the gear button must already exist in the re-skinned add bar). Could ship in either order if the gear placeholder is wired in this UC instead, but cleanest is UC 07 → 08 → 09.
> - Touches `crates/app/src/db/settings.rs` to add `focus_mode` and `ads_personalized` keys.
> - Touches `crates/app/src/ui_bridge.rs` to expose all settings to Slint and write back on change.
> - Touches `crates/app/src/browsers.rs` only as a consumer (read detected list).

## Clarifications

- Q: Slide-in implementation approach?
  A: Layered Rectangle with animated `x` + backdrop overlay. Full control over animation and backdrop. `PopupWindow` is NOT used.
- Q: rfd file picker — async or blocking?
  A: Async via `rfd::AsyncFileDialog`. UI stays responsive while the OS picker is up.
- Q: "Read the vendor's privacy policy" link with no vendor configured?
  A: Show the link but disable it with tooltip "No ad vendor configured yet." Honest about the missing piece.
- Q: Should the panel remember which tab was last open?
  A: In-memory only (per session). Reopening within the same session shows the last tab; app restart resets to General. No new KV key for this.
