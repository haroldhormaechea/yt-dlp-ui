//! Unit tests for [`super`] (the `about` module).
//!
//! Pins the static contract of `entries()` — names, version surfacing,
//! license-text non-emptiness, and the ffmpeg source-notice — plus the
//! per-OS branching of `ffmpeg_display_version()`.
//!
//! Wire-up note: this file requires
//! `#[cfg(test)] #[path = "about_test.rs"] mod about_tests;` at the bottom
//! of `about.rs` (mirroring `bot_check.rs:26-28` and `paths.rs:13-15`).
//! Without that line, Cargo will not pick this file up.

#[cfg(not(target_os = "macos"))]
use super::FFMPEG_RELEASE_TAG;
#[cfg(target_os = "macos")]
use super::FFMPEG_VERSION_SOURCE;
use super::{APP_VERSION, entries, ffmpeg_display_version};
use std::collections::HashSet;

/// AC#1 / AC#2 — version surfacing. The workspace Cargo.toml is pinned to
/// `0.5.1-rc.6`; `APP_VERSION` mirrors `env!("CARGO_PKG_VERSION")` and must
/// reflect that bump. A future bump updates this constant in lock-step.
#[test]
fn app_version_matches_cargo_pin() {
    assert_eq!(APP_VERSION, "0.5.1-rc.6");
}

/// AC#3-7, AC#13 — entry name set. The bundled-software scope per UC 18 is
/// `yt-dlp-ui`, `yt-dlp`, `deno`, `Inter`, `JetBrains Mono`, plus the
/// combined `ffmpeg + ffprobe` entry (UC 28 — single About row covering both
/// binaries since they ship under the same FFmpeg distribution / LGPL-2.1+
/// license / source notice).
/// Any drift here (added entry, removed entry, renamed entry) is intentional
/// and should land alongside a test update — the assertion is on the SET
/// (not the count) so accidental duplicates also trip the check.
#[test]
fn entries_cover_exactly_the_uc18_bundled_scope() {
    let names: HashSet<&str> = entries().iter().map(|e| e.name).collect();
    let expected: HashSet<&str> = [
        "yt-dlp-ui",
        "yt-dlp",
        "deno",
        "ffmpeg + ffprobe",
        "Inter",
        "JetBrains Mono",
    ]
    .into_iter()
    .collect();

    assert_eq!(
        names, expected,
        "about::entries() name set drifted from UC 18 scope"
    );

    // Defensive — set vs. slice length check catches duplicate names that
    // would silently survive the set comparison.
    assert_eq!(
        entries().len(),
        expected.len(),
        "about::entries() contains duplicate names"
    );
}

/// AC#13 — `include_str!` plumbing. Each entry's `license_text` is bundled
/// at compile time from `crates/app/assets/licenses/<name>.txt` (or the
/// reused fonts/LICENSE.OFL.txt). A typo in a path or an empty file would
/// surface as an empty / near-empty bundled string. Threshold is 50 bytes
/// rather than the original 100-byte spec because the LGPL ships as a
/// deliberate placeholder header pending a pasted canonical text (98 bytes
/// today; AC#13 + the use-case "Pitfalls" treat the placeholder as
/// acceptable). 50 bytes still catches an empty include or a single-line
/// truncation — every real license body is multi-paragraph and hundreds of
/// bytes long, so any drift below that threshold is a regression.
#[test]
fn every_entry_carries_non_trivial_license_text() {
    for entry in entries() {
        assert!(
            entry.license_text.len() > 50,
            "entry {:?} license_text is suspiciously short ({} bytes) — likely an include_str! typo or empty file",
            entry.name,
            entry.license_text.len(),
        );
    }
}

/// AC#6 — ffmpeg's LGPL § 4 source-notice. The combined `ffmpeg + ffprobe`
/// entry must carry a non-empty `source_notice` whose text references
/// `ffmpeg.org` so users can locate the upstream source. Every other entry
/// carries `source_notice: None` (no LGPL obligation). UC 28 folds ffprobe
/// into the same About row as ffmpeg — they ship from the same FFmpeg
/// distribution, so a single LGPL notice satisfies both.
#[test]
fn ffmpeg_entry_has_source_notice_pointing_at_ffmpeg_org() {
    let ffmpeg = entries()
        .iter()
        .find(|e| e.name == "ffmpeg + ffprobe")
        .expect("ffmpeg + ffprobe entry present");

    let notice = ffmpeg
        .source_notice
        .expect("ffmpeg + ffprobe entry must carry an LGPL § 4 source notice");

    assert!(
        notice.contains("ffmpeg.org"),
        "ffmpeg source notice must reference ffmpeg.org for LGPL § 4 compliance; got: {notice:?}"
    );

    // Counter-side — every non-ffmpeg entry has no notice. A regression
    // that broadened the notice to e.g. yt-dlp would surface here.
    for entry in entries().iter().filter(|e| e.name != "ffmpeg + ffprobe") {
        assert!(
            entry.source_notice.is_none(),
            "entry {:?} unexpectedly carries a source notice",
            entry.name,
        );
    }
}

/// AC#6 — `ffmpeg_display_version()` returns a non-empty, user-facing
/// string. On non-macOS targets, the BtbN/FFmpeg-Builds release tag is
/// stripped of its leading `n` for display (e.g. `n7.1.4` → `7.1.4`); on
/// macOS, the from-source tarball version (`FFMPEG_VERSION_SOURCE`) is
/// returned verbatim. Both polarities are covered here — the active branch
/// runs on the host's OS and the inactive constant is asserted via its
/// `env!`-bound declaration at the module level.
#[test]
fn ffmpeg_display_version_is_non_empty_and_strips_leading_n_on_non_macos() {
    let displayed = ffmpeg_display_version();
    assert!(
        !displayed.is_empty(),
        "ffmpeg_display_version must surface a non-empty string"
    );

    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            !displayed.starts_with('n'),
            "ffmpeg_display_version on non-macOS must strip the BtbN release-tag leading 'n'; got {displayed:?}"
        );
        // The strip is `n` → tail; the displayed version must therefore be
        // a tail of the raw tag (or equal to it if the tag had no leading n).
        assert!(
            FFMPEG_RELEASE_TAG == displayed
                || FFMPEG_RELEASE_TAG.strip_prefix('n') == Some(displayed),
            "displayed version must be the raw tag or its tail after stripping a leading 'n'; \
             tag={FFMPEG_RELEASE_TAG:?}, displayed={displayed:?}"
        );
    }

    #[cfg(target_os = "macos")]
    {
        assert_eq!(
            displayed, FFMPEG_VERSION_SOURCE,
            "on macOS ffmpeg_display_version must surface the from-source tarball version verbatim"
        );
    }
}

/// AC#2 — entry version surfacing. The yt-dlp-ui entry's `version` field
/// must mirror `APP_VERSION` (no hardcoded duplicate) so a workspace
/// version bump propagates to the dialog without a second touch point.
#[test]
fn project_entry_version_mirrors_app_version() {
    let project = entries()
        .iter()
        .find(|e| e.name == "yt-dlp-ui")
        .expect("yt-dlp-ui entry present");

    assert_eq!(
        project.version, APP_VERSION,
        "yt-dlp-ui entry version must mirror APP_VERSION (env!(CARGO_PKG_VERSION))"
    );
}
