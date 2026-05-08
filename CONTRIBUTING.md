# Contributing to yt-dlp-ui

This file covers the desktop UI (Rust workspace) and the installers that
ship around it. The bundled `yt-dlp` and `ffmpeg` binaries are fetched at
build time from upstream releases and verified per
`scripts/runtime-deps-pins.env` and `scripts/fetch-*.{sh,ps1}`; we never
modify upstream's source.

## Hard rules

1. **Bundled binaries are pulled in by the build, not committed.** Edit the
   pin in `scripts/runtime-deps-pins.env` and the verifier under
   `scripts/fetch-*.{sh,ps1}` (and the matching `bats` / `pwsh` tests under
   `scripts/`) when bumping a bundled binary. The upstream public key for
   yt-dlp lives at `scripts/keys/yt-dlp.asc`; the build pipeline fails
   loudly if signature or SHA verification does not match.

2. **All new code lives under `crates/`.** Three workspace crates:
   - `crates/app` — main UI process (Slint frontend, tokio runtime, SQLite
     queue, ad-window orchestration, download manager).
   - `crates/ad-window` — separate binary running a `wry`/`tao` WebView for
     ads. Spawned and killed by `app` on demand to keep idle RAM low. Has no
     dependency on the main app crate or the bridge crate.
   - `crates/yt-dlp-bridge` — typed wrapper around the bundled `yt-dlp`
     standalone binary. UI-free; depends only on `tokio` (minimal features),
     `serde`, `tracing`, `thiserror`. Stays unit-testable in isolation.

3. **License.** UI code is licensed under PolyForm Noncommercial 1.0.0
   (see repo-root `LICENSE`). This is **source-available, not OSI
   open-source.** Forks, redistribution, and modification are permitted;
   commercial use is not. Forks may strip the ad slot but may not add their
   own ads or paid features and redistribute. The bundled `yt-dlp` binary
   retains its upstream Unlicense terms (shipped as
   `installer/yt-dlp-LICENSE.txt`).

4. **No `unsafe`.** The workspace lints table forbids `unsafe_code` at the
   workspace root. Direct dependencies (`slint`, `wry`, `tao`, `rusqlite`,
   `reqwest`, `tokio`) already encapsulate the unsafe code they need.

## Local development

### Toolchain

`rustup` with the channel pinned by `rust-toolchain.toml` (currently
`1.95.0`, Rust 2024 edition). Components: `rustfmt`, `clippy`,
`llvm-tools-preview`. `rustup` will install the right toolchain
automatically on the first `cargo` invocation in this directory.

### Per-OS native dependencies

The `ad-window` crate depends on `wry`, which uses the system WebView. Each
OS needs a few native packages.

**Linux (Debian / Ubuntu):**
```sh
sudo apt install -y \
  build-essential \
  pkg-config \
  libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev \
  libssl-dev
```

Equivalent for Fedora-family distros:
```sh
sudo dnf install -y \
  gcc gcc-c++ make \
  pkgconf-pkg-config \
  webkit2gtk4.1-devel \
  libsoup3-devel \
  openssl-devel
```

**macOS:**
```sh
xcode-select --install
```
That's it — system WebKit is used automatically by `wry`.

**Windows:**
- WebView2 Runtime (ships with modern Edge / Windows 11; on older Windows 10
  install the Evergreen redistributable from
  https://developer.microsoft.com/microsoft-edge/webview2/).
- Visual Studio Build Tools 2022 for the MSVC toolchain (`rustup` prompts
  for this during install).

### Common commands

A `Justfile` ships at the repo root. Install `just` (`brew install just` /
`cargo install just` / `winget install --id Casey.Just`) and run:

```sh
just                    # default: lint then test
just run                # cargo run --bin app
just test               # cargo test --workspace
just lint               # cargo clippy --workspace --all-targets -- -D warnings
just fmt                # cargo fmt --all
just audit              # cargo audit
just deny               # cargo deny check
just coverage           # cargo llvm-cov --workspace --html
just adwin              # cargo run --bin ad-window
just fake-ad-server     # cargo run --example fake-ad-server
just fetch-runtime-deps # UC 17 — pull / build bundled ffmpeg for the host
```

Or run any of those `cargo` commands directly if you prefer.

### `YT_DLP_UI_FETCH_DEPS` opt-out (UC 17)

`crates/app/build.rs` auto-fetches `runtime-deps/ffmpeg` on Unix dev hosts
when it is missing, so `cargo run` works on a fresh clone without an
explicit `just fetch-runtime-deps`. Set `YT_DLP_UI_FETCH_DEPS=0` to opt out
of the auto-fetch (offline / air-gapped builds, or simply if you prefer
to control fetches explicitly):

```sh
YT_DLP_UI_FETCH_DEPS=0 cargo run --bin app
```

With the opt-out set and no `runtime-deps/ffmpeg` present, downloads that
require ffmpeg will surface a clear error at the row level — `cargo build`
itself does not fail. **Release builds (cargo-dist pipeline) skip the
auto-fetch entirely** and rely on the explicit `package-*.yml` steps.

### Testing layout

- **Unit tests** — inline `#[cfg(test)] mod tests { ... }` blocks alongside
  the production code, plus `*_test.rs` files in `crates/*/src/**` when a
  test surface gets large.
- **Integration tests** — `crates/*/tests/` (Cargo's standard integration-
  test location). Each file is a separate binary linking the crate as an
  external library, forcing public-API testing.
- **Property-based tests** — used in `yt-dlp-bridge` for the progress parser
  via the `proptest` dev-dependency.
- **Smoke binary test** — CI test that spawns the `app` binary and asserts a
  clean exit. Coming once `app` actually does something useful.
- **No true UI automation at MVP.** Manual smoke per OS at release time.

Coverage target: 60% at MVP, ratcheting to 80% at production maturity. Run
`just coverage` to generate an HTML report under `target/llvm-cov/html/`.

### Quality gates

Before opening a PR:

```sh
just fmt
just lint
just test
just audit
just deny
```

CI runs the same set on every PR plus `cargo test` on macOS / Windows /
Linux runners.

## Where things go

| If you're adding... | Put it in... |
|---|---|
| A new UI screen | `crates/app/src/ui/` (`.slint` files) + `crates/app/src/views/` (Rust glue) |
| Queue / settings / history logic | `crates/app/src/db/` and `crates/app/src/queue/` |
| Subprocess / yt-dlp parsing logic | `crates/yt-dlp-bridge/src/` |
| Ad-window webview behavior | `crates/ad-window/src/` |
| A reusable example | `examples/` (referenced via `[[example]]` in the relevant `Cargo.toml`) |
| An architecture decision | `docs/adr/<NN>-<slug>.md` (MADR-style) |
| Anything Python | Don't. The Python tree is read-only third-party code. |

## Manual settings-panel smoke (UC 09)

The settings slide-in panel (UC 09) cannot be fully driven headlessly —
`rfd::AsyncFileDialog` opens an OS-native picker and Slint+rfd is hard to
exercise from a `cargo test` harness. UC 09 AC#23 explicitly accepts a
documented manual smoke as the right escape hatch. Run this checklist on at
least one host before merging UI-touching changes near the panel, and on all
three OSes at release time.

Steps (`just run`, then in the running app):

1. **Destination flow (rfd async picker).** Open the gear → General tab →
   click `Choose…`. The OS folder picker appears. Pick a folder. The
   destination row updates with the chosen path (mono, `$HOME → ~`,
   middle-ellipsis if long). Cancel the picker on a second attempt — the
   destination row must be unchanged.

2. **Three close affordances.** With the panel open:
   - Click anywhere on the translucent backdrop → panel closes.
   - Reopen, click the `×` button in the header → panel closes.
   - Reopen, press <kbd>Esc</kbd> → panel closes. <kbd>Esc</kbd> must NOT
     also close the main window (AC#6: ESC is consumed by the panel handler
     when the panel is open).

3. **Three-tab traversal.** Open the panel and click each of `General` /
   `Cookies` / `Privacy & Ads` in turn. Active tab gets the 2-px violet
   underline; inactive tabs render in `text-2`. Close the panel, reopen it
   in the same session — the last-active tab is restored. Restart the app —
   the panel reopens on `General` (AC#9: in-memory only, no KV key).

4. **Bot-check popup wording.** Trigger the bot-check dialog (e.g. queue a
   URL that yt-dlp gates). The browser list reads `Brave / Chrome / …` in
   title case (UC 09 rename), not the lower-case yt-dlp arg form.

5. **Theme flip with the panel open.** Open the panel. Click the moon/sun
   button in the add bar. The whole panel — backdrop, surface, borders,
   text, toggles, the stepper track gradient, and the disclosure block —
   re-themes in place (AC#20). No stale colors should remain.

If any step fails, file the deviation as a UC 09 regression — the panel's
acceptance criteria are still in scope.

## Manual bot-check modal smoke (UC 10)

The bot-check modal (UC 10) replaced UC 05's `slint::PopupWindow` with a
centered layered overlay. Headless smoke (`crates/app/tests/bot_check_modal_smoke.rs`)
pins construction, theme flip, and the open → close → reopen property cycle
that exercises the developer's `states […]` reset fallback (Slint 1.16.1
does not support `<prop>-changed(value) => …` on Rectangle). What that
headless suite cannot reach is the modal's internal `picked` / `remember`
state after a `closed` → `open-state` transition (Slint's testing backend
does not expose component-internal property reads in 1.16.1) and the actual
runtime entrance / dismissal interactions. Run this checklist on at least
one host before merging UI-touching changes near the modal, and on all
three OSes at release time.

Steps (`just run`, then in the running app):

1. **Render at affected-count = 1.** Queue exactly one yt-dlp URL that
   triggers the bot-check (`BridgeError::AuthRequired`) with no
   `cookies_browser` setting persisted. The modal opens, centered, with
   the singular header copy ("YouTube needs cookies to verify you're not
   a bot.") and NO trailing "This applies to N queued items." fragment.

2. **Render at affected-count = N (N ≥ 2).** Queue several URLs that all
   trigger the bot-check. The modal opens once, with the trailing fragment
   "This applies to <N> queued items." appended (count bolded in the
   `text` color). Subsequent rows hitting the bot-check while the modal is
   open update the count in place.

3. **ESC key = Cancel.** With the modal open, press <kbd>Esc</kbd>. All
   batched rows transition to `error` with the existing UC 05 tooltip
   ("YouTube blocked this download. Set a Cookies source in Settings to
   retry."). <kbd>Esc</kbd> must NOT also close the main window or the
   settings panel — the modal-precedence ESC routing in `main_window.slint`
   consumes the key when the modal is open.

4. **Backdrop click = Cancel.** Reopen the modal (queue another URL).
   Click anywhere on the dark translucent backdrop outside the panel.
   Same outcome as ESC: rows go to `error` with the UC 05 tooltip.

5. **Pick without Remember (no DB write).** Reopen the modal. Pick a
   browser without checking "Remember this choice", then click the
   primary `Use <browser>` button. Rows retry with
   `--cookies-from-browser <browser>`. Open Settings → Cookies tab — the
   `cookies_browser` dropdown still reads `None` (the choice was
   one-shot, not persisted).

6. **Pick with Remember (DB write).** Reopen the modal. Pick a different
   browser, check "Remember this choice", click `Use <browser>`. Rows
   retry. Open Settings → Cookies tab — the dropdown now reads the
   browser you picked. Restart the app — the setting persists.

7. **Session-default pre-selection.** With Remember unchecked in step 5
   above, trigger another bot-check in the same session. The modal opens
   pre-selected on the browser you picked last (held in memory only, the
   `bot-check-last-pick` window property — separate from the persisted
   `cookies_browser` setting). Restart the app, trigger again — the modal
   defaults back to the canonical-order first-detected browser.

8. **`states […]` reset fallback at the picked / remember level.** The
   developer fell back to a `states […]` block to reset `picked` and
   `remember` on each open because Slint 1.16.1 rejected the
   `<prop>-changed(value) => …` callback syntax. Verify the runtime
   behavior the headless suite cannot reach: open the modal, toggle the
   "Remember this choice" checkbox ON, then close (ESC or backdrop click).
   Reopen — the checkbox must be unchecked again. Click a different
   browser row than the default-pick (the radio indicator switches), then
   close and reopen — the radio must be back on the host-supplied
   default-pick (last-pick or canonical-order first-detected).

9. **Theme flip with the modal open.** Open the modal. Click the moon/sun
   button in the add bar. The backdrop opacity, panel surface, borders,
   header shield circle, browser-row dividers, gradient glyphs (per
   browser brand color), selected-row `accent-soft` background, the
   `accent` ring on the radio, and the primary button — all re-theme in
   place (AC#14). No stale colors.

10. **Zero-browsers fallback (UC 05 inheritance).** On a host with no
    detected browsers (or in an environment where `browsers::detect_installed`
    returns empty), trigger a bot-check. The modal must NOT render — a
    toast appears instead, per UC 05 AC#13. UC 10 inherits this contract
    unchanged.

If any step fails, file the deviation as a UC 10 regression — the modal's
acceptance criteria are still in scope.

## Manual UC 11 smoke (ad slot, deno banner, Toast)

UC 11 introduced three independent overlay surfaces with cross-cutting
visual + timing concerns. The headless suites (`crates/app/tests/toast_smoke.rs`
and `crates/app/tests/main_window_overlays.rs`) pin construction, the
front-evict-at-3 queueing model, and the id-based dismissal that survives
front-eviction. What those suites cannot reach is the actual rendered
geometry, the 200 ms cubic-bezier opacity animation, the 3 s `Timer`-driven
auto-dismiss, the diagonal-stripe `Image` swap on theme flip, the
`rotation-angle` "AD" label layout, and the `brew install deno` mono chip
inside the banner. Run this checklist on at least one host before merging
UI-touching changes near the bottom strip or the Toast component, and on
all three OSes at release time.

Steps (`just run`, then in the running app):

1. **Ad slot — light theme.** With light theme active and `focus_mode` off,
   the bottom 64 px region renders below the footer: `#eaeaee` background,
   1-px `border` top edge, vertical "AD" label on the left (rotated, 9-px
   JBM uppercase), 48-px stripe placeholder in the middle (light stripes
   `#d8d8de` / `#d2d2d8` via `Icons.stripe-light`), centered mono caption
   "ad slot · WebView render area · 728×48", "Focus" ghost button (eye icon
   + label) on the right.

2. **Ad slot — dark theme.** Click the moon/sun toggle. The slot re-themes
   in place: `#0d0d0f` background, dark stripes `#1a1a1d` / `#18181b` via
   `Icons.stripe-dark`, all foreground colors honor `DesignTokens.text-3`.
   No stale palette anywhere.

3. **Focus button hides the slot.** Click the "Focus" button on the right
   of the ad slot. The whole 64-px region disappears (the `if !focus-mode`
   branch in `main_window.slint`). Open Settings → Privacy & Ads — the
   "Focus mode" toggle reads ON. Toggle it OFF — the ad slot reappears.

4. **Deno banner appears when deno is missing.** On a host where neither
   the bundled deno binary nor a PATH `deno` is present (rename `~/.deno`
   or unset `PATH` in a scratch shell, then `cargo run`), a thin warning
   strip appears under the add bar: `warning-soft` background,
   `warning-text` foreground, info icon, message "Some YouTube downloads
   may require Deno. `brew install deno` (or platform equivalent).". The
   `brew install deno` token renders in JetBrains Mono with the
   `rgba(0,0,0,0.06)` chip background, 1×5 padding, 3 px radius.

5. **Banner × dismisses for the session only.** Click the small ghost ×
   on the right of the banner. The banner disappears and stays gone for
   the rest of the session (no other interactions bring it back). Quit
   the app and relaunch with deno still missing — the banner reappears
   on next launch (session-only dismissal, no KV key written; `db.sqlite`
   `settings` table contains no `deno_warning_dismissed` row at any
   point).

6. **Banner does NOT appear when deno is found.** With deno on PATH (or
   bundled), the banner never shows. Verify by `which deno` returning a
   path before launching, then confirming the strip is absent.

7. **Cancel-all toast — non-empty queue.** Add at least one URL to the
   queue (URL doesn't need to actually download, just be in `queued` or
   `in_flight`). Click "Cancel all" in the footer. An info-kind toast
   "Queue cancelled." appears bottom-center, 80 px above the window edge
   (above the ad slot), 8×14 padding, 6 px radius, `text` background,
   `bg` foreground, `shadow-md`. It fades in over ~200 ms and
   auto-dismisses after 3 s.

8. **Cancel-all toast — empty queue.** With the queue empty, click
   "Cancel all". NO toast appears (the gating in `ui_bridge.rs`'s
   `on_cancel_all` snapshots `had_work` and only fires the toast when
   non-empty).

9. **Add-failure toast.** Paste a syntactically invalid URL (e.g.
   `not-a-url`) into the add bar and submit. A danger-kind toast
   "Failed to add URL(s)." appears (warning palette `danger-soft` /
   `danger-text`).

10. **No "Settings saved." toast.** Open Settings, change the concurrency
    cap, theme, or any other setting. NO toast fires (UC 09 AC 17: the
    settings panel persists silently per change).

11. **Toast queue caps at 3 — oldest evicted on overflow.** Quickly fire
    four toasts back-to-back: paste 4 invalid URLs in 4 separate add-bar
    submissions before any of the first three's 3 s timer elapses. The
    bottom-most three toasts are the LATEST three; the first one
    disappears immediately on the 4th's arrival (no animation queue,
    just a model removal). Stack layout: 40 px between successive
    toasts, newest at the bottom.

12. **Auto-dismiss after 3 s.** Fire one toast (e.g. invalid URL).
    Without interacting, watch it disappear ~3 s later via the same
    200 ms cubic ease-in fade as entrance. The Slint-side `Timer` inside
    the `Toast` component drives this; the host's id-based scan in
    `on_dismiss_toast` removes the entry from the model when the timer
    fires.

13. **Z-order — toasts behind settings panel and bot-check modal.** Fire
    a toast (e.g. cancel-all on a non-empty queue), then immediately open
    Settings (`,` shortcut or the gear). The settings panel's surface
    must paint OVER the toast (UC 09 panel beats UC 11 toast). Close the
    panel — the toast reappears in front of the queue list. Same with
    the bot-check modal (UC 10) if you can trigger one with a toast
    visible.

14. **Theme flip with all three surfaces visible.** Trigger the deno
    banner (deno missing), fire a toast (invalid URL), keep the ad slot
    visible (focus mode off). Click the theme toggle. All three surfaces
    re-theme in place: ad-slot background and stripe `Image` swap,
    banner `warning-soft` / `warning-text` swap, toast `text` / `bg`
    swap. No stale colors on any of the three.

If any step fails, file the deviation as a UC 11 regression — the
ad-slot, deno-banner, and Toast acceptance criteria are still in scope.

### Manual smoke for UC 15 (queue scroll)

UC 15 fixed the queue list to scroll vertically — mouse wheel, scrollbar
drag, and Up/Down arrow keys all move the viewport one row at a time. The
headless suite (`crates/app/tests/queue_scroll.rs`) pins construction with
many rows, the empty-queue branch, and that model push/remove sequences
do not panic. What it cannot reach is the actual scroll geometry,
keyboard focus routing, or scrollbar visibility — Slint 1.16.1 does not
expose `ListView::viewport-y` / `visible-height` on the generated public
Rust API. Run this checklist on at least one host before merging
queue-list-touching changes, and on all three OSes at release time.

Steps (`cargo run --bin app`, then in the running app):

1. **Mouse wheel scrolls (AC #1).** Open the app and queue 12+ URLs from
   `examples/sample-urls.txt` via the AddBar (paste-and-add, one URL per
   line, or repeated single-URL adds). With 12+ rows visible, hover the
   queue area and spin the mouse wheel — the list moves. Wheel-up at
   the top and wheel-down at the bottom clamp without overshoot.

2. **Scrollbar visible and draggable (AC #2).** With the same overflowing
   queue, a vertical scrollbar appears next to the list, reflecting the
   current scroll position. Drag the scrollbar thumb — the viewport
   tracks the drag, and releasing the thumb leaves the list at the
   dragged position.

3. **Arrow keys scroll one row (AC #3).** Click anywhere inside the
   queue area to give it focus. Press Down — the viewport moves down
   one row's worth (90 px). Press Up — it moves back up one row.
   Holding either key auto-repeats. Other keys (Left, Right, letters)
   are ignored by the queue's `FocusScope` and do not consume focus
   from sibling controls.

4. **Resize add/remove scrollbar (AC #4).** Drag the window vertically
   larger until all rows fit — the scrollbar disappears, layout does
   not break mid-resize. Drag smaller again — the scrollbar reappears
   without flicker, the viewport stays at a sensible position.

5. **Add row while scrolled — no jump (AC #5).** Scroll partway down a
   12+ row queue (so neither top nor bottom is visible). Paste a new
   URL into the AddBar and submit. The new row appears at the tail of
   the list; the viewport stays where it was — it does NOT jump to the
   new row, the top, or the bottom. Note the visible top row before
   adding; it must be the same visible top row after.

6. **Remove rows — auto-clamp (AC #6).** With a 12+ row queue, scroll
   to the bottom. Cancel-all (or cancel + remove enough rows to make
   the new content height shorter than the previous viewport top).
   The viewport must auto-clamp — no sustained scroll past the new
   content end, no empty space below the last row.

If any step fails, file the deviation as a UC 15 regression. Pitfalls
specific to this fix: a `min-height` / `max-height` regression on
`QueueRow`'s outer `HorizontalLayout` (UC 15 introduced `min-height:
90px`) would shift row heights and break AC #3's "one row at a time"
assumption; a regression that drops the `FocusScope` or the
`changed viewport-height => clamp-y(...)` callback would surface as
AC #3 (no arrow-key scroll) or AC #6 (sustained over-scroll on
removal).

### Manual smoke for UC 14 (Start all broaden)

UC 14 broadened the footer's "Start all queued" button to "Start all"
and made it pick up `cancelled` and `error` rows too — same per-row
start handler, no separate "Resume all" button, no confirmation modal,
yt-dlp's `--continue` covers `.part`-file resume identically to per-row
Restart. The headless suite covers the bulk-reset SQL semantics
(`download_mgr_test.rs`) and the enable-predicate / tooltip pure-helper
matrices (`ui_bridge.rs` inline tests). What it cannot reach is the
Slint-side label, hover tooltip rendering, or the disabled-while-busy
visual transition. Run this checklist on at least one host before
merging Start-all-touching changes, and on all three OSes at release
time.

Steps (`cargo run --bin app`, then in the running app):

1. **Seed a queue covering every status (AC #1, #4).** Paste 6+ URLs
   from `examples/sample-urls.txt` via the AddBar. As they land:
   - Cancel two of the in-flight rows mid-download (per-row Cancel) so
     they end at `cancelled` with `.part` files on disk.
   - Feed two deliberately-bad URLs (e.g. `https://example.com/404`)
     so they end at `error`.
   - Leave at least two rows in the `queued` state by setting the
     concurrency cap to 1 (Settings panel) so a backlog forms.
   - Optionally let one or two rows complete to `done`.
   You should have visible rows in `queued`, `in_flight`, `cancelled`,
   `error`, and `done` simultaneously.

2. **Footer button reads "Start all" (AC #1).** With the mixed-state
   queue above, the footer's primary action button reads "Start all"
   (not "Start all queued"). The play icon is the same one that was on
   the pre-UC-14 button.

3. **Tooltip breakdown (AC #11).** Hover the Start-all button. A
   tooltip appears with the comma-joined breakdown — e.g.
   `"2 queued, 2 cancelled, 2 error"`. Each segment is omitted when
   its count is zero: only-queued shows `"<N> queued"`, only-error
   shows `"<K> error"`, etc. Verify by manually clearing one of the
   states (× the cancelled rows or the error rows) and re-hovering —
   the tooltip drops the cleared segment.

4. **Click Start all — mixed-state resumes (AC #3, #5).** With the
   queue from step 1, click Start all. Observe:
   - The two `cancelled` rows resume from their `.part` files (yt-dlp
     `--continue` picks up where it left off; you should see progress
     start from a non-zero percentage if the partial was substantial).
   - The two `error` rows restart fresh (progress from 0%, error
     message clears).
   - The pre-existing `queued` rows promote in turn.
   - `in_flight`, `done`, and any `cancelling` rows are NOT touched —
     they retain their original status and progress.

5. **Concurrency cap honored (AC #2, #6).** With cap=1 (Settings →
   concurrency cap), only one row at a time runs after Start all;
   the rest stay `queued` and promote in order as each finishes.
   Set cap=3 and repeat: exactly three rows go in_flight at once.

6. **Busy gate surfaces "Nothing to start" tooltip (AC #7, #8).**
   While the bulk-SQL window is in flight (briefly visible right
   after the click — usually a single frame), the Start-all button
   is disabled and the tooltip reads `"Nothing to start"`. If the
   queue is fast enough that you cannot catch the busy frame, this
   step is covered by `compute_start_all_tooltip`'s busy=true matrix
   in the headless suite. Re-firing the button during the busy
   window does nothing — no double-spawn, no console warning escapes
   the `start_all clicked while busy; ignoring` debug log path.

7. **Per-row Restart on a cancelled row still works (AC #9).** With
   a fresh `cancelled` row in the queue, click its per-row Restart
   button. The row resumes (same `--continue` path); UC 02's per-row
   semantics are unchanged.

If any step fails, file the deviation as a UC 14 regression. Pitfalls
specific to this fix: a regression in the enable predicate (a stale
`queued-count > 0` check rather than the broadened
`queued + cancelled + error > 0`) would manifest as the button being
disabled when only cancelled / error rows remain (step 4 fails); a
regression in the bulk-reset transaction inside `start_all` could
leave some rows in their original `cancelled` / `error` state while
others reset (step 4 fails partially); a regression that drops the
busy gate would re-enable the button mid-flight and admit a
double-click race (step 6 fails).

### Manual smoke for UC 18 (About dialog)

UC 18 added an About modal that consolidates the project license
(PolyForm Noncommercial 1.0.0), upstream yt-dlp's Unlicense, deno's MIT,
ffmpeg's LGPL-2.1-or-later (with the LGPL § 4 source notice), and the
embedded fonts' OFL into a single discoverable surface. The headless
suites (`crates/app/src/about_test.rs` for the entry contract and
`crates/app/tests/about_modal_smoke.rs` for the modal subtree
construction) pin the version pin, the entry name set, the license-text
include_str! plumbing, the ffmpeg source-notice, the open ↔ close
property cycle, and theme flip with the modal mounted. What those suites
cannot reach is the actual rendered geometry of the centered card, the
backdrop animation, the License-detail `ScrollView` viewport, the
runtime Esc / Close-button / backdrop-click dismissals, the Settings
panel's "About yt-dlp-ui" row click, and the `open` redirect for the
ffmpeg source URL — all of which require the live event loop and a real
display. Run this checklist on at least one host before merging
About-dialog-touching changes, and on all three OSes at release time.

Steps (`just run`, then in the running app):

1. **Open from Settings → "About yt-dlp-ui" (AC #9).** Click the gear
   icon to open the Settings panel. Scroll to the bottom of the panel —
   below the last tab content there is a divider then a row labeled
   "About yt-dlp-ui" with the small info icon on its left. Click the
   row. The About modal opens above the panel, centered, with a dark
   translucent backdrop.

2. **Header shows the app version (AC #2).** The header reads "About
   yt-dlp-ui" with a sub-line "Version 0.5.0". The version string must
   match the workspace `Cargo.toml` `[workspace.package].version` (and
   `env!("CARGO_PKG_VERSION")`); a mismatch means a future version bump
   forgot to propagate to the dialog.

3. **Bundled-software list (AC #3, #4, #5, #6, #7).** Below the header,
   verify that all six entries render in this order:
   - `yt-dlp-ui` · `0.5.0` · `PolyForm Noncommercial 1.0.0`
   - `yt-dlp` · `<pinned version>` · `Unlicense`
   - `deno` · `<pinned version>` · `MIT`
   - `ffmpeg` · `<pinned version>` · `LGPL-2.1-or-later`
   - `Inter` · `Variable` · `SIL OFL 1.1`
   - `JetBrains Mono` · `Variable` · `SIL OFL 1.1`

   Each row has a "View full license" ghost button on the right.

4. **License full-text view is scrollable (AC #8, #13).** Click "View
   full license" on the `PolyForm Noncommercial 1.0.0` row. The body
   replaces with the full PolyForm text in mono, inside a ScrollView
   sized between 320 px and 480 px tall. Scroll the body up and down —
   the text moves; the surrounding card chrome (header, footer, "←
   Back" row) does not.

5. **Back navigation.** Click "← Back". The body returns to the
   bundled-software summary. Reopen the modal (Close + Settings → About
   again) and confirm the body resets to the summary on each open
   (covered structurally by the `states […]` reset in
   `about_modal.slint:46-56`).

6. **ffmpeg source notice + clickable URL (AC #6, #12).** Click "View
   full license" on the `ffmpeg` row. Below the LGPL body, a callout
   box reads:
   `Source available at: https://ffmpeg.org/ — see scripts/build-ffmpeg-macos.sh for the rebuild recipe`
   with a clickable `https://ffmpeg.org/` link styled in
   `accent-text`. Click the link — your default system browser must
   open `https://ffmpeg.org/`. The link must NOT open inside the app.
   Verify the same callout is absent on every other entry (AC #6
   counter-side).

7. **Three close affordances (AC #11).** With the modal open:
   - Press <kbd>Esc</kbd> — modal closes; the Settings panel beneath
     stays open. <kbd>Esc</kbd> must NOT also close the Settings panel
     or the main window in this stacked state (the modal-precedence
     ESC routing in `main_window.slint:138-148` consumes the key when
     About is open).
   - Reopen via the Settings "About" row. Click the `×` button in the
     header — modal closes.
   - Reopen. Click the dark translucent backdrop outside the card —
     modal closes via the same `close-clicked` callback.

8. **Modal pattern — centered, backdrop, design tokens (AC #10).** With
   the modal open, the card is horizontally and vertically centered in
   the window. The backdrop covers the entire window and renders at
   ~42 % opacity (`#0a0a0f6b`). Card surface uses `DesignTokens.surface`,
   border `DesignTokens.border`, divider lines `DesignTokens.divider`,
   accent text `DesignTokens.accent-text`. Resize the window — the
   modal stays centered.

9. **Theme flip with the modal open (AC #10, #14).** Open the modal.
   Click the moon/sun toggle in the add bar (the modal's backdrop will
   not block this if you click the toggle position blindly — the
   toggle lives above the modal's backdrop in the z-order, but if the
   add bar is unreachable, close the modal, flip theme, reopen). The
   card surface, borders, dividers, body text, accent link colors, and
   the source-notice callout all re-theme in place. No stale colors
   anywhere.

10. **No regression to UC 09 / 10 / 11 / 13 / 15 (AC #14).** Close the
    modal. The Settings panel underneath is unchanged (UC 09). Trigger
    a bot-check by queueing a YouTube URL that yt-dlp gates — UC 10's
    modal layers above About if you re-open About first. The deno
    banner (UC 11), Toast (UC 11), icon assets (UC 13), and queue
    scroll (UC 15) all behave as before.

If any step fails, file the deviation as a UC 18 regression — the
About-dialog acceptance criteria are still in scope. Pitfalls specific
to this fix: a regression in `about::APP_VERSION` (e.g. an inline
literal sneaking in instead of `env!("CARGO_PKG_VERSION")`) would
manifest as step 2 showing the wrong version after a future bump; a
regression that drops the `source_notice: Some(…)` on the ffmpeg entry
breaks step 6 and is an LGPL § 4 compliance failure; a regression that
swaps the modal `if root.open : Rectangle` branches for an
unconditionally-mounted card would surface as a clipped or visible
empty card with the modal "closed" — both branches must be guarded by
`root.open`.
