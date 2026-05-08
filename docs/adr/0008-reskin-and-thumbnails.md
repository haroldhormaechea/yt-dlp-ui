# 0008 — UC 08 reskin and thumbnail pipeline

## Status

Accepted (UC 08, 2026-04-30).

## Context

`PROJECT_BRIEF.md` § Architecture pins the UI to Slint 1.16.1 and design tokens
ported from `design/project/tokens.css`. UC 07 shipped the `DesignTokens`
global and primitive components (`Button`, `Input`, `Select`, `Badge`,
`KbdHint`). UC 08 is the visible re-skin: the main shell (add bar, queue list,
footer, empty state) and the per-row delegate consume those primitives,
match the design's seven row states, and ship a thumbnail pipeline (gradient
placeholder + on-disk cache + background fetcher).

## Decisions

### Layout split

`crates/app/ui/main_window.slint` keeps the window-level wiring (queue
model, settings panel, bot-check popup, deno banner, flash). The rest is
extracted into focused component files:

- `add_bar.slint`
- `queue_row.slint` (and the canonical `QueueRow` struct, re-exported from
  `main_window.slint`)
- `footer.slint`
- `empty_state.slint`
- `thumbnail.slint`
- `icons.slint` (central `Icons` global + `SourceIcon` helper)

Slint structs are not duck-typed across files — the canonical `QueueRow`
struct is owned by `queue_row.slint` and re-exported, so the Rust side
sees a single generated type.

### `Button` extension

UC 07's `Button` primitive gained an `in property <image> icon`. When set,
the button's `HorizontalLayout` prepends a 12×12 `Image { colorize }`
followed by a 5-px gap shim. For `modifier == "icon"` / `"icon-sm"` the
icon is the only content. This is the natural follow-up to UC 07's
deliberately deferred icon work (it shipped Unicode glyph stand-ins).

### Strikethrough fallback

The proposal targeted Slint's `text-decoration-line: line-through` for the
cancelled-row title style. A compile-only `_StrikethroughSmoke` was added
to `components.slint` to fence the assumption and produced this verbatim
build error on Slint 1.16.1:

> `error: Unknown property text-decoration-line in Text`

UC 08 falls back to overlaying a 1-px Rectangle inside the title's
wrapping Rectangle in `queue_row.slint`. Re-test on a future Slint
version by re-introducing the smoke. The compile-only smoke was removed
once the fact was captured here so it doesn't block future builds; a
docblock pointer remains in `components.slint`.

### Other Slint 1.16.1 quirks hit and worked around

- `image.width` is an `int` (raw pixels), not a `length` — comparisons
  cannot use `0px`. The `if root.icon.width > 0 :` guard in `Button` uses
  the int form.
- `image-fit: tile` is unsupported. The shimmer overlay uses
  `image-fit: fill` against an SVG `<pattern>` that already tiles
  internally, so the visual is preserved.
- `Image` already declares `source` as a property, so an `inherits Image`
  helper component cannot redeclare it. `SourceIcon` exposes its input as
  `source-kind` and binds `source` internally.

### Thumbnail pipeline (hybrid)

- **Placeholder:** `thumbnail.slint` renders a gradient card whose palette
  is selected by `seed % 8` from a 32-bit FNV-style hash of the row URL,
  overlays a diagonal-stripe SVG (theme-aware), a top-left source-icon
  glyph, and a faint center play glyph.
- **Playlist branch:** `crates/yt-dlp-bridge/src/metadata.rs::PlaylistEntry`
  gained `thumbnail: Option<String>`. Many extractors leave it `None` even
  with `--flat-playlist` — those rows fall back to the gradient
  placeholder until a separate refresh succeeds. **Documented as expected
  behaviour.**
- **Single-video branch:** new `pub async fn get_thumbnail_url` mirrors
  `get_title` (`yt-dlp --skip-download --print %(thumbnail)s
  --no-playlist`). Called inline from `add_url`'s single-video branch
  BEFORE the per-row task spawns. The per-row task itself never spawns
  yt-dlp — it consumes the pre-resolved URL.
- **Per-row task (`spawn_thumbnail_fetch`):** plain reqwest GET → write
  `<app-data>/thumbnails/<sha1(url)>.<ext>` → DB write → emit
  `UiEvent::ThumbnailReady { id, path }`. Errors log at WARN; gradient
  placeholder remains.
- **Restart behavior:** `requeue_pending_thumbnail_fetches` queries
  `WHERE thumbnail_path IS NULL` and re-issues the single-video resolution
  path for each row. **Known startup behavior:** N rows with NULL
  thumbnail = N yt-dlp spawns at startup. Bounded but visible. The source
  URL is intentionally not cached because signed CDN URLs expire across
  restarts.
- **Aspect-ratio policy:** `image-fit: cover` (center-crop). YouTube-style
  16:9 fits the slot; shorts (9:16) and 4:3 Vimeos lose top/bottom or
  side strips, which matches the design's intent (a uniformly-sized
  thumbnail card is more important than preserving framing).
- **Cache eviction:** none today (write-only). Recorded as a
  scaffolding-gap follow-up.

### Bridge `Progress` / `Finished` widening

`DownloadEvent::Progress` gains `downloaded_bytes: Option<u64>` and
`total_bytes: Option<u64>` (both `Option` because yt-dlp emits `NA` on
live streams or pre-metadata). `DownloadEvent::Finished` gains
`bytes: Option<u64>`, snapshotted bridge-side from the last-seen
`total_bytes` in the streaming stdout loop. Zero extra I/O. If no
progress line ever carried a known total, `bytes` is `None` and the
done-state mono line gracefully drops the size. The existing
`DownloadRequest` shape and the `start` entry-point signature are
unchanged.

### DB

Single new migration step (`0002_uc08_thumbnails_and_bytes.sql`):

```sql
ALTER TABLE queue_items ADD COLUMN thumbnail_path  TEXT;
ALTER TABLE queue_items ADD COLUMN size_bytes      INTEGER;
ALTER TABLE queue_items ADD COLUMN downloaded_bytes INTEGER;
```

`queue::update_progress` widens to `(c, id, pct, speed, eta,
downloaded_bytes, total_bytes)`. `queue::set_finished` is new and stamps
both `size_bytes` and `downloaded_bytes` (so the done row reads 100 %).
For `Cancelled` and `Error`, the existing `update_status` /
`set_error_msg` calls leave `downloaded_bytes` intact — the last
`Progress` event already wrote the correct value.

### Footer counts

A single helper `ui_bridge::recompute_counts` walks the Slint model and
sets the four count properties. Called from the `RowUpserted` and
`RowRemoved` apply arms. `cap` is set once in `lib.rs::run_ui` from
`Settings::concurrency_cap`.

### Action signal contract

`queue_row.slint::QueueRowDelegate` emits `start-clicked(int)`,
`cancel-clicked(int)`, `remove-clicked(int)`, `restart-clicked(int)`.
`main_window.slint::MainWindow` keeps `start-one(int)` for UC 01 and
adds `cancel-clicked` / `remove-clicked` / `restart-clicked`. UC 02 will
replace the `tracing::info!` placeholder bodies in
`ui_bridge::wire_callbacks` with real `DownloadManager` calls without
renaming.

### Cancelling state

`QueueStatus` (Rust) is unchanged. Slint markup handles
`status == "cancelling"` (disabled `Cancelling…` button + cancelling
badge). UC 08 never produces it; UC 02 introduces the transition.

### Minor brief drift

- The brief pins reqwest 0.13.2 with feature `rustls-tls`. reqwest 0.13.x
  renamed that feature to `rustls`. The `default-features = false` +
  rustls combination still avoids OpenSSL exactly as the brief intends —
  only the feature flag name has changed. Surfaced inline in the
  workspace `Cargo.toml`.
- The proposal listed the shimmer overlay assets as `.png`; actual
  implementation ships `.svg` (`<pattern>`-based) because the developer
  cannot author binary PNG content from text edits, and SVG is fully
  supported by Slint's `Image::source`.

### Tooltip deviation (AC #5)

The use case calls for a tooltip carrying the raw URL on the mono URL
row. Slint 1.16.1 has no tooltip primitive. UC 08 renders the URL row
as plain `Text { overflow: elide }`; tooltip support is deferred to a
future UC.

## Consequences

- New runtime dependencies: `reqwest` is now part of the `app` crate's
  direct deps (it was already in the workspace).
- Schema version bumps to 2; existing `db.sqlite` files migrate
  automatically on first launch.
- The bridge's `DownloadEvent` enum gained two fields and a third on
  `Finished`. All bridge consumers in this workspace are updated; out-of-
  tree consumers (none today) would need to add `..` to existing match
  patterns.
- The thumbnail fetcher does N upstream resolution subprocesses at
  startup for restored-pending rows — bounded but visible. Acceptable
  given the existing UC 01 startup cost (concurrent title fetches do the
  same thing).
- The strikethrough overlay fallback adds a small Rectangle to every
  cancelled row. Rendering cost is negligible; visual fidelity is close
  to native `text-decoration-line` at typical viewing distances.
