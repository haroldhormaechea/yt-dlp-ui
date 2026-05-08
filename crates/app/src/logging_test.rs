//! Tests for [`crate::logging`] retention logic.
//!
//! Note on file mtime: `prune_old_logs` actually parses the date suffix in the
//! filename rather than reading the OS mtime, which is the right choice for
//! `tracing-appender`'s daily-rolled files. So these tests construct files with
//! crafted suffixes; they do not need to manipulate filesystem timestamps.

use std::fs;

use super::{parse_yyyy_mm_dd_to_epoch, prune_old_logs};

#[test]
fn parse_yyyy_mm_dd_accepts_valid_dates() {
    assert!(parse_yyyy_mm_dd_to_epoch("2025-01-01").is_some());
    assert!(parse_yyyy_mm_dd_to_epoch("2024-12-31").is_some());
    assert!(parse_yyyy_mm_dd_to_epoch("1970-01-01").is_some());
}

#[test]
fn parse_yyyy_mm_dd_rejects_garbage() {
    assert!(parse_yyyy_mm_dd_to_epoch("").is_none());
    assert!(parse_yyyy_mm_dd_to_epoch("not-a-date").is_none());
    assert!(
        parse_yyyy_mm_dd_to_epoch("2025-13-01").is_none(),
        "month 13"
    );
    assert!(parse_yyyy_mm_dd_to_epoch("2025-01-32").is_none(), "day 32");
    assert!(parse_yyyy_mm_dd_to_epoch("2025-00-15").is_none(), "month 0");
    assert!(
        parse_yyyy_mm_dd_to_epoch("2025-1-1").is_some(),
        "loose digits OK"
    );
    assert!(
        parse_yyyy_mm_dd_to_epoch("1969-12-31").is_none(),
        "pre-epoch"
    );
    assert!(
        parse_yyyy_mm_dd_to_epoch("2025-01-01-extra").is_none(),
        "trailing parts rejected"
    );
}

#[test]
fn prune_skips_non_log_filenames() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path_unrelated = tmp.path().join("unrelated.txt");
    fs::write(&path_unrelated, b"hi").unwrap();

    prune_old_logs(tmp.path());

    assert!(
        path_unrelated.exists(),
        "unrelated files must not be touched"
    );
}

#[test]
fn prune_keeps_recent_log_rolls_and_deletes_ancient_ones() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Today-ish (well within retention window): 2099-12-31 — guaranteed future.
    // Ancient (~30 years before epoch crossing 7-day cutoff): 1971-01-01.
    let recent = tmp.path().join("yt-dlp-ui.log.2099-12-31");
    let ancient = tmp.path().join("yt-dlp-ui.log.1971-01-01");
    let malformed = tmp.path().join("yt-dlp-ui.log.malformed-name");
    let unrelated = tmp.path().join("not-a-log.2020-01-01");
    fs::write(&recent, b"r").unwrap();
    fs::write(&ancient, b"a").unwrap();
    fs::write(&malformed, b"m").unwrap();
    fs::write(&unrelated, b"u").unwrap();

    prune_old_logs(tmp.path());

    assert!(recent.exists(), "recent roll must survive");
    assert!(!ancient.exists(), "ancient roll must be deleted");
    assert!(
        malformed.exists(),
        "malformed-suffix rolls are skipped (not deleted)"
    );
    assert!(unrelated.exists(), "non-log files are not touched");
}

#[test]
fn prune_handles_empty_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Should not panic on an empty directory.
    prune_old_logs(tmp.path());
}
