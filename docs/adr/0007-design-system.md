# ADR 0007: Design system foundation

## Status

Accepted — 2026-04-30 (UC 07).

## Context

UC 07 ports the Claude Design tokens (defined in `design/project/tokens.css`)
into a Slint global so every consuming component can re-theme at runtime via
a single `dark-mode: bool` flip. UC 08–11 will build on top of the primitives
this UC introduces. Several decisions inside that port have long-term
consequences and deserve a record.

## Decisions

### 1. OKLCH → sRGB conversion strategy

**Decision.** OKLCH literals from `tokens.css` are pre-converted to sRGB hex
once, at port time, using the [Culori](https://culorijs.org/) reference
implementation (npm `culori@4`). The hex values are committed verbatim into
`crates/app/ui/design/tokens.slint`; the original OKLCH spec is preserved as
an inline comment next to each value so future re-conversions are auditable.

Conversion is reproducible via `tools/oklch-to-srgb.js` — a ~50-line Node
script with `culori@4` pinned in its header. Run a human-paced

```sh
TMP=$(mktemp -d) && cd "$TMP" && npm init -y >/dev/null \
  && npm install culori@4 --no-save \
  && NODE_PATH="$TMP/node_modules" node /path/to/tools/oklch-to-srgb.js
```

to regenerate the table. The script is **not** wired into the Cargo build —
it is a one-shot conversion artifact.

**Rationale.** Slint colors are sRGB-only as of v1.16; there is no native
OKLCH literal. Doing the conversion at runtime would add a dependency
(`palette` or hand-rolled math) and pay a cost per re-theme; doing it at
port time is zero-runtime-cost and deterministic. Out-of-gamut OKLCH values
(notably the high-chroma `accent-soft`/`accent-hover` literals at L≈0.96/0.78)
are clipped to the sRGB gamut by Culori's `clampChroma`, matching what a
modern browser would render for the same literal.

**Caveat.** A handful of accent-family literals are slightly outside the sRGB
gamut. The clipped sRGB hex is a perceptual approximation, not a perfect
round-trip. Anyone editing `tokens.css` and re-running the script should eyeball
the diff against the design canvas before committing.

### 2. Font bundling — Inter + JetBrains Mono

**Decision.** Both fonts are bundled inside the binary, under
`crates/app/assets/fonts/`, and registered automatically by the Slint
compiler via top-of-file `import "../../assets/fonts/<font>.ttf";` lines in
`tokens.slint`. No Rust glue (`register_font_from_memory` / `_from_path`) is
needed.

The 5 shipped files (committed under `crates/app/assets/fonts/`):

| File                          | Upstream         | sha256                                                             | Fetched from                                                                                              |
|-------------------------------|------------------|--------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|
| `Inter-Regular.ttf`           | Inter            | `a414b48aa577ef2c62ebb135341ddeef33ee26a4f5dc9f787f93c1aab08ebb50` | `cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/400Regular/Inter_400Regular.ttf`                     |
| `Inter-Medium.ttf`            | Inter            | `f1738576525e86db1d5cf63a6c1b56e0a7e2a2898b499ac93db95f2e7a9f9cd5` | `cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/500Medium/Inter_500Medium.ttf`                       |
| `Inter-SemiBold.ttf`          | Inter            | `f30e9d2574c3bec5144347ff965f9841c8f06857f0b7383000f8c9489a161841` | `cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/600SemiBold/Inter_600SemiBold.ttf`                   |
| `Inter-Bold.ttf`              | Inter            | `c1c6ba111e8d04d392b741d194ab548186ec3c006ed7cc134be0525402520339` | `cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/700Bold/Inter_700Bold.ttf`                           |
| `JetBrainsMono-Regular.ttf`   | JetBrains Mono   | `b6b1ff4ddefe36d7f2a6174e1d001cab374e594519ee9049af028d577b64c5f5` | `cdn.jsdelivr.net/npm/@expo-google-fonts/jetbrains-mono@0.4.1/400Regular/JetBrainsMono_400Regular.ttf`    |

All five files are SIL OFL 1.1.

#### Download URLs (for re-fetching / pipeline reference)

The exact URLs the UC 07 commit was fetched from. Pinned at `@expo-google-fonts/...@0.4.1`; treat these as the canonical re-fetch source until the upstream-zip pipeline (recommended follow-up below) is in place. Re-running the same URLs and matching the sha256s above is the verification protocol.

- `https://cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/400Regular/Inter_400Regular.ttf`
- `https://cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/500Medium/Inter_500Medium.ttf`
- `https://cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/600SemiBold/Inter_600SemiBold.ttf`
- `https://cdn.jsdelivr.net/npm/@expo-google-fonts/inter@0.4.1/700Bold/Inter_700Bold.ttf`
- `https://cdn.jsdelivr.net/npm/@expo-google-fonts/jetbrains-mono@0.4.1/400Regular/JetBrainsMono_400Regular.ttf`

Upstream-equivalent endpoints, kept here so the eventual `scripts/fetch-fonts.{sh,ps1}` rewrite has a known target:

- `https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip` (32 MB combined zip; extract `Inter-{Regular,Medium,SemiBold,Bold}.ttf` from the `Inter Desktop` subdir).
- `https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip` (extract `fonts/ttf/JetBrainsMono-Regular.ttf`).

Companion license source:

- `https://raw.githubusercontent.com/google/fonts/main/ofl/inter/OFL.txt` — used for the verbatim SIL OFL 1.1 body shipped at `crates/app/assets/fonts/LICENSE.OFL.txt`. The file at `crates/app/assets/fonts/LICENSE.OFL.txt` adds both the Inter and JetBrains Mono copyright lines on top of that body.

**Note on source choice.** The proposal called for fetching from upstream's
official GitHub releases (`rsms/inter` v4.1 + `JetBrains/JetBrainsMono` v2.304).
The actual fetch was done from the `@expo-google-fonts/*` npm packages on
jsdelivr because (a) `rsms/inter` v4.1's release ships static TTFs only
inside a 32 MB combined `.zip`, infeasible on the bandwidth-constrained
link in use during this UC; (b) `rsms/inter`'s `docs/font-files/` only
ships WOFF2 in v4.1, which Slint does not consume; (c) Google's mirror
inside the `@expo-google-fonts` packages is the same upstream Inter /
JetBrains Mono font binaries, individually downloadable per weight. The
OFL covers either origin identically. Re-pinning to the upstream zips
is a follow-up the release pipeline should do once stable bandwidth is
available — recommended path is a `scripts/fetch-fonts.{sh,ps1}` mirroring
the existing `scripts/fetch-yt-dlp.{sh,ps1}` shape, run on each CI runner
rather than committing the binaries (not yet written).

A copy of the SIL OFL 1.1 license text lives at
`crates/app/assets/fonts/LICENSE.OFL.txt` (source-tree copy, NOT separately
shipped via installer configs — the existing PolyForm-Noncommercial bundling
already covers `crates/`). Attribution is appended to `LICENSE` under a
"Bundled fonts" section.

**Rationale.** System-font fallback was the alternative considered. It ships
nothing extra but produces visually different output per OS — unacceptable
for a design-system effort that explicitly aims for a deterministic look. The
~500–700 KB cost (5 ttfs combined) fits comfortably under the project's
<50 MB installer budget recorded in `PROJECT_BRIEF.md` § Performance budgets.

**Why the four Inter weights but only Regular for JetBrains Mono.** Inter is
used for body, badges, button labels (Medium / SemiBold), and primary
(Bold/SemiBold). JetBrains Mono is used only for the URL row in
`RowDelegate` and the `KbdHint` primitive — both at a single weight. UC 11
will revisit if mono italics or bold are needed.

**`cargo-deny`** is currently configured to scan crate licenses; bundled font
assets sit outside that policy. The OFL-1.1 attribution requirements are met
by the source-tree LICENSE files and the appendix in `LICENSE`. If
asset-license scanning is added later, the OFL files should be allowlisted.

### 3. File layout — `crates/app/ui/design/`

**Decision.** All design-system Slint sources live under
`crates/app/ui/design/`:

- `tokens.slint` — the `DesignTokens` global (colors, lengths, font names).
- `components.slint` — primitives (`Button`, `Input`, `Select`, `Badge`,
  `KbdHint`).

`main_window.slint` imports from `design/tokens.slint` and continues to focus
on layout. UC 08–11 will add a thin layer of `app/feature/<feature>.slint`
files that import from both `design/components.slint` and (where needed)
`design/tokens.slint`.

**Rationale.** Clear separation of concerns and a single well-known root
import point. Avoids the "everything in one .slint" trap that becomes
impossible to navigate as primitives accumulate.

### 4. Design source of truth

**Decision.** `design/project/tokens.css` is the canonical reference. Any
visual change to the palette starts there; `tools/oklch-to-srgb.js` is then
re-run and the resulting hex values pasted into `tokens.slint`.

**Rationale.** A single human-readable file with the OKLCH spec, both
themes, and full CSS context (variants, `[data-theme="dark"]` overrides) is
easier to review than a Slint global where ternaries fan out across 40+
properties. The OKLCH-spec comments next to each Slint color make the link
auditable in either direction.

### 5. Theme persistence — `theme` KV row

**Decision.** A new `theme` row in the existing `settings` table stores
`"light"` | `"dark"` | `"system"`. First-launch default is `system`,
resolved at startup against `window.window().color_scheme()`. Once the user
toggles, the explicit value (`light` / `dark`) is persisted and system mode
is exited permanently. **No migration file** — KV-table reads default when
the key is absent.

**Rationale.** Mirrors the existing pattern used for `concurrency_cap`,
`format_pref`, `dest_dir`, `cookies_browser`, `deno_warning_dismissed`. A
dedicated migration would create more friction than value for a single row
in an already-existing KV table.

### 6. Toggle UI — text glyph for v0

**Decision.** The moon/sun toggle in the add bar uses a Unicode glyph
(`☀` / `🌙`) for v0. UC 08 (queue re-skin) is expected to swap in a vector
icon (Slint inline `Path` or a small SVG asset) as part of the broader
visual polish pass.

**Rationale.** The JSX reference (`design/project/app.jsx:222-226`) uses an
`<Icon name="moon|sun" />` component whose underlying SVG path data is not
checked into this repo. Porting that icon set is a UC 08 concern; for UC 07
the glyph keeps the toggle visible and clickable without expanding scope.

## Consequences

- Re-skinning UC 01 visuals (`RowDelegate`, popup, settings panel, deno
  banner, flash bar) is deferred to UC 08; AC#10 requires UC 07 leaves UC 01
  visually unchanged.
- Adding new colors to `tokens.css` is a 3-step process: edit the CSS, run
  `tools/oklch-to-srgb.js`, paste the output into `tokens.slint`. The lint
  rule preventing OKLCH expressions from appearing in Slint source is
  enforced by code review (no automated check today).
- A future "subset Inter to Latin/Latin-Ext" optimization can shave another
  ~500 KB off the installer if needed; out of scope for UC 07.
- Adding cargo-deny coverage for asset licenses would catch a stray
  GPL-licensed font drop; not configured today.

## Alternatives considered

- **Runtime OKLCH conversion in Rust.** Rejected — adds a dependency, costs
  per-re-theme, no benefit. The OKLCH spec is preserved as a comment for
  audit; that is enough.
- **System-font fallback (no bundling).** Rejected — non-deterministic look
  per OS, kills the design-system value proposition.
- **A `Theme` enum stored as the dynamic property** rather than a `bool`.
  Rejected — `dark-mode` flips faster (single boolean property in Slint), and
  the persisted KV row already carries the richer three-state preference.
- **One Slint global per family** (separate `Colors`, `Type`, `Radii`).
  Rejected — adds import noise without buying anything; one global with
  ~40 properties is fine at this size.

## Addendum (UC 11)

- **Dashed borders unsupported in Slint 1.16.1.** `Rectangle.border-style` does
  not exist; only solid borders are rendered. The UC 11 ad-slot placeholder,
  which the design canvas calls for as a dashed `border-strong` rectangle,
  degrades to a 1-px solid `border-strong`. Re-evaluate when a future Slint
  release exposes a stroke-style property.

## Addendum (UC 13)

- **Icon-size tokens + `Icon` primitive.** Two-tier `icon-size` (14 px /
  12 px) added to `DesignTokens`; new non-interactive `Icon` primitive
  alongside `Button`; `SourceIcon` rebased on `Icon` and moved to its own
  file to break a Slint import cycle; ad-hoc `Image` glyphs migrated
  app-wide (with two deliberate raw-`Image` carve-outs for feature
  illustrations); `_ComponentSmoke` promoted from a `Rectangle` to a
  fixed-size `Window` and gated by a pixel-diff snapshot test. See
  ADR 0009.
