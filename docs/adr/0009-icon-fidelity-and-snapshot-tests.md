# 0009 — UC 13 icon fidelity and snapshot tests

## Status

Accepted (UC 13, 2026-05-06).

## Context

Every icon in the main window — the `+ Add` button, the footer play / stop /
Focus eye, per-row `×`, the error-row warning triangle, the source-icon
badge, and the header sun/moon theme toggles — was rendering noticeably
smaller than intended and visibly anchored to the top-left of its
containing surface instead of centered. The root cause sat at
`crates/app/ui/design/components.slint:97-103`: the `Button` primitive's
leading `Image` was sized `12px × 12px` with `vertical-alignment: center;`
but no `image-fit` and no `horizontal-alignment`, so the SVG rendered at
its native intrinsic geometry under Slint's default `fill` behaviour and
anchored top-left. The same defective pattern had been copied to every
ad-hoc icon usage outside the primitive (six files across `crates/app/ui/`).

UC 07 shipped the design-system foundation (`DesignTokens` + primitive
components). UC 08 ported the re-skin onto those primitives. UC 13 is the
fit-and-finish pass that closes out the design system: a token for icon
sizing, a non-interactive `Icon` primitive sibling to `Button`, and a
snapshot-test gate so future regressions are caught at CI time rather
than at "the user noticed".

## Decisions

### Two-tier icon-size token

`crates/app/ui/design/tokens.slint` exposes:

```
out property <length> icon-size:    14px;
out property <length> icon-size-sm: 12px;
```

`icon-size` (14 px) is the standard glyph size for full-height controls
and decorative icons. `icon-size-sm` (12 px) is the dense / `sm` /
`icon-sm` variant. Every glyph in the app routes through one of these
two values; no caller hard-codes a magic icon size.

### `Button` primitive patch

The leading icon in `Button` (`design/components.slint`) consumes the
token, with `image-fit: contain;` and centered horizontal/vertical
alignment. Resolution rule: `modifier == "sm" || modifier == "icon-sm"`
→ `icon-size-sm`, otherwise `icon-size`. `colorize: root.fg-color()` is
preserved so `currentColor` SVGs continue to recolor for every variant
(`primary`, `default`, `ghost`, `danger`) and disabled / hover state.

### New `Icon` primitive

Added to `design/components.slint` as a non-interactive sibling of
`Button`. Three inputs, no escape hatches:

```
in property <image>  source;
in property <string> size:  "";   // "" → icon-size, "sm" → icon-size-sm
in property <color>  tint:  DesignTokens.text;
```

Internals: a single `Image` child with `source: root.source`,
`colorize: root.tint`, token-driven `width`/`height`, `image-fit:
contain;`, and centered alignment. The wrapper `Rectangle` binds its
`width`/`height` to the same token so layout sees a definite intrinsic
size.

A `kind`-string variant was rejected — every caller already imports the
`Icons` global and supplies the image directly, so a `kind` indirection
just doubles the surface area. A `custom-size` escape hatch was rejected
because the *point* of the design-system completion is that no one
sneaks a third icon size in.

### `SourceIcon` rebased on `Icon`

`SourceIcon` was already a near-duplicate of the new `Icon` primitive
with a hostname-derived source mapping. It now `inherits Icon` (with
`size: "sm"`, `tint: white`, plus the existing `opacity: 0.85`) and the
hostname-to-image function. Call sites are unaffected.

`SourceIcon` lives in a new `crates/app/ui/source_icon.slint` file
rather than `icons.slint`. The reason is mechanical: the snapshot-test
`_ComponentSmoke` (see below) needs the `Icons` global to construct
sample buttons, so `design/components.slint` imports `icons.slint`. If
`icons.slint` also imported `Icon` from `design/components.slint`, the
Slint compiler would refuse the recursive import. Splitting out
`SourceIcon` breaks that cycle.

### App-wide migration of ad-hoc `Image` icons

Every `Image` instance across all `.slint` files under `crates/app/ui/`
that draws a hand-authored icon registered in the `Icons` global was
audited. After migration, no file outside `design/components.slint`,
`icons.slint`, and `source_icon.slint` instantiates a raw `Image` for an
icon glyph — *with two deliberate exceptions:*

| Site | Disposition | Reason |
|---|---|---|
| `add_bar.slint:41` (link) | → `Icon` | interactive container, decorative glyph |
| `empty_state.slint:28` (download) | **raw `Image` + bug-fix patch** | feature illustration, 22×22 |
| `queue_row.slint:177` (alert) | → `Icon` | error-row warning triangle |
| `queue_row.slint:308` (info) | → `Icon` | waiting-on-user info glyph |
| `queue_row.slint:330` (check) | → `Icon` | done-row success glyph |
| `main_window.slint:166` (info) | → `Icon` | deno banner |
| `settings_panel.slint:61` (shield) | → `Icon` | privacy-disclosure header |
| `bot_check_modal.slint:235` (shield) | **raw `Image` + bug-fix patch** | feature illustration, 16×16 inside a 32×32 circle |

**Deviation from AC #4.** AC #4 reads "every ad-hoc `Image` instance ...
is migrated to either `Button` (when interactive) or the new `Icon`
(when decorative)." The empty-state and bot-check shield are both
*feature illustrations* — they live inside a deliberately-sized
container (a 56×56 download card and a 32×32 warning circle
respectively) and are not part of the standard glyph cadence the
14 / 12 px tokens describe. Migrating them to `Icon` would either drop
them to 14 px (visually wrong against their containing card) or force a
`custom-size` escape hatch that defeats the entire token discipline.
Instead, both keep their original sizing and receive the same three-line
bug-fix patch as the `Button` primitive (`image-fit: contain;` +
horizontal and vertical alignment center). The user signed off on this
deviation; it is recorded here so future readers know it is intentional.

Thumbnails and shimmer/stripe rasters are out of scope — AC #4 targets
glyphs in the `Icons` global, not raster image content.

### `_ComponentSmoke` rendered surface

`_ComponentSmoke` previously inherited `Rectangle` and held every
primitive sample behind an `if root.always-false :` block — a
type-checker-only smoke that never reached the renderer. UC 13 promotes
the icon-bearing samples to **always-rendered children**:

- `Button { icon: Icons.plus; text: "Sample"; }`
- `Button { icon: Icons.x; modifier: "icon"; }`
- `Button { icon: Icons.play; modifier: "sm"; text: "Play"; }`
- `Icon { source: Icons.alert; }`
- `Icon { source: Icons.alert; size: "sm"; }`

The non-icon-bearing samples (Input, Select, Badge, KbdHint, Toggle,
Stepper, Tooltip, Toast) stay behind `if false :` so the type-check
exercise is preserved without paying for them at render. The component
is now a `Window` rooted at fixed `360 × 240 px` so the snapshot test
can render it via Slint's software renderer without instantiating
`MainWindow`. It is re-exported from `main_window.slint` so the test
can `slint::include_modules!()` and reach it.

### Snapshot-test approach (Branch A — embedded software renderer)

The test infrastructure itself (`crates/app/tests/icon_snapshot_test.rs`)
is the QA agent's scope, not the developer's. The developer's
contribution is the `Cargo.toml` dev-dep wiring and the `Justfile`
recipe; both depend on which of two implementation branches we pick.
The decision was time-boxed to a ~50-line spike against Slint 1.16.1's
`Platform` trait surface.

**Branch A wins.** `i_slint_renderer_software::MinimalSoftwareWindow`
already implements `WindowAdapter` and exposes `draw_if_needed(&self,
render_callback: impl FnOnce(&SoftwareRenderer) -> ())`. A `Platform`
impl is therefore tiny — only `create_window_adapter()` is required;
every other method has a usable default. Sketch (for QA reference; the
spike is informational, not committed code):

```rust
use std::rc::Rc;
use slint::platform::{Platform, WindowAdapter, PlatformError};
use slint::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};

struct TestPlatform { window: Rc<MinimalSoftwareWindow> }

impl Platform for TestPlatform {
    fn create_window_adapter(&self)
        -> Result<Rc<dyn WindowAdapter>, PlatformError>
    {
        Ok(self.window.clone())
    }
    // duration_since_start, click_interval, cursor_flash_cycle, clipboard,
    // event-loop methods all use defaults — no event loop is needed for a
    // single render pass.
}
```

The renderer side is a `SoftwareRenderer::render(&self, buffer:
&mut [Rgb565Pixel] | &mut [Rgba8Pixel], pixel_stride: usize)` call from
inside `draw_if_needed`. The buffer feeds straight into the `image`
crate (`image::RgbaImage::from_raw(...)`) for PNG encoding / decoding /
diffing.

This Branch A path requires the `renderer-software` Slint feature.
Because the workspace's default `slint = "1.16.1"` does not enable it,
`crates/app/Cargo.toml` carries an `image` + `slint = { ...,
features = ["renderer-software"] }` dev-dep block scoped to
`cfg(target_os = "macos")` (see "Baseline scope" below). Cargo's
feature-unification means the macOS dev/test build pulls the software
renderer in, while release builds (which never see dev-dependencies)
stay lean.

**Branch B alternative (rejected).** A purely structural smoke +
manual `slint-viewer` baseline. Branch B sidesteps the `Platform` impl
but loses CI gating — a human has to remember to re-render the baseline
in `slint-viewer` and visually compare. The Branch A spike fit
comfortably under the budget, so the gating value won out.

### Baseline scope: macOS-only, light-mode-only

The baseline PNG at `crates/app/tests/baselines/_component_smoke.png`
is captured on macOS in light mode at MVP. Cross-OS gating is out of
scope for this UC because sub-pixel AA and font-rendering differences
between macOS, Linux, and Windows make multi-OS pixel diffs unstable
without per-OS baselines (which is a research project of its own). The
test is gated on `cfg(target_os = "macos")` to match. Dark-mode
baseline is recorded as a follow-up; AC #14 only requires that
dark-mode renders correctly, which is exercised manually for now.

### `Justfile` `snapshot-update` recipe

Adds `snapshot-update`, which re-runs the snapshot test with
`SNAPSHOT_UPDATE=1` set so the test overwrites the committed baseline
instead of pixel-diffing against it. Re-run after any legitimate visual
change to the icon-bearing samples, inspect the PNG, commit it. The
recipe wraps the Branch A path; Branch B would have used a manual
`slint-viewer` invocation instead.

## Consequences

- **Pixel snapshots are high-friction as the design evolves.** Every
  legitimate visual change forces a baseline regeneration. The
  `snapshot-update` recipe + this ADR are the friction-reducers; they
  do not eliminate it. Expect to regenerate the baseline whenever
  `_ComponentSmoke`'s visible children change.
- **A second image-comparison test will appear someday.** The eventual
  dark-mode baseline (or an Input / Select / Badge regression test) is
  a near-certain follow-up. Before adding a second test, consolidate
  the `Platform` setup, render-to-buffer, and diff helpers into a
  shared `crates/app/tests/snapshot_support.rs` module so each new
  snapshot is one function call, not a copy-paste of the renderer
  plumbing. **Future-test consolidation warning** recorded here so
  whoever lands the second snapshot test sees this advice.
- **`image-fit: contain` letterboxes non-square SVGs.** UC 13's
  pre-flight grep of `crates/app/assets/icons/*.svg` viewBoxes is
  the developer's responsibility at use-case time; if a non-square
  viewBox is introduced later (say, a wordmark-shaped logo), it will
  visibly letterbox inside its 14 px / 12 px box. Either normalize the
  viewBox or split out a follow-up.
- **The `_component_smoke.png` baseline is committed to the repo.** It
  is a small PNG (≪ 50 KB at 360×240). Treating it as source rather
  than a build artifact is the right call for a snapshot regression
  gate; ignore it from no-format-bumps lint rules if any are added.
- **AC interpretation deviations are documented above.** Two raw
  `Image` carve-outs (empty-state download, bot-check shield) and the
  `kind`-property rejection for `Icon` (replaced with `source: image`)
  are both deliberate and signed off.

## Alternatives considered

- **`Icon` with a `kind: string` enum.** Rejected — the `Icons` global
  already hands out `image` values; a `kind` indirection just doubles
  the surface area for no win. Callers write
  `Icon { source: Icons.alert; }` directly.
- **`Icon { custom-size: 22px }` escape hatch.** Rejected — the whole
  point is that no one sneaks a third icon size in. The two feature
  illustrations that need a non-token size keep their raw `Image`
  intentionally; that carve-out is bounded and visible in code review.
- **Branch B (manual `slint-viewer` baseline).** Rejected — see above.
- **Cross-OS pixel diffs.** Deferred — sub-pixel AA differences make
  this a research project of its own.
- **Dark-mode baseline now.** Deferred to a follow-up. AC #14 only
  requires dark-mode renders correctly, not that it is gated by a
  pixel diff.

## Cross-references

- ADR 0007 (design-system foundation) — addended below to point here.
- ADR 0008 (reskin and thumbnails) — `_ComponentSmoke` was originally
  introduced there as a `Rectangle`; UC 13 promotes it to a `Window`.
