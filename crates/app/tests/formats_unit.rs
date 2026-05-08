//! Unit-level tests for [`app::formats`] — `format_size` and
//! `format_dest_dir`. Lives in `tests/` (not `src/formats_test.rs`) because
//! the production-code `#[cfg(test)] mod` include needed to activate a side
//! file is owned by the developer and is not in QA's write scope.

use std::path::PathBuf;

use app::formats::{format_dest_dir, format_size};

// -- format_size -----------------------------------------------------------

#[test]
fn format_size_none_returns_empty() {
    assert_eq!(format_size(None), "");
}

#[test]
fn format_size_below_1kb_renders_as_bytes() {
    assert_eq!(format_size(Some(0)), "0 B");
    assert_eq!(format_size(Some(1)), "1 B");
    assert_eq!(format_size(Some(512)), "512 B");
    assert_eq!(format_size(Some(1023)), "1023 B");
}

#[test]
fn format_size_kb_range_renders_one_decimal() {
    assert_eq!(format_size(Some(1024)), "1.0 KB");
    assert_eq!(format_size(Some(1536)), "1.5 KB");
    assert_eq!(format_size(Some(1024 * 1024 - 1)), "1024.0 KB");
}

#[test]
fn format_size_mb_range_renders_one_decimal() {
    assert_eq!(format_size(Some(1024 * 1024)), "1.0 MB");
    assert_eq!(format_size(Some(5 * 1024 * 1024 + 512 * 1024)), "5.5 MB");
}

#[test]
fn format_size_gb_range_renders_two_decimals() {
    let one_gb: u64 = 1024 * 1024 * 1024;
    assert_eq!(format_size(Some(one_gb)), "1.00 GB");
    let one_point_five_gb = one_gb + 512 * 1024 * 1024;
    assert_eq!(format_size(Some(one_point_five_gb)), "1.50 GB");
}

#[test]
fn format_size_handles_extreme_values_without_panic() {
    // u64::MAX → display only; no panic, no overflow.
    let _ = format_size(Some(u64::MAX));
}

// -- format_dest_dir -------------------------------------------------------

#[test]
fn format_dest_dir_substitutes_home_with_tilde() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = PathBuf::from(format!("{home}/Downloads/yt-dlp-ui"));
    let s = format_dest_dir(&dir);
    assert!(
        s.starts_with("~/"),
        "$HOME prefix must be replaced with ~/ : {s}"
    );
    assert!(
        s.ends_with("Downloads/yt-dlp-ui") || s.contains('…'),
        "tail of dest dir must survive the substitution (or be middle-elided): {s}"
    );
}

#[test]
fn format_dest_dir_short_path_passes_through_unchanged() {
    let dir = PathBuf::from("/tmp/short");
    let s = format_dest_dir(&dir);
    // No HOME substitution applies; no ellipsis on a short path.
    assert_eq!(s, "/tmp/short");
}

#[test]
fn format_dest_dir_long_path_middle_ellipsizes_at_40_chars() {
    // A 60-char path (well above the 40-char threshold) must contain the
    // ellipsis character and the result length must be ≤ 40 chars.
    let long = "/var/some/deep/nested/path/that/keeps/going/for/a/while/file";
    let dir = PathBuf::from(long);
    let s = format_dest_dir(&dir);
    assert!(
        s.contains('…'),
        "long path must include the middle-ellipsis character: {s}"
    );
    let count = s.chars().count();
    assert!(
        count <= 40,
        "ellipsized result must fit in 40 chars, got {count}: {s}"
    );
}

#[test]
fn format_dest_dir_handles_multibyte_characters_without_panic() {
    // Unicode-rich path > 40 chars must middle-ellipsize on char boundaries
    // (not byte boundaries) — no panic.
    let dir = PathBuf::from("/Users/hhormaechea/Downloads/日本語/yt-dlp-ui/very-long-folder-name");
    let _ = format_dest_dir(&dir);
}

#[test]
fn format_dest_dir_empty_path_returns_empty_string() {
    let s = format_dest_dir(&PathBuf::from(""));
    assert_eq!(s, "");
}
