# Use Case 07: Design system foundation

## Summary

Port the Claude Design tokens from `design/project/tokens.css` into a Slint global named `DesignTokens` (or `AppPalette` if there's a name clash) with light and dark variants, and define the reusable component primitives (Button variants, Input, Select, Badge, KbdHint) that UC 08–11 will consume. Files live under a new `crates/app/ui/design/` subfolder (`tokens.slint`, `components.slint`); `main_window.slint` keeps its focus on layout and imports from there. OKLCH values are pre-converted to sRGB hex via Culori/colorjs.io reference values and pasted into Slint with the original OKLCH preserved as adjacent comments. Inter and JetBrains Mono `.ttf` files are bundled under `crates/app/assets/fonts/` and registered at startup. A new `theme` key in the SQLite settings KV table stores `"light"`, `"dark"`, or `"system"`; first-launch default is `"system"`, resolved at startup against `slint::ColorScheme` so the user gets the OS preference. A moon/sun toggle in the add bar switches between `"light"` and `"dark"` and persists immediately. UC 01's existing layout, behavior, and acceptance criteria stay green — this UC introduces foundation only, no cosmetic re-skin.

## Acceptance Criteria

1. `crates/app/ui/design/tokens.slint` defines a global named `DesignTokens` (fallback `AppPalette` if Slint reserves the name) exporting every token from `tokens.css`: backgrounds (bg, surface, surface-2, surface-3), borders + divider, four text shades, accent family (5), success / warning / danger / muted families, three shadow tiers, three radius tiers.
2. The global has light and dark variants selected by a `dark-mode: bool` property; flipping it re-themes every consuming component at runtime.
3. Every OKLCH value from `tokens.css` is pre-converted to sRGB hex using Culori or colorjs.io reference values, with the original OKLCH literal preserved as an adjacent comment. No OKLCH expression appears in Slint source. A small reproducible conversion artifact (script or table) is checked into `tools/oklch-to-srgb.{py,js,md}`.
4. The add bar in `crates/app/ui/main_window.slint` shows a moon/sun icon button matching `design/project/app.jsx`. Clicking it flips `DesignTokens.dark-mode` and persists the explicit choice immediately.
5. A `theme` key is added to the SQLite settings KV table with values `"light" | "dark" | "system"`. First-launch default is `"system"`.
6. `crates/app/src/ui_bridge.rs` reads `theme` at startup, resolves `"system"` against `slint::ColorScheme` (or equivalent), sets `DesignTokens.dark-mode` accordingly, and writes the explicit pick back on toggle. The toggle persists `"light"` or `"dark"` (never `"system"` — once the user picks, it sticks).
7. Persistence survives app restart, verified by an automated test (set theme → close → reopen → assert).
8. `crates/app/ui/design/components.slint` defines `Button` (variants: `primary`, `default`, `ghost`, `danger`; modifiers: `sm`, `icon`), `Input`, `Select`, `Badge` (seven status colorings: `queued`, `inflight`, `done`, `error`, `cancelled`, `cancelling`, `waiting`), and `KbdHint`. Each replicates hover / focus / active / disabled states from `tokens.css`'s `.btn`, `.input`, `.select`, `.badge` rules.
9. Inter and JetBrains Mono `.ttf` files are committed under `crates/app/assets/fonts/` and registered at startup via Slint's font-registration API. UI text uses Inter; mono surfaces use JetBrains Mono.
10. UC 01's layout and behavior in `main_window.slint` keeps working unchanged from a user perspective; every UC 01 acceptance criterion still passes after this UC lands.
11. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check` pass.
12. No new third-party runtime crate is added. (A reference conversion script under `tools/` may use any language; it is not part of the Cargo build.)
13. `docs/adr/0007-design-system.md` records: OKLCH→sRGB conversion approach, font bundling decision (Inter + JetBrains Mono with subset details), file layout (`crates/app/ui/design/`), and design source-of-truth path (`design/project/tokens.css`).

## Potential Pitfalls & Open Questions

- **Risk** — Slint global hot-swap. Most properties re-evaluate on global change, but compile-time-bound style aspects may not. The analyst must verify every primitive actually re-themes when `dark-mode` flips, and bind dynamic properties to the global directly where needed.
- **Risk** — `slint::ColorScheme` requires Slint 1.4+; project pins 1.16.1, so this should be fine, but confirm during analysis. Fallback when OS preference is unavailable is `"light"`.
- **Edge case** — User toggles after starting in `"system"` — they exit system mode permanently per AC#6. Restating to keep the contract visible.
- **Edge case** — KV write race: silent retry + tracing WARN on failure; the toggle still works for the session even if persistence fails. No UI surface needed.
- **Risk** — Font bundle size: Inter (full) ~1 MB; with Latin/Latin-Ext subset ~300–500 KB. JetBrains Mono similar subset ~200 KB. Total ~500–700 KB added; fits the <50 MB installer budget. Subset choice is an implementation decision.
- **Risk** — Hover/focus/active/disabled state machines need visual smoke. No automated way to assert "the button looks right when hovered"; manual review per primitive is required at QA.
- **Edge case** — Naming conflict: Slint's std `Palette` global may exist in some versions. Use `DesignTokens` or `AppPalette` to avoid; analyst picks at implementation time.

## Original Description

> Use case: Design system foundation — port the Claude Design tokens into a Slint global palette, add light/dark theme support with a runtime toggle persisted in SQLite, and define the reusable visual primitives (Button variants, Input, Select, Badge, etc.) that every subsequent UI use case depends on.
>
> Source of truth for the design lives in <TARGET_DIR>/design/ (just saved). Specifically:
> - design/project/tokens.css — the canonical color/spacing/typography/radius/shadow tokens for both light and dark themes.
> - design/project/app.jsx — shows the moon/sun toggle in the add bar header (line ~219-226).
> - design/project/queue-row.jsx, settings-panel.jsx, bot-check-modal.jsx — consumers of the tokens; useful as reference for which tokens are actually used.
> - design/README.md — handoff instructions from Claude Design.
>
> Key design decisions already locked in:
> - Aesthetic: clean utilitarian (think native macOS Finder / Transmission).
> - Accent color: violet, oklch(0.58 0.18 295) light / oklch(0.72 0.16 295) dark.
> - Fonts: Inter for UI, JetBrains Mono for monospace/URLs/numbers. These need to be bundled into the binary or fall back to system fonts; the choice between bundling and system-fallback is open and should be settled in this use case.
> - Both themes (light/dark) shipped, runtime-switchable via the moon/sun button in the add bar.
> - Theme choice persists across app restarts in the existing SQLite settings KV table.
> - OKLCH values in tokens.css must be pre-converted to sRGB hex/RGBA at port time (Slint colors are sRGB-only); preserve original OKLCH as comments next to each Slint color.
>
> Scope boundaries:
> - This UC ships the foundation only — palette, theme toggle wiring, primitives. It does NOT re-skin the queue rows, the settings panel, the modal, the ad slot, or the banner; those live in UC 08-11.
> - The existing main_window.slint UC 01 layout keeps working visually after this UC; this UC adds the palette and a toggle, but cosmetic re-skin is UC 08.
> - No new behavioral functionality (no new buttons that do anything beyond toggling theme).
>
> Dependencies / interactions with existing code:
> - crates/app/ui/main_window.slint (353 lines today) — receives the new palette and toggle.
> - crates/app/src/db/settings.rs — needs a new "theme" KV entry (string: "light" | "dark", default "light" or system, TBD).
> - crates/app/src/ui_bridge.rs — needs to read/write the theme setting and propagate to Slint global.
>
> Blocks: UC 08 (queue re-skin), UC 09 (settings panel), UC 10 (bot-check modal UI), UC 11 (ad slot + banner + toast).

## Clarifications

- Q: What should the default theme be on first launch?
  A: Follow OS — read `slint::ColorScheme` on startup; user's explicit toggle thereafter persists `"light"` or `"dark"` and exits system mode permanently.
- Q: Font loading strategy?
  A: Bundle Inter + JetBrains Mono `.ttf` under `crates/app/assets/fonts/` and register at startup. Deterministic look across all OSes; ~500–700 KB added with sensible subsetting, well within the <50 MB installer budget.
- Q: Where should the palette / primitives Slint files live?
  A: `crates/app/ui/design/` subfolder — `tokens.slint`, `components.slint`. Keeps `main_window.slint` focused on layout; UC 09–11 import from one well-known root.
- Q: How should OKLCH → sRGB conversion be handled?
  A: One-time conversion via Culori/colorjs.io reference values, hex literals checked into Slint with the original OKLCH preserved as comments. A reproducible conversion script or table lives in `tools/`. Zero runtime cost, deterministic, no math drift.
- Q: Should the `Badge` component ship in this UC or wait for UC 08?
  A: Include in this UC. Defining all seven status variants here keeps UC 08 leaner and tests the seven-color system in isolation before any consumer is built.
