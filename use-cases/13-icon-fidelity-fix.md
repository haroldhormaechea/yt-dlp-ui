# Use Case 13: Icon fidelity fix (sizing + centering, app-wide)

## Summary

The `Button` primitive in `crates/app/ui/design/components.slint` and a number of ad-hoc `Image` usages across every `.slint` file under `crates/app/ui/` render icons noticeably smaller than intended and visibly anchored to the top-left of their containing surface instead of centered next to the text label or centered inside an icon-only container. Visible regressions span every icon-bearing surface in the app. The most likely root cause sits at `components.slint:97–103`: the `Image` is sized `12px × 12px` with `vertical-alignment: center;` but no `image-fit` and no `horizontal-alignment`, so the SVG renders at its native intrinsic geometry under Slint's default `fill` behaviour and anchors top-left; the same defective pattern has been copied to every ad-hoc icon usage outside the primitive. The fix is a single-source pass on the design system: introduce a two-tier icon-size token in `tokens.slint` (`icon-size: 14px`, `icon-size-sm: 12px`), patch the `Button` primitive to consume it with `image-fit: contain;` and centered alignment, add a sibling non-interactive `Icon` primitive for decorative glyphs (warning triangle, source-icon badge, status indicators) so every glyph in the app routes through one of the two primitives, migrate every ad-hoc `Image` icon usage app-wide, extend `_ComponentSmoke` with icon-bearing samples, and add a `slint-viewer`-driven snapshot test in CI that re-renders the smoke component to a PNG and pixel-diffs against a committed baseline. Builds on UC 07 (design-system foundation) and UC 08 (re-skin); this is fit-and-finish + design-system completion, not a re-skin.

## Acceptance Criteria

1. `crates/app/ui/design/tokens.slint` exposes `icon-size: 14px` (full-height controls / standard glyphs) and `icon-size-sm: 12px` (`sm` / `icon-sm` modifiers, badges, dense usages).
2. The `Button` primitive's leading `Image` consumes the token (no magic `12px`), and sets `image-fit: contain;`, `horizontal-alignment: center;`, `vertical-alignment: center;`. Resolves to `icon-size-sm` when `modifier == "sm" || modifier == "icon-sm"`, otherwise `icon-size`.
3. A new non-interactive `Icon` primitive is added to `design/components.slint`, wrapping `Image` with `image-fit: contain;`, centered alignment, and `colorize`-ready defaults. Inputs: `kind` (or `source: image`), `size: ""` / `"sm"` (default `""`), and `tint: color` defaulting to `DesignTokens.text`.
4. Every ad-hoc `Image` instance across all `.slint` files under `crates/app/ui/` (`main_window.slint`, `add_bar.slint`, `footer.slint`, `queue_row.slint`, `settings_panel.slint`, `bot_check_modal.slint`, `empty_state.slint`, `thumbnail.slint`) that draws a hand-authored icon registered in the `Icons` global is migrated to either `Button` (when interactive) or the new `Icon` (when decorative). After migration, no file outside `design/components.slint` and `icons.slint` instantiates a raw `Image` for an icon glyph. Thumbnails and shimmer/stripe rasters are out of scope — the AC targets glyphs in the `Icons` global.
5. Every icon visible in the main window — Add `+`, footer play / stop / Focus eye, per-row `×`, error-row warning triangle, source-icon badge, header sun/moon theme toggles — renders centered within its container at the size dictated by the token.
6. Icon-only buttons (`modifier: "icon"` / `"icon-sm"`) center the glyph both horizontally and vertically; nothing is clipped, no top-left offset is visible.
7. `currentColor` SVGs continue to recolor correctly via `colorize` across every `Button` variant (`primary`, `default`, `ghost`, `danger`) and disabled / hover state, and via the `Icon` primitive's `tint` input. Both light and dark themes verified.
8. `_ComponentSmoke` is extended with samples that exercise: (a) `Button { icon: Icons.plus; text: "Sample"; }`, (b) `Button { icon: Icons.x; modifier: "icon"; }`, (c) `Button { icon: Icons.play; modifier: "sm"; text: "Play"; }`, (d) `Icon { kind: "alert"; }`, (e) `Icon { kind: "alert"; size: "sm"; }`.
9. A new test (`crates/app/tests/icon_snapshot_test.rs` or equivalent) drives `slint-viewer` (or an embedded software-renderer call) to render `_ComponentSmoke` to a PNG and pixel-diffs against a committed baseline at `crates/app/tests/baselines/_component_smoke.png`. Failures emit the diff PNG path so a human can eyeball the regression.
10. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `cargo test --workspace` all pass; no new findings.
11. `Justfile` gains a `snapshot-update` recipe that regenerates the committed baseline.
12. `docs/adr/` gains a new ADR (next free number) recording the icon-size token choice, the `Button` + `Icon` primitive split, and the snapshot-test approach as the new design-system regression gate.
13. CI runs the snapshot test on at least the maintainer's primary OS. Cross-OS gating is out of scope for this UC (font/AA differences make multi-OS pixel diffs unstable).
14. No dark-mode regression: every icon stays centered and recolors correctly. Baseline is captured in light mode only at MVP; dark-mode baseline noted as a follow-up.

## Potential Pitfalls & Open Questions

- **Risk** — Bumping the standard icon size from 12 px → 14 px reflows every icon-bearing button. `add_bar.slint` Add button and the footer buttons have layout-sensitive widths; the per-row `×` is constrained by `queue_row.slint`. The migration must include a layout audit pass.
- **Risk** — `slint-viewer` snapshot diffing is not a first-class Slint feature today. Implementation will likely be a small workspace-internal helper that loads `_ComponentSmoke`, captures via Slint's software renderer or a hidden window, and pixel-diffs with the `image` crate. Sub-pixel AA / font differences make this OS-sensitive — hence AC #13 scoping to one OS.
- **Risk** — Pixel snapshots are high-friction as the design evolves. Every legitimate visual change forces a baseline regeneration. Mitigated by the `Justfile` recipe + ADR.
- **Edge case** — `image-fit: contain` will visibly letterbox any SVG whose viewBox is non-square. The dev-team must grep `crates/app/assets/icons/*.svg` viewBoxes pre-flight; any non-square ones get normalized inside this UC or split out as follow-ups.
- **Open question** — `SourceIcon` (already in `icons.slint`) duplicates some of the new `Icon` primitive's behaviour. Either rebase `SourceIcon` on `Icon` (cleanest) or leave both (simpler but redundant). Pick one in the ADR.

## Original Description

Improve the visual fidelity of icons in buttons and controls. Currently all icons render small-ish and anchored to the top-left of their button/control area instead of being properly sized and centered next to the label. Affects multiple controls visible in the main window: the "+ Add" button next to the URL field, the play icon in "Start all queued", the stop icon in "Cancel all", the warning triangle next to song titles on error rows, the "×" close buttons on each row, the two theme-toggle sun/moon icons in the header, and the eye icon in the "Focus" indicator at the bottom-right. Screenshot was provided showing the issue across all of these.

## Clarifications

- Q: What target icon size should the design system standardize on?
  A: Two-tier token (`icon-size: 14px` for full-height, `icon-size-sm: 12px` for `sm` / `icon-sm` modifiers and badges).
- Q: What surfaces should this UC cover?
  A: Every icon, app-wide — every `Image`-backed glyph in every `.slint` file under `crates/app/ui/` registered in the `Icons` global.
- Q: How should ad-hoc Image icon glyphs outside Button be handled?
  A: Migrate all to the design-system primitives (`Button` for interactive, `Icon` for decorative).
- Q: What's the acceptance gate for visual correctness?
  A: `slint-viewer` snapshot test in CI, pixel-diffing `_ComponentSmoke` against a committed baseline.
- Q: How should non-interactive icon glyphs (warning triangle, source-icon badge, status-only indicators) fit into the Button-everything migration?
  A: Add a new non-interactive `Icon` primitive alongside `Button`; both share the icon-size tokens, `image-fit: contain`, and `colorize` defaults.
