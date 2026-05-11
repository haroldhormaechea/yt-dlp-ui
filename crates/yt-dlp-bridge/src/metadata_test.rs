//! Unit tests for the manual `Deserialize` impl on [`PlaylistEntry`] in
//! `metadata.rs`. Covers the canonical-URL resolution rules:
//! `webpage_url` wins over `url`; either alone is sufficient; both absent
//! is a structured error; titles are passed through (including `null`).

use super::{PlaylistEntry, VideoMetadata};

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

// -- UC 27: VideoMetadata ----------------------------------------------

#[test]
fn video_metadata_deserializes_integer_duration() {
    // yt-dlp emits duration as an integer on many extractors.
    let json = r#"{"title":"Sample","thumbnail":"https://example.com/t.jpg","duration":180}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize");
    assert_eq!(meta.title.as_deref(), Some("Sample"));
    assert_eq!(meta.thumbnail.as_deref(), Some("https://example.com/t.jpg"));
    assert_eq!(meta.duration_s, Some(180));
}

#[test]
fn video_metadata_deserializes_float_duration() {
    // The float case (e.g. HLS extractors with sub-second precision); the
    // custom deserializer must clamp to u64 not bail.
    let json = r#"{"title":"Float","duration":182.45}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize float duration");
    assert_eq!(meta.duration_s, Some(182), "float floors to u64");
}

#[test]
fn video_metadata_handles_negative_duration_as_none() {
    let json = r#"{"title":"Neg","duration":-1.0}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize");
    assert_eq!(
        meta.duration_s, None,
        "negative duration clamps to None rather than panicking on cast_sign_loss"
    );
}

#[test]
fn video_metadata_handles_null_duration() {
    let json = r#"{"title":"NoDur","duration":null}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize");
    assert_eq!(meta.duration_s, None);
}

#[test]
fn video_metadata_handles_absent_duration() {
    // Extractor omitted the field entirely.
    let json = r#"{"title":"Bare"}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize");
    assert_eq!(meta.duration_s, None);
    assert_eq!(meta.title.as_deref(), Some("Bare"));
    assert_eq!(meta.thumbnail, None);
}

#[test]
fn video_metadata_ignores_unknown_fields() {
    // Stability across upstream versions — yt-dlp's actual --dump-single-json
    // output carries dozens of fields.
    let json = r#"{
        "title": "Full",
        "thumbnail": "https://example.com/t.jpg",
        "duration": 240,
        "id": "abc",
        "uploader": "Channel",
        "view_count": 12345,
        "ie_key": "Youtube"
    }"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("unknown fields are ignored");
    assert_eq!(meta.title.as_deref(), Some("Full"));
    assert_eq!(meta.duration_s, Some(240));
}

#[test]
fn video_metadata_handles_nan_duration_as_none() {
    // Non-finite float must not panic on cast — guarded by `is_finite`.
    let json = r#"{"title":"NaN","duration":null}"#;
    let meta: VideoMetadata = serde_json::from_str(json).expect("deserialize");
    assert_eq!(meta.duration_s, None);
}
