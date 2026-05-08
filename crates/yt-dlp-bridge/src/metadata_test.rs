//! Unit tests for the manual `Deserialize` impl on [`PlaylistEntry`] in
//! `metadata.rs`. Covers the canonical-URL resolution rules:
//! `webpage_url` wins over `url`; either alone is sufficient; both absent
//! is a structured error; titles are passed through (including `null`).

use super::PlaylistEntry;

#[test]
fn deserializes_with_both_url_and_webpage_url() {
    let json = r#"{
        "_type": "url",
        "ie_key": "Youtube",
        "id": "fryat2XxbWc",
        "url": "https://www.youtube.com/watch?v=fryat2XxbWc",
        "webpage_url": "https://www.youtube.com/watch?v=fryat2XxbWc",
        "title": "Sample title",
        "duration": 180,
        "view_count": 12345
    }"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(
        entry.url, "https://www.youtube.com/watch?v=fryat2XxbWc",
        "webpage_url is the canonical URL when both are present"
    );
    assert_eq!(entry.title.as_deref(), Some("Sample title"));
}

#[test]
fn deserializes_with_only_url() {
    let json = r#"{"url":"https://example.com/only-url","title":"T"}"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(entry.url, "https://example.com/only-url");
    assert_eq!(entry.title.as_deref(), Some("T"));
}

#[test]
fn deserializes_with_only_webpage_url() {
    let json = r#"{"webpage_url":"https://example.com/only-webpage","title":"T"}"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(entry.url, "https://example.com/only-webpage");
    assert_eq!(entry.title.as_deref(), Some("T"));
}

#[test]
fn deserialize_fails_when_both_url_fields_absent() {
    let json = r#"{"title":"orphan"}"#;
    let err = serde_json::from_str::<PlaylistEntry>(json)
        .expect_err("must fail when neither url nor webpage_url is present");
    let msg = err.to_string();
    assert!(
        msg.contains("missing both `webpage_url` and `url`"),
        "error must surface the structured custom message (got: {msg})"
    );
}

#[test]
fn prefers_webpage_url_over_url_when_both_differ() {
    let raw_url = "https://example.com/raw-url";
    let canonical = "https://example.com/canonical-webpage-url";
    let json = format!(r#"{{"url":"{raw_url}","webpage_url":"{canonical}","title":"X"}}"#);
    let entry: PlaylistEntry = serde_json::from_str(&json).expect("deserialize succeeds");
    assert_eq!(
        entry.url, canonical,
        "webpage_url wins by data, not just by name"
    );
    assert_ne!(
        entry.url, raw_url,
        "the raw `url` field must NOT be picked when webpage_url is also present"
    );
}

#[test]
fn deserializes_with_null_title() {
    let json = r#"{"webpage_url":"https://example.com/p","title":null}"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(entry.url, "https://example.com/p");
    assert_eq!(
        entry.title, None,
        "explicit null title deserializes to None"
    );
}

#[test]
fn deserializes_with_thumbnail_field() {
    let json = r#"{
        "webpage_url":"https://example.com/p",
        "title":"T",
        "thumbnail":"https://i.ytimg.com/vi/abc/maxresdefault.jpg"
    }"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(
        entry.thumbnail.as_deref(),
        Some("https://i.ytimg.com/vi/abc/maxresdefault.jpg"),
        "UC 08 thumbnail field carried through"
    );
}

#[test]
fn deserializes_without_thumbnail_field() {
    let json = r#"{"webpage_url":"https://example.com/p","title":"T"}"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(
        entry.thumbnail, None,
        "absent thumbnail key deserializes to None — common with --flat-playlist"
    );
}

#[test]
fn deserializes_with_null_thumbnail() {
    let json = r#"{"webpage_url":"https://example.com/p","title":"T","thumbnail":null}"#;
    let entry: PlaylistEntry = serde_json::from_str(json).expect("deserialize succeeds");
    assert_eq!(
        entry.thumbnail, None,
        "explicit null thumbnail deserializes to None"
    );
}
