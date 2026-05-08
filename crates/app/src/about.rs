//! About-dialog content surface. No Slint or I/O dependencies.

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const YT_DLP_VERSION: &str = env!("YT_DLP_UI_PINNED_YT_DLP_VERSION");
pub const DENO_VERSION: &str = env!("YT_DLP_UI_PINNED_DENO_VERSION");
pub const FFMPEG_RELEASE_TAG: &str = env!("YT_DLP_UI_PINNED_FFMPEG_RELEASE_TAG");
pub const FFMPEG_VERSION_SOURCE: &str = env!("YT_DLP_UI_PINNED_FFMPEG_VERSION_SOURCE");

pub struct AboutEntry {
    pub name: &'static str,
    pub version: &'static str,
    pub license_name: &'static str,
    pub license_text: &'static str,
    pub source_notice: Option<&'static str>,
}

// SOURCE: lines 13..end of repo-root LICENSE (PolyForm 1.0.0 body, preamble stripped)
const POLYFORM_TEXT: &str = include_str!("../assets/licenses/polyform-noncommercial-1.0.0.txt");
// SOURCE: byte-for-byte copy of installer/yt-dlp-LICENSE.txt (yt-dlp Unlicense)
const UNLICENSE_TEXT: &str = include_str!("../assets/licenses/unlicense.txt");
// SOURCE: existing crates/app/assets/fonts/LICENSE.OFL.txt — reused via relative path
const OFL_TEXT: &str = include_str!("../assets/fonts/LICENSE.OFL.txt");
// SOURCE: canonical Deno MIT (Copyright (c) 2018-Present the Deno authors)
const DENO_MIT_TEXT: &str = include_str!("../assets/licenses/deno.txt");
// SOURCE: canonical GNU LGPL-2.1; ships as a TODO placeholder until the user pastes canonical bytes
const LGPL_TEXT: &str = include_str!("../assets/licenses/lgpl-2.1.txt");

const FFMPEG_SOURCE_NOTICE: &str = "Source available at: https://ffmpeg.org/ — see scripts/build-ffmpeg-macos.sh for the rebuild recipe";

const ENTRIES: &[AboutEntry] = &[
    AboutEntry {
        name: "yt-dlp-ui",
        version: APP_VERSION,
        license_name: "PolyForm Noncommercial 1.0.0",
        license_text: POLYFORM_TEXT,
        source_notice: None,
    },
    AboutEntry {
        name: "yt-dlp",
        version: YT_DLP_VERSION,
        license_name: "Unlicense",
        license_text: UNLICENSE_TEXT,
        source_notice: None,
    },
    AboutEntry {
        name: "deno",
        version: DENO_VERSION,
        license_name: "MIT",
        license_text: DENO_MIT_TEXT,
        source_notice: None,
    },
    AboutEntry {
        name: "ffmpeg",
        version: ffmpeg_display_version(),
        license_name: "LGPL-2.1-or-later",
        license_text: LGPL_TEXT,
        source_notice: Some(FFMPEG_SOURCE_NOTICE),
    },
    AboutEntry {
        name: "Inter",
        version: "Variable",
        license_name: "SIL OFL 1.1",
        license_text: OFL_TEXT,
        source_notice: None,
    },
    AboutEntry {
        name: "JetBrains Mono",
        version: "Variable",
        license_name: "SIL OFL 1.1",
        license_text: OFL_TEXT,
        source_notice: None,
    },
];

#[must_use]
pub fn entries() -> &'static [AboutEntry] {
    ENTRIES
}

/// On macOS we ship a from-source `FFmpeg` build (no LGPL-only mainstream
/// macOS prebuilt exists), so the meaningful version is the tarball
/// version (`FFMPEG_VERSION_SOURCE`, e.g. `7.1`). On Linux/Windows we
/// fetch a `BtbN/FFmpeg-Builds` artifact whose stable-release filename
/// embeds an `n`-prefixed release tag (e.g. `n7.1.4`); strip the `n` for
/// display.
#[cfg(target_os = "macos")]
#[must_use]
pub const fn ffmpeg_display_version() -> &'static str {
    FFMPEG_VERSION_SOURCE
}

#[cfg(not(target_os = "macos"))]
#[must_use]
pub const fn ffmpeg_display_version() -> &'static str {
    trim_leading_n(FFMPEG_RELEASE_TAG)
}

/// `const`-friendly leading-`n` strip: if the first byte is `b'n'`, return
/// the slice starting at index 1; otherwise return the original. Done via
/// raw byte/pointer arithmetic because `str::split_at` is not `const fn`
/// on the pinned Rust 1.95.0 toolchain.
#[cfg(not(target_os = "macos"))]
const fn trim_leading_n(s: &'static str) -> &'static str {
    let bytes = s.as_bytes();
    if !bytes.is_empty() && bytes[0] == b'n' {
        // SAFETY-equivalent in const: slicing a UTF-8 string at a known
        // ASCII boundary (byte 1 after a leading ASCII 'n') is valid.
        // We use `from_utf8` on the tail bytes to stay safe-Rust.
        let tail = bytes.split_at(1).1;
        match std::str::from_utf8(tail) {
            Ok(t) => t,
            Err(_) => s,
        }
    } else {
        s
    }
}

#[cfg(test)]
#[path = "about_test.rs"]
mod about_tests;
