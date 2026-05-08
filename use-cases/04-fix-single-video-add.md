# Use Case 04: Fix single-video URL add (PlaylistEntry deserialization)

## Summary

Fix the bug where adding any single-video URL via the UI fails with `Bridge(Json(Error("duplicate field 'url'", ...)))`. UC 01 implemented `crates/yt-dlp-bridge/src/metadata.rs::expand_playlist` using a `PlaylistEntry { url: String, title: Option<String> }` struct annotated with `serde(alias = "webpage_url")` so the canonical Rust `url` field could read either JSON `url` or JSON `webpage_url`. For real single-video output from `yt-dlp --flat-playlist --dump-json <single_video_url>`, the resulting JSON document contains BOTH a `url` field and a `webpage_url` field as separate top-level keys (they refer to the same resource but with different formats); serde sees the alias collide with the canonical key name and rejects the document with `"duplicate field 'url'"`. Replace the `serde(alias)` pattern with two separate `Option<String>` fields on the deserialization-only struct (`url` and `webpage_url`), and add a small conversion that prefers `webpage_url` when present, falls back to `url`, and returns a structured error if both are absent. The single-video signal-back to the caller (`expand_playlist` returning `Ok(vec![])` when one entry matches the input URL) is preserved. Regression test uses a JSON fixture matching the real yt-dlp single-video shape (both fields present).

## Acceptance Criteria

1. Adding any valid single-video URL via the UI (e.g. a `https://www.youtube.com/watch?v=…` URL) succeeds and produces exactly one queued row, with the title fetched via the existing single-video fallback path (`get_title`).
2. `metadata::expand_playlist` returns `Ok(vec![])` (the single-video signal) when the input URL refers to a single video AND the JSON document has its `webpage_url` (or `url` if `webpage_url` is absent) matching the input URL — preserving UC 01 § metadata.rs's documented contract.
3. `metadata::expand_playlist` returns `Ok(vec![entry, ...])` with one entry per playlist item when the input URL is a real playlist (multiple JSON lines, each with `url` and/or `webpage_url`).
4. `PlaylistEntry` deserialization no longer fails with `duplicate field 'url'` for any input shape that yt-dlp emits, including the two-field-both-present case that UC 01's `serde(alias)` setup rejected.
5. The internal struct used for deserialization carries both fields independently. The canonical URL exposed to callers is `webpage_url` if present, else `url`. If neither is present, deserialization (or post-deserialization conversion) yields a structured error mapped to `BridgeError::Json` or `BridgeError::Parse` — never a panic, never an unwrap.
6. A regression test exists at `crates/yt-dlp-bridge/src/metadata_test.rs` (or `crates/yt-dlp-bridge/tests/metadata_fake_binary.rs` if integration-style is more natural for the fake-binary pattern already used) using a JSON fixture matching the real yt-dlp single-video shape (both `url` and `webpage_url` present, plus a representative subset of other fields) and asserting successful deserialization plus the correct canonical-URL resolution.
7. The existing tests in `crates/yt-dlp-bridge/tests/metadata_fake_binary.rs` continue to pass without modification (e.g., `expand_playlist_returns_entries`, `expand_playlist_skips_blank_lines`, `get_title_*`).
8. All three gates pass: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`.
9. No new third-party Cargo dependencies are introduced. The fix uses only what `crates/yt-dlp-bridge` already depends on.

## Potential Pitfalls & Open Questions

- **Risk** — `serde(alias)` was the right idea but wrong execution: the alias maps two JSON names to the same Rust field, which makes the deserializer reject documents that contain both names. The canonical pattern for "prefer one of two field names" is two `Option` fields plus a post-deserialization picker, or a custom `Deserialize` impl. Pick the simpler one.
- **Risk** — Pure `serde(rename = "webpage_url")` would not solve the bug either, because the JSON has both names; renaming `url` to `webpage_url` would fail in the same way for the inverse case (only `url` present, no `webpage_url`). The two-field approach handles both shapes.
- **Edge case** — Some yt-dlp extractors produce only `url` and no `webpage_url` (especially for non-YouTube sites). The fallback must keep working — single-`url`-only documents must still deserialize.
- **Edge case** — Some yt-dlp extractors produce only `webpage_url` and no `url`. Same logic — `webpage_url` is sufficient.
- **Edge case** — Both fields present but referring to different URLs (e.g., `url` is a fragment URL, `webpage_url` is the canonical full URL). Per yt-dlp convention, `webpage_url` is the user-visible canonical form; prefer it.
- **Risk** — `expand_playlist` returns `Ok(vec![])` when "only one line is returned and webpage_url matches the input" (UC 01 proposal § metadata.rs). After this UC's struct change, the comparison must use the resolved canonical URL (the picked `webpage_url || url`), not raw struct fields. Keep the behavior.
- **Edge case** — Title field is unaffected. `title: Option<String>` was already correct in UC 01.
- **Risk** — Existing `metadata_fake_binary.rs` integration tests construct minimal JSON with only `url` (no `webpage_url`). They must keep passing — the fix preserves that path. The regression test adds the both-fields case as a new fixture, doesn't alter the existing one.
- **Assumption** — The user-supplied URL `https://www.youtube.com/watch?v=fryat2XxbWc` was a single-video URL (not a playlist). The bug applies to any single-video URL where yt-dlp's single-video JSON dump includes both `url` and `webpage_url` — which is the YouTube extractor's standard shape and likely most other major extractors.
- **Risk** — Manual smoke after the fix: re-launch the app, paste the same URL, confirm the row appears. UC 04's automated test verifies the deserializer; the manual smoke verifies the end-to-end add flow.

## Original Description

> User reported: launching the app and adding URL https://www.youtube.com/watch?v=fryat2XxbWc from the UI shows "failed to add url's". Logs show three identical failures:
>
>     Bridge(Json(Error("duplicate field `url`", line: 1, column: 611114)))
>
> The column count (~600k chars) confirms it's the full single-video JSON dump from `yt-dlp --flat-playlist --dump-json`. UC 01's `PlaylistEntry` uses `serde(alias = "webpage_url")` to read either field name as the canonical Rust `url` field, but real yt-dlp JSON for a single video contains BOTH `url` and `webpage_url` as separate top-level fields, which serde rejects as a duplicate. Fix by separating the two fields on the deserialization-only struct and picking the canonical one in code.
