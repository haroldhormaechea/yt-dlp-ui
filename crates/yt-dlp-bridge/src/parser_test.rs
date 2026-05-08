//! Tests for [`crate::parser::parse_progress_line`].

use crate::download::DownloadEvent;
use crate::parser::{parse_destination_line, parse_progress_line};
use proptest::prelude::*;

#[test]
fn parses_well_formed_line() {
    let line = "yt-dlp-ui-progress 50 100 1024 30";
    let evt = parse_progress_line(line).expect("should parse");
    match evt {
        DownloadEvent::Progress {
            pct,
            speed_bps,
            eta_s,
            downloaded_bytes,
            total_bytes,
        } => {
            assert!((pct.unwrap() - 50.0).abs() < 0.01);
            assert_eq!(speed_bps, Some(1024));
            assert_eq!(eta_s, Some(30));
            assert_eq!(downloaded_bytes, Some(50));
            assert_eq!(total_bytes, Some(100));
        }
        other => panic!("expected Progress, got {other:?}"),
    }
}

#[test]
fn missing_prefix_returns_none() {
    assert!(parse_progress_line("[download] something else").is_none());
    assert!(parse_progress_line("").is_none());
    assert!(parse_progress_line("yt-dlp-ui-progres 1 2 3 4").is_none());
}

#[test]
fn na_in_each_field_collapses_to_none() {
    let evt = parse_progress_line("yt-dlp-ui-progress NA 100 1024 30").unwrap();
    if let DownloadEvent::Progress {
        pct,
        speed_bps,
        eta_s,
        downloaded_bytes,
        total_bytes,
    } = evt
    {
        assert_eq!(pct, None, "downloaded NA → no pct");
        assert_eq!(speed_bps, Some(1024));
        assert_eq!(eta_s, Some(30));
        assert_eq!(downloaded_bytes, None, "downloaded NA → None");
        assert_eq!(total_bytes, Some(100));
    } else {
        panic!("expected Progress");
    }

    let evt = parse_progress_line("yt-dlp-ui-progress 50 NA 1024 30").unwrap();
    if let DownloadEvent::Progress {
        pct,
        speed_bps,
        eta_s,
        downloaded_bytes,
        total_bytes,
    } = evt
    {
        assert_eq!(pct, None, "total NA → no pct");
        assert_eq!(speed_bps, Some(1024));
        assert_eq!(eta_s, Some(30));
        assert_eq!(downloaded_bytes, Some(50));
        assert_eq!(total_bytes, None, "total NA → None");
    } else {
        panic!("expected Progress");
    }

    let evt = parse_progress_line("yt-dlp-ui-progress 50 100 NA 30").unwrap();
    if let DownloadEvent::Progress {
        pct,
        speed_bps,
        eta_s,
        downloaded_bytes,
        total_bytes,
    } = evt
    {
        assert!(pct.is_some());
        assert_eq!(speed_bps, None);
        assert_eq!(eta_s, Some(30));
        assert_eq!(downloaded_bytes, Some(50));
        assert_eq!(total_bytes, Some(100));
    } else {
        panic!("expected Progress");
    }

    let evt = parse_progress_line("yt-dlp-ui-progress 50 100 1024 NA").unwrap();
    if let DownloadEvent::Progress {
        pct,
        speed_bps,
        eta_s,
        downloaded_bytes,
        total_bytes,
    } = evt
    {
        assert!(pct.is_some());
        assert_eq!(speed_bps, Some(1024));
        assert_eq!(eta_s, None);
        assert_eq!(downloaded_bytes, Some(50));
        assert_eq!(total_bytes, Some(100));
    } else {
        panic!("expected Progress");
    }
}

#[test]
fn all_na_yields_progress_with_none_fields() {
    let evt = parse_progress_line("yt-dlp-ui-progress NA NA NA NA").unwrap();
    if let DownloadEvent::Progress {
        pct,
        speed_bps,
        eta_s,
        downloaded_bytes,
        total_bytes,
    } = evt
    {
        assert_eq!(pct, None);
        assert_eq!(speed_bps, None);
        assert_eq!(eta_s, None);
        assert_eq!(downloaded_bytes, None);
        assert_eq!(total_bytes, None);
    } else {
        panic!("expected Progress");
    }
}

#[test]
fn partial_fields_returns_none() {
    assert!(parse_progress_line("yt-dlp-ui-progress 50 100 1024").is_none());
    assert!(parse_progress_line("yt-dlp-ui-progress 50 100").is_none());
    assert!(parse_progress_line("yt-dlp-ui-progress 50").is_none());
    assert!(parse_progress_line("yt-dlp-ui-progress").is_none());
}

#[test]
fn extra_fields_returns_none() {
    assert!(parse_progress_line("yt-dlp-ui-progress 50 100 1024 30 extra").is_none());
}

#[test]
fn non_numeric_field_returns_none() {
    assert!(parse_progress_line("yt-dlp-ui-progress fifty 100 1024 30").is_none());
    assert!(parse_progress_line("yt-dlp-ui-progress 50 -100 1024 30").is_none());
}

#[test]
fn percentage_clamped_to_100() {
    let evt = parse_progress_line("yt-dlp-ui-progress 200 100 1024 30").unwrap();
    if let DownloadEvent::Progress { pct, .. } = evt {
        assert_eq!(pct, Some(100.0), "downloaded > total → clamp to 100");
    } else {
        panic!("expected Progress");
    }
}

#[test]
fn percentage_zero_total_yields_no_pct() {
    let evt = parse_progress_line("yt-dlp-ui-progress 0 0 0 0").unwrap();
    if let DownloadEvent::Progress { pct, .. } = evt {
        assert_eq!(pct, None, "total 0 → no pct (avoid div-by-zero)");
    } else {
        panic!("expected Progress");
    }
}

#[test]
fn integer_overflow_defenses() {
    // u64::MAX should still parse (not panic).
    let line = format!(
        "yt-dlp-ui-progress {} {} {} {}",
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX
    );
    let evt = parse_progress_line(&line).expect("u64::MAX should parse");
    if let DownloadEvent::Progress { pct, .. } = evt {
        assert_eq!(pct, Some(100.0), "MAX/MAX clamps to 100 (the ratio is 1.0)");
    } else {
        panic!("expected Progress");
    }
}

#[test]
fn whitespace_handling_in_prefix() {
    // Single space after "progress" is required by the prefix definition;
    // multiple spaces between fields are tolerated by `split_whitespace`.
    let evt = parse_progress_line("yt-dlp-ui-progress 50  100   1024 30");
    assert!(evt.is_some());
}

// -- UC 02: parse_destination_line ---------------------------------------

#[test]
fn parse_destination_line_strips_prefix_for_ascii_path() {
    // Locks the "everything after `[download] Destination: `" semantics:
    // the bridge captures the partial-file path from this exact stdout
    // line so Remove can clean up the `.part` file later.
    let line = "[download] Destination: /tmp/Big Buck Bunny.mp4.part";
    assert_eq!(
        parse_destination_line(line),
        Some("/tmp/Big Buck Bunny.mp4.part")
    );
}

#[test]
fn parse_destination_line_handles_path_with_spaces() {
    let line = "[download] Destination: /home/user/My Videos/clip 1.mkv.part";
    assert_eq!(
        parse_destination_line(line),
        Some("/home/user/My Videos/clip 1.mkv.part")
    );
}

#[test]
fn parse_destination_line_handles_non_ascii_path() {
    // yt-dlp emits UTF-8 paths verbatim; the parser must not normalize.
    let line = "[download] Destination: /home/user/Música/canción ñ é.mp3.part";
    assert_eq!(
        parse_destination_line(line),
        Some("/home/user/Música/canción ñ é.mp3.part")
    );
}

#[test]
fn parse_destination_line_returns_none_for_unrelated_lines() {
    assert_eq!(parse_destination_line(""), None);
    assert_eq!(parse_destination_line("[download] something else"), None);
    assert_eq!(parse_destination_line("yt-dlp-ui-progress 1 2 3 4"), None);
    // A near-miss prefix (note the missing colon) must not match.
    assert_eq!(
        parse_destination_line("[download] Destination /tmp/x"),
        None
    );
    // Wrong leading whitespace — the prefix is anchored, not trimmed.
    assert_eq!(
        parse_destination_line(" [download] Destination: /tmp/x"),
        None
    );
}

proptest! {
    /// Fuzzing: any 4-tuple of u64s produces a valid Progress event with no panic.
    #[test]
    fn proptest_no_panic_on_arbitrary_u64_tuples(a: u64, b: u64, c: u64, d: u64) {
        let line = format!("yt-dlp-ui-progress {a} {b} {c} {d}");
        let evt = parse_progress_line(&line);
        prop_assert!(evt.is_some(), "well-formed numeric line must parse");
        if let Some(DownloadEvent::Progress { pct: Some(p), .. }) = evt {
            prop_assert!((0.0..=100.0).contains(&p), "pct must be clamped: got {p}");
        }
    }

    /// Fuzzing: arbitrary text without the prefix returns None.
    #[test]
    fn proptest_arbitrary_lines_dont_panic(s in ".{0,200}") {
        let _ = parse_progress_line(&s);
    }
}
