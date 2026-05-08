//! Tests for [`crate::model`] helpers.

use crate::model::split_pasted_urls;

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
