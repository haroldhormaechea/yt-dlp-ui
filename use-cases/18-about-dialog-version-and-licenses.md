# Use Case 18: About dialog — version + bundled-software licenses

## Summary
Add an About dialog that displays the app version (target 0.5.0) and the licenses of the project plus every bundled third-party component. Satisfies LGPL § 4's "notify users that the program uses LGPL'd software" requirement triggered by UC 17's bundled ffmpeg, and consolidates the project's `LICENSE.UI.md` (PolyForm Noncommercial 1.0.0), upstream yt-dlp's Unlicense, deno's license (UC 05), ffmpeg's LGPL (UC 17), and the embedded fonts' OFL (Inter + JetBrains Mono per UC 07) into a single user-discoverable surface. Rendered as a centered modal reusing the UC 10 bot-check-modal infrastructure, launched from a new "About yt-dlp-ui" entry at the bottom of the UC 09 Settings slide-in panel. Version comes from `env!("CARGO_PKG_VERSION")` after the workspace `Cargo.toml` is bumped to 0.5.0; bundled-binary versions are pinned at build time from `scripts/runtime-deps-pins.env` (no runtime spawn). License texts ship under `crates/app/assets/licenses/` and are `include_str!`-ed into the binary so the dialog works fully offline. This UC depends on UC 17 having merged (FFMPEG_VERSION pin must exist in `runtime-deps-pins.env`).

## Acceptance Criteria
1. Workspace `Cargo.toml` version bumped to `0.5.0` (workspace `[workspace.package]` + each member crate via `version.workspace = true` — developer picks the exact split based on the existing layout); `cargo build` succeeds.
2. The About dialog displays "yt-dlp-ui" and the version string `0.5.0` read from `env!("CARGO_PKG_VERSION")` — no hardcoded duplicate.
3. Dialog displays project license: PolyForm Noncommercial 1.0.0 (label + access to full text via a "View full license" button).
4. Dialog displays an entry for upstream yt-dlp: name, pinned version (from `runtime-deps-pins.env`), license name (Unlicense), full text accessible.
5. Dialog displays an entry for deno: name, pinned version (from `runtime-deps-pins.env`), license name, full text accessible.
6. Dialog displays an entry for ffmpeg: name, pinned version (from `FFMPEG_VERSION` in `runtime-deps-pins.env` per UC 17), license name (LGPL-2.1-or-later), full text accessible, plus a one-line "Source available at: https://ffmpeg.org/ — see scripts/build-ffmpeg-macos.sh for the rebuild recipe" notice for LGPL § 4 compliance.
7. Dialog displays entries for the embedded Inter and JetBrains Mono fonts (license: SIL OFL 1.1, full text accessible).
8. License full-text views are scrollable (reuse the UC 15 `ListView` / `ScrollView` patterns as needed).
9. The dialog is launched from a new "About yt-dlp-ui" entry at the bottom of the UC 09 Settings slide-in panel; no new top-level shell chrome.
10. The dialog is a centered modal reusing the UC 10 bot-check-modal infrastructure (backdrop, centered card, design-system tokens from UC 07).
11. Dismiss via Esc key, Close button, or backdrop click — same pattern as UC 10.
12. External links (the ffmpeg source URL) open in the system default browser via the same handler used for UC 11 ad-banner clicks; no in-app browser.
13. License texts ship under `crates/app/assets/licenses/<name>.txt` (one file per license) and are bundled at compile time via `include_str!`.
14. No regression to UC 09 / UC 10 / UC 11 / UC 13 / UC 15.
15. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.
16. No new third-party Rust crates.

## Potential Pitfalls & Open Questions
- **Dependency** — This UC depends on UC 17 being merged first (FFMPEG_VERSION must be pinned in `runtime-deps-pins.env`). If UC 17 is still in flight when this UC is picked up, the dev team blocks until UC 17 lands.
- **Implementation choice** — Build-time pin reading: `runtime-deps-pins.env` is shell `.env` format. The crate needs a small build-script helper (or inline in `build.rs`) to parse the pins and emit them as compile-time constants the dialog reads (e.g. via `cargo:rustc-env=YT_DLP_VERSION=…` and `env!` at usage). The analyst picks the parser approach.
- **Edge case** — Workspace `Cargo.toml` structure: bumping version may need to happen at the root `[workspace.package]` section with each member crate inheriting via `version.workspace = true`, OR per-crate. The developer verifies and applies whichever the existing layout uses.
- **Edge case** — `include_str!` bundles license text into the binary, increasing binary size by ~10–30 KB. Acceptable per UC 17's revised 100 MB bundle-size ceiling.
- **Assumption** — "Bundled software" scope: yt-dlp, deno, ffmpeg, Inter, JetBrains Mono. NOT in scope: transitive Rust crates (covered by `cargo-deny` license allow-list, not user-facing).

## Original Description
About dialog. Should display the app version (current target version is 0.5) and the licenses of the project itself plus all bundled third-party software (yt-dlp Unlicense, ffmpeg LGPL, deno license, plus any other bundled binaries / fonts). Satisfies the LGPL § 4 in-app notice requirement that came up during the UC 17 ffmpeg discussion.

## Clarifications
- Q: Where should the About dialog be triggered from?
  A: Inside the UC 09 Settings panel — new "About yt-dlp-ui" row at the bottom of the existing slide-in. Lowest friction; no new shell chrome.
- Q: Display surface for the dialog itself?
  A: Centered modal (UC 10 pattern). Reuse the bot-check-modal infrastructure: backdrop + centered card.
- Q: License-text source-of-truth — where do the license texts live?
  A: Embedded at build time via `include_str!`. Copies under `crates/app/assets/licenses/`. Works offline, no I/O, no per-OS path logic.
- Q: Bundled-binary version display — pinned (from runtime-deps-pins.env) or live (queried at runtime)?
  A: Pinned at build time. Display the version recorded in `runtime-deps-pins.env`, baked into the binary at compile time. Fast, offline, no spawn overhead.
