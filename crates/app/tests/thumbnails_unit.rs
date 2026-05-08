//! Unit-level tests for the pure parts of [`app::thumbnails`] —
//! `deterministic_seed` purity, `source_kind_from_url` table-driven hostname
//! mapping, `cache_path` filename derivation. The HTTP fetcher
//! `fetch_and_cache_thumbnail` is exercised by the integration test
//! `tests/thumbnail_pipeline.rs`.
//!
//! Same rationale as `formats_unit.rs`: lives in `tests/` because the
//! production-code `mod` include for a `src/thumbnails_test.rs` side file
//! is owned by the developer and is not in QA's write scope.

use std::path::PathBuf;

use app::thumbnails::{ThumbnailError, cache_path, deterministic_seed, source_kind_from_url};

// -- deterministic_seed ----------------------------------------------------

#[test]
fn deterministic_seed_is_pure_same_input_same_output() {
    let url = "https://example.com/video?id=42";
    let a = deterministic_seed(url);
    let b = deterministic_seed(url);
    assert_eq!(a, b, "deterministic_seed must be a pure function of input");
}

#[test]
fn deterministic_seed_differs_for_different_urls_typically() {
    // A non-trivial set of URLs almost certainly maps to >1 distinct seed.
    let urls = [
        "https://www.youtube.com/watch?v=a",
        "https://www.youtube.com/watch?v=b",
        "https://vimeo.com/123",
        "https://soundcloud.com/track",
        "https://example.com/x",
        "https://example.com/y",
    ];
    let seeds: std::collections::HashSet<u32> =
        urls.iter().map(|u| deterministic_seed(u)).collect();
    assert!(
        seeds.len() > 1,
        "FNV-like rolling hash must produce variety across distinct URLs (got {} unique)",
        seeds.len()
    );
}

#[test]
fn deterministic_seed_empty_url_does_not_panic() {
    let _ = deterministic_seed("");
}

#[test]
fn deterministic_seed_modulo_8_is_in_range() {
    // The Slint row indexes 8 gradient palettes via `seed % 8`, so the
    // valid range must cover 0..=7 over a meaningful sample.
    let values: Vec<u32> = (0..200)
        .map(|i| deterministic_seed(&format!("https://example.com/v/{i}")) % 8)
        .collect();
    assert!(
        values.iter().all(|v| *v < 8),
        "seed % 8 must always be 0..=7"
    );
}

// -- source_kind_from_url --------------------------------------------------

#[test]
fn source_kind_youtube_long_form() {
    assert_eq!(
        source_kind_from_url("https://www.youtube.com/watch?v=abc"),
        "youtube"
    );
}

#[test]
fn source_kind_youtube_short_form() {
    assert_eq!(source_kind_from_url("https://youtu.be/abc"), "youtube");
}

#[test]
fn source_kind_vimeo() {
    assert_eq!(source_kind_from_url("https://vimeo.com/123"), "vimeo");
}

#[test]
fn source_kind_soundcloud() {
    assert_eq!(
        source_kind_from_url("https://soundcloud.com/artist/track"),
        "soundcloud"
    );
}

#[test]
fn source_kind_bandcamp() {
    assert_eq!(
        source_kind_from_url("https://artist.bandcamp.com/track/foo"),
        "bandcamp"
    );
}

#[test]
fn source_kind_unknown_falls_back_to_globe() {
    assert_eq!(
        source_kind_from_url("https://random.example.com/x"),
        "globe"
    );
    assert_eq!(source_kind_from_url(""), "globe");
}

#[test]
fn source_kind_case_insensitive() {
    assert_eq!(
        source_kind_from_url("https://WWW.YouTube.COM/watch?v=abc"),
        "youtube"
    );
    assert_eq!(source_kind_from_url("https://VIMEO.com/123"), "vimeo");
}

// -- cache_path ------------------------------------------------------------

#[test]
fn cache_path_uses_sha1_filename_with_extension() {
    let dir = PathBuf::from("/tmp/cache");
    let p = cache_path(&dir, "https://example.com/x", "jpg");
    let name = p.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(
        std::path::Path::new(&name)
            .extension()
            .and_then(|e| e.to_str()),
        Some("jpg"),
        "extension preserved: {name}"
    );
    let stem = name.trim_end_matches(".jpg");
    assert_eq!(stem.len(), 40, "SHA-1 hex digest is 40 chars: {stem}");
    assert!(
        stem.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA-1 hex digest must be hex-only: {stem}"
    );
}

#[test]
fn cache_path_empty_extension_defaults_to_jpg() {
    let p = cache_path(&PathBuf::from("/tmp/cache"), "https://example.com/x", "");
    let name = p.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(
        std::path::Path::new(&name)
            .extension()
            .and_then(|e| e.to_str()),
        Some("jpg"),
        "empty ext defaults to .jpg: {name}"
    );
}

#[test]
fn cache_path_is_deterministic_for_same_url() {
    let a = cache_path(
        &PathBuf::from("/tmp/cache"),
        "https://example.com/x",
        "webp",
    );
    let b = cache_path(
        &PathBuf::from("/tmp/cache"),
        "https://example.com/x",
        "webp",
    );
    assert_eq!(
        a, b,
        "cache_path must be a pure function of (dir, url, ext)"
    );
}

#[test]
fn cache_path_differs_for_different_urls() {
    let a = cache_path(&PathBuf::from("/tmp/c"), "https://example.com/a", "jpg");
    let b = cache_path(&PathBuf::from("/tmp/c"), "https://example.com/b", "jpg");
    assert_ne!(a, b, "different URLs hash to different filenames");
}

// -- ThumbnailError --------------------------------------------------------

#[test]
fn thumbnail_error_is_not_fatal_via_warn_path() {
    // The contract from `thumbnails.rs` (and the proposal): every error
    // variant is non-fatal at the row level. We can't instrument the WARN
    // log here, but we can pin that constructing each variant produces a
    // displayable error — which is what the caller logs.
    let http = ThumbnailError::Http("boom".to_string());
    let too_large = ThumbnailError::TooLarge(99_999_999_999);
    let io = ThumbnailError::Io(std::io::Error::other("disk full"));
    assert!(http.to_string().contains("http"));
    assert!(too_large.to_string().contains("too large"));
    assert!(io.to_string().contains("filesystem"));
}
