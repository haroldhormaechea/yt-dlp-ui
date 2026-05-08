//! Display-format helpers for the queue row mono lines.
//!
//! These take raw model values (bytes, paths) and produce the strings the
//! Slint row consumes verbatim. Pure (no I/O), trivially testable; QA owns
//! `formats_test.rs` alongside this file.

use std::path::Path;

// QA owns `formats_test.rs` next to this file. The include line is added
// here once that file lands; until then, leaving it out keeps `cargo fmt`
// from tripping on a missing module path.

/// Maximum length of the destination-dir display before middle-ellipsis
/// kicks in. Picked to match the design's "saved to <path>" layout
/// (`design/project/queue-row.jsx` ~line 218 uses a fixed-width slot).
const DEST_DIR_MAX_LEN: usize = 40;

/// Formats an optional byte count into a human-readable string. Returns the
/// empty string on `None` so the Slint markup can omit the field via its
/// `if data.size != "" :` guards.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else {
        return String::new();
    };
    if b < 1024 {
        return format!("{b} B");
    }
    // u64 → f64 cast: the precision loss only kicks in above ~9 PiB, well
    // beyond any realistic media size. Acceptable for a display string.
    let kb = b as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1024.0;
    format!("{gb:.2} GB")
}

/// Substitutes `$HOME` with `~` and applies a middle-ellipsis when the
/// resulting string is longer than [`DEST_DIR_MAX_LEN`] characters. Empty
/// path → empty string (the row hides the "saved to" line).
#[must_use]
pub fn format_dest_dir(dir: &Path) -> String {
    let raw = dir.to_string_lossy().to_string();
    let with_home = match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && raw.starts_with(&home) => {
            let rest = &raw[home.len()..];
            format!("~{rest}")
        }
        _ => raw,
    };
    middle_ellipsis(&with_home, DEST_DIR_MAX_LEN)
}

/// Truncates `s` with a middle-ellipsis if its character count exceeds
/// `max_len`. Operates on chars (not bytes) so multi-byte filenames don't
/// produce broken UTF-8.
fn middle_ellipsis(s: &str, max_len: usize) -> String {
    let count = s.chars().count();
    if count <= max_len {
        return s.to_string();
    }
    let keep = max_len.saturating_sub(1); // 1 for the `…`
    let head_len = keep / 2;
    let tail_len = keep - head_len;
    let head: String = s.chars().take(head_len).collect();
    let tail: String = s.chars().skip(count - tail_len).collect();
    format!("{head}…{tail}")
}
