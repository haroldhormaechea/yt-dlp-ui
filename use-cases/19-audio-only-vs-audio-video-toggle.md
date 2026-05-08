# Use Case 19: Per-URL audio-only vs. audio+video toggle on AddBar

## Summary
Add a per-URL format toggle on the AddBar letting the user choose "Audio + video" (default) or "Audio only" before enqueueing. The chosen format is captured into `queue_items.format_pref` at enqueue and read by the yt-dlp argv builder at spawn time (existing pattern). Audio-only output is **m4a** (yt-dlp's natural extraction from YouTube DASH streams — no conversion needed, no GPL dependencies). The yt-dlp argv builder maps the format choice to `-f bestvideo*+bestaudio/best` (or current default) for audio+video and to `-f bestaudio[ext=m4a]/bestaudio` plus `--extract-audio --audio-format m4a` for audio-only. UC 17's bundled LGPL-only ffmpeg covers the muxing/extraction post-processing without needing libmp3lame. No global Settings entry is added in this UC; the toggle lives on the AddBar only. No DB schema change (`format_pref` column already exists). No new third-party dependencies.

## Acceptance Criteria
1. The AddBar exposes a per-URL toggle with two mutually-exclusive states: "Audio + video" (default for new entries) and "Audio only".
2. The toggle's choice is captured into `queue_items.format_pref` at the moment the URL is enqueued.
3. On spawn, the yt-dlp argv builder reads `format_pref` from the row and emits the correct `-f` flag (and `--extract-audio --audio-format m4a` when audio-only is active).
4. Audio + video mode produces a single muxed video file (mp4 or mkv per yt-dlp's default selector) with both tracks correctly synced (UC 17 ffmpeg merge step).
5. Audio-only mode produces a single `.m4a` file with audio only. AAC source streams are remuxed losslessly via `--extract-audio --audio-format m4a`. **Opus-source uploads (newer DASH tiers, YouTube Music) are transcoded to AAC by the bundled LGPL ffmpeg's native `aac` encoder** so the output extension stays `.m4a` across all sources. (Amended 2026-05-08 during analysis: original "no re-encoding" parenthetical was incompatible with the "single `.m4a` file" promise on opus-source content; Path (a) — always-`.m4a` with silent transcode — locked.)
6. The toggle's default position on AddBar resets to "Audio + video" each time the AddBar is cleared / a new URL is entered (no sticky last-choice memory in this UC; could be added as a follow-up).
7. Once enqueued, a row's format is locked — it does not change if the user later toggles the AddBar to a different state. Mid-queue / in-flight format changes are NOT supported in this UC.
8. No regression to UC 09 (Settings panel), UC 16 (destination resolution), UC 14 (Start all broaden), UC 17 (ffmpeg bundling), UC 18 (About dialog).
9. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.
10. No new third-party Rust crates.

## Potential Pitfalls & Open Questions
- **Reconciliation note** — The clarification round answered "Per-URL toggle on AddBar only" + "Read at spawn time", which is internally inconsistent (no global setting exists to be re-read mid-queue). The interpretation locked here: per-URL toggle captures format at enqueue, the `format_pref` column is the source of truth at spawn, mid-queue editing is NOT supported. If a future iteration adds per-row format editing on a queued row (e.g., right-click → change format), spawn-time-read becomes meaningful — that is explicitly a follow-up, not part of this UC.
- **Edge case** — AddBar UI placement: where exactly does the toggle live? Inline next to the URL field, in a dropdown on the Add button, in a popover triggered by an icon? UI design pending — the dev team picks during analysis based on UC 07 design system + UC 13 icon fidelity.
- **Edge case** — Sticky last-choice memory: should the AddBar remember the last-used choice and pre-select it on next entry? Out of scope per AC #6; could be a follow-up UC if useful.
- **Edge case** — yt-dlp's actual format selector for "audio + video" today: the analyst must read the existing argv builder before changing it. The change must add the audio-only branch without regressing the existing audio+video selector.
- **Risk** — Existing rows on upgrade: rows enqueued before this UC ships have `format_pref` set to whatever the old default was. Their behavior is unchanged (existing column value drives spawn). New rows post-UC carry the new toggle's value.
- **Risk** — m4a extraction depends on ffmpeg's built-in AAC demuxer (which is enabled by default in UC 17's LGPL-only build). If YouTube's audio for a given video is opus rather than AAC, yt-dlp's `--extract-audio --audio-format m4a` would need to either re-encode (requires `--enable-libfdk-aac` which is GPL — disabled in UC 17) or remux from opus to m4a (may not be possible without libopus, which UC 17 dropped due to a `pkg-config` issue). The analyst should verify what happens for opus-source YouTube videos and either fall back to opus output or surface an error. Concrete fallback options: (a) when source is opus, output `.opus` instead of `.m4a` and rename AC #5; (b) re-enable `--enable-libopus` in UC 17's macOS build (requires `brew install opus libvorbis` in CI/dev setup) so opus → m4a remuxing works; (c) declare audio-only mode "extracts whatever native audio yt-dlp finds" and accept either `.m4a` or `.opus` output.

## Original Description
We must be able to allow the user to download only audio or both audio+video in the settings.

## Clarifications
- Q: Audio-only output format — mp3 requires libmp3lame which is GPL (UC 17 explicitly disabled it for PolyForm compatibility):
  A: m4a (AAC) — native YouTube audio, no conversion. Compatible with current LGPL-only ffmpeg.
- Q: UI scope — where does the choice live?
  A: Per-URL toggle on AddBar only. Each enqueued URL carries its own format choice; no global Settings entry in this UC.
- Q: Default value for new installs?
  A: Audio + video (current behavior). Aligns with the primary "media saver" user type.
- Q: Mid-queue change semantic — read at spawn or snapshot at enqueue?
  A: Read at spawn time (answered with the assumption of a global setting). Reconciled in pitfalls: with per-URL only, spawn reads from the row's `format_pref` column (already current behavior); mid-queue editing is explicitly out of scope.
