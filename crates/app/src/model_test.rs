//! Tests for [`crate::model`] helpers.

use crate::model::{PlaceholderKind, split_pasted_urls};

#[test]
fn single_url_returns_one_entry() {
    let urls = split_pasted_urls("https://example.com/a");
    assert_eq!(urls, vec!["https://example.com/a".to_string()]);
}

#[test]
fn multi_line_paste_splits_on_newlines() {
    let raw = "https://example.com/a\nhttps://example.com/b\nhttps://example.com/c";
    let urls = split_pasted_urls(raw);
    assert_eq!(
        urls,
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
            "https://example.com/c".to_string(),
        ]
    );
}

#[test]
fn drops_empty_lines() {
    let raw = "https://example.com/a\n\nhttps://example.com/b\n\n\n";
    let urls = split_pasted_urls(raw);
    assert_eq!(
        urls,
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ]
    );
}

#[test]
fn trims_surrounding_whitespace() {
    let raw = "   https://example.com/a   \n\thttps://example.com/b\t";
    let urls = split_pasted_urls(raw);
    assert_eq!(
        urls,
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ]
    );
}

#[test]
fn whitespace_only_lines_are_dropped() {
    let raw = "https://example.com/a\n   \n\t\nhttps://example.com/b";
    let urls = split_pasted_urls(raw);
    assert_eq!(urls.len(), 2);
}

#[test]
fn empty_input_yields_empty_vec() {
    assert!(split_pasted_urls("").is_empty());
    assert!(split_pasted_urls("   \n\n   \n").is_empty());
}

#[test]
fn preserves_inner_characters() {
    let raw = "https://example.com/a?b=c&d=e";
    let urls = split_pasted_urls(raw);
    assert_eq!(urls, vec!["https://example.com/a?b=c&d=e".to_string()]);
}

#[test]
fn carriage_return_lf_handled_via_trim() {
    // Windows-style line endings: \r\n. The split is on \n only, so trailing \r
    // must be trimmed by the per-line .trim() pass.
    let raw = "https://example.com/a\r\nhttps://example.com/b\r\n";
    let urls = split_pasted_urls(raw);
    assert_eq!(
        urls,
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ]
    );
}

// -- UC 27: PlaceholderKind --------------------------------------------

#[test]
fn placeholder_kind_round_trips_via_as_str_and_parse() {
    for kind in [PlaceholderKind::Video, PlaceholderKind::Pending] {
        let s = kind.as_str();
        let parsed = PlaceholderKind::parse(s).expect("known variant parses cleanly");
        assert_eq!(parsed, kind, "round-trip via as_str() / parse() must match");
    }
}

#[test]
fn placeholder_kind_as_str_matches_sql_column_strings() {
    // SQL CHECK constraint mirrors these strings exactly. Keeping the
    // assertion explicit pins the contract against accidental rename.
    assert_eq!(PlaceholderKind::Video.as_str(), "video");
    assert_eq!(PlaceholderKind::Pending.as_str(), "pending");
}

#[test]
fn placeholder_kind_parse_rejects_unknown_variants() {
    let err = PlaceholderKind::parse("bogus").expect_err("unknown string must fail");
    assert_eq!(err, "bogus", "error surfaces the offending input");
}
