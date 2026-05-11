//! Integration tests for `expand_playlist` and `get_title` using a fake
//! `yt-dlp` binary realized as a shell script in a tempdir.
//!
//! Skipped on Windows; the script-based fake relies on POSIX shell.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::Notify;
use yt_dlp_bridge::{
    BridgeError, EnumerationOutcome, enumerate_playlist_cancellable, expand_playlist,
    fetch_metadata, get_thumbnail_url, get_title, get_title_cancellable,
};

// Serialises test bodies to prevent the Linux fork+exec ETXTBSY race:
// a concurrent test's brief write-FD on its own fake yt-dlp binary can be
// inherited by another test's fork, and then exec-ing that binary while the
// inherited FD is still open triggers "Text file busy" (ETXTBSY).
// tokio::sync::Mutex is used (not std::sync) to avoid the clippy
// `await_holding_lock` lint that fires when a std MutexGuard crosses `.await`.
static FAKE_BIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn write_fake(script: &str) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = tmp.path().join("yt-dlp");
    fs::write(&bin, script).expect("write fake binary");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    (tmp, bin)
}

#[tokio::test]
async fn get_title_returns_stdout_trimmed() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "  My Test Title  "
"#,
    );
    let title = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect("get_title");
    assert_eq!(title, "My Test Title");
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_empty_stdout_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake("#!/bin/sh\nexit 0\n");
    let err = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("must fail on empty stdout");
    assert!(matches!(err, BridgeError::ExitedWithError { .. }));
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_nonzero_exit_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "boom" >&2
exit 1
"#,
    );
    let err = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("non-zero exit must fail");
    if let BridgeError::ExitedWithError { code, stderr_tail } = err {
        assert_eq!(code, Some(1));
        assert!(stderr_tail.contains("boom"));
    } else {
        panic!("wrong error variant");
    }
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_timeout_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
sleep 5
echo "title"
"#,
    );
    let err = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_millis(100),
        None,
        None,
        None,
    )
    .await
    .expect_err("timeout must fail");
    assert!(matches!(err, BridgeError::ExitedWithError { .. }));
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_returns_entries() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"webpage_url":"https://example.com/p1","title":"P1"}'
echo '{"webpage_url":"https://example.com/p2","title":"P2"}'
echo '{"webpage_url":"https://example.com/p3","title":null}'
"#,
    );
    let entries = expand_playlist(&bin, "https://example.com/playlist", None, None, None)
        .await
        .expect("expand_playlist");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].url, "https://example.com/p1");
    assert_eq!(entries[0].title.as_deref(), Some("P1"));
    assert_eq!(entries[2].title, None, "null title deserializes to None");
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_single_entry_matching_input_returns_empty_vec() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Falls back to "single video" semantics.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"webpage_url":"https://example.com/single","title":"X"}'
"#,
    );
    let entries = expand_playlist(&bin, "https://example.com/single", None, None, None)
        .await
        .expect("expand_playlist");
    assert!(
        entries.is_empty(),
        "single entry whose url matches input → empty vec"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_nonzero_exit_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "extractor failed" >&2
exit 1
"#,
    );
    let err = expand_playlist(&bin, "https://example.com/x", None, None, None)
        .await
        .expect_err("non-zero exit must fail");
    if let BridgeError::ExitedWithError { code, stderr_tail } = err {
        assert_eq!(code, Some(1));
        assert!(stderr_tail.contains("extractor failed"));
    } else {
        panic!("wrong error variant");
    }
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_malformed_json_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake("#!/bin/sh\necho 'this is not json'\n");
    let err = expand_playlist(&bin, "https://example.com/x", None, None, None)
        .await
        .expect_err("malformed JSON must fail");
    assert!(
        matches!(err, BridgeError::Json(_)),
        "expected BridgeError::Json (got {err:?})"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_single_video_with_both_url_fields_returns_empty_vec() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Real yt-dlp single-video JSON dump shape: BOTH `url` and `webpage_url`
    // top-level fields, plus a representative subset of yt-dlp metadata.
    // Pre-fix (UC 01's `serde(alias = "webpage_url")` setup) this fails with
    // BridgeError::Json("duplicate field `url`"). Post-fix it must succeed
    // and emit the single-video signal (empty vec).
    let url = "https://www.youtube.com/watch?v=fryat2XxbWc";
    let script = format!(
        r#"#!/bin/sh
echo '{{"_type":"url","ie_key":"Youtube","id":"fryat2XxbWc","url":"{url}","webpage_url":"{url}","title":"Sample","duration":180}}'
"#
    );
    let (tmp, bin) = write_fake(&script);
    let entries = expand_playlist(&bin, url, None, None, None)
        .await
        .expect("both-fields single-video JSON must deserialize cleanly");
    assert!(
        entries.is_empty(),
        "single video matching input URL → empty vec (single-video signal)"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_skips_blank_lines() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"webpage_url":"https://example.com/p1","title":"P1"}'
echo ''
echo '{"webpage_url":"https://example.com/p2","title":"P2"}'
echo ''
"#,
    );
    let entries = expand_playlist(&bin, "https://example.com/playlist", None, None, None)
        .await
        .expect("expand_playlist");
    assert_eq!(entries.len(), 2);
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

// -- UC 05 ----------------------------------------------------------------

#[tokio::test]
async fn expand_playlist_bot_check_stderr_yields_auth_required() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // The fake yt-dlp emits the canonical bot-check ERROR line and exits
    // non-zero. The bridge must classify it as `AuthRequired`, NOT a generic
    // `ExitedWithError`. This pins the matcher → typed-error contract end-to-end.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "ERROR: [youtube] B10ECkQXQtU: Sign in to confirm you're not a bot. Use --cookies-from-browser." >&2
exit 1
"#,
    );
    let err = expand_playlist(
        &bin,
        "https://www.youtube.com/watch?v=B10ECkQXQtU",
        None,
        None,
        None,
    )
    .await
    .expect_err("bot-check stderr must surface as an error");
    assert!(
        matches!(err, BridgeError::AuthRequired { .. }),
        "expected BridgeError::AuthRequired, got {err:?}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

// -- UC 08 ----------------------------------------------------------------

#[tokio::test]
async fn get_thumbnail_url_returns_stdout_trimmed() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "  https://i.ytimg.com/vi/abc/maxresdefault.jpg  "
"#,
    );
    let url = get_thumbnail_url(
        &bin,
        "https://www.youtube.com/watch?v=abc",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect("get_thumbnail_url");
    assert_eq!(url, "https://i.ytimg.com/vi/abc/maxresdefault.jpg");
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_empty_stdout_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake("#!/bin/sh\nexit 0\n");
    let err = get_thumbnail_url(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("must fail on empty stdout");
    assert!(matches!(err, BridgeError::ExitedWithError { .. }));
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_bot_check_stderr_yields_auth_required() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "ERROR: [youtube] abc: Sign in to confirm you're not a bot. Use --cookies-from-browser." >&2
exit 1
"#,
    );
    let err = get_thumbnail_url(
        &bin,
        "https://www.youtube.com/watch?v=abc",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("bot-check must surface as an error");
    assert!(
        matches!(err, BridgeError::AuthRequired { .. }),
        "expected BridgeError::AuthRequired, got {err:?}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_timeout_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
sleep 5
echo "https://example.com/thumb.jpg"
"#,
    );
    let err = get_thumbnail_url(
        &bin,
        "https://example.com/x",
        Duration::from_millis(100),
        None,
        None,
        None,
    )
    .await
    .expect_err("timeout must fail");
    assert!(matches!(err, BridgeError::ExitedWithError { .. }));
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_forwards_cookies_and_deno_args() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let deno = std::path::PathBuf::from("/usr/local/bin/deno");
    let out = get_thumbnail_url(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        Some("firefox"),
        Some(&deno),
        None,
    )
    .await
    .expect("get_thumbnail_url");
    assert!(
        out.contains("--cookies-from-browser"),
        "argv must include --cookies-from-browser: {out}"
    );
    assert!(
        out.contains("firefox"),
        "argv must include browser value: {out}"
    );
    assert!(
        out.contains("--js-runtimes"),
        "argv must include --js-runtimes: {out}"
    );
    assert!(
        out.contains("deno:/usr/local/bin/deno"),
        "argv must include resolved deno path: {out}"
    );
    assert!(
        out.contains("%(thumbnail)s"),
        "argv must include --print %(thumbnail)s template token: {out}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_forwards_url_arg() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // The yt-dlp invocation must place the URL after all flags.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let target_url = "https://www.youtube.com/watch?v=specific123";
    let out = get_thumbnail_url(&bin, target_url, Duration::from_secs(5), None, None, None)
        .await
        .expect("get_thumbnail_url");
    assert!(out.contains(target_url), "argv must include the URL: {out}");
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

// -- UC 02 ----------------------------------------------------------------

#[tokio::test]
async fn get_title_cancellable_returns_title_when_not_cancelled() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Happy path: when cancel is never fired, the cancellable variant
    // returns the same trimmed title as `get_title`.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "  Cancellable Title  "
"#,
    );
    let cancel = Arc::new(Notify::new());
    let title = get_title_cancellable(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("get_title_cancellable");
    assert_eq!(title, "Cancellable Title");
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_cancellable_cancel_returns_cancelled_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // The fake hangs in a pure-shell loop with an active SIGTERM trap so
    // `terminate_with_grace` MUST escalate to SIGKILL. The cancellable
    // variant must surface `BridgeError::Cancelled` once the process is
    // reaped, regardless of whether SIGTERM or SIGKILL did the job.
    let (tmp, bin) = write_fake(
        r"#!/bin/sh
trap 'echo got_term >&2; true' TERM
while true; do
    j=0
    while [ $j -lt 100000 ]; do
        j=$((j+1))
    done
done
",
    );
    let cancel = Arc::new(Notify::new());
    let cancel_clone = cancel.clone();

    // Fire cancel after the shell has reliably finished startup. 300 ms
    // covers shell startup variance under heavy parallel test load on
    // macOS without dragging total test runtime up.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel_clone.notify_one();
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        get_title_cancellable(
            &bin,
            "https://example.com/cancel-me",
            // 1 min — well above the 2 s cancel grace, so the cancel
            // notify clearly wins over the timeout branch.
            Duration::from_mins(1),
            None,
            None,
            None,
            cancel,
        ),
    )
    .await
    .expect("cancellable title fetch must surface a result within 5s");

    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "expected BridgeError::Cancelled (got {result:?})"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_cancellable_timeout_distinct_from_cancel() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // The cancellable variant still honours its `timeout_dur`. With a tight
    // timeout and a hanging fake (and cancel never fired), it must surface
    // `ExitedWithError`, NOT `Cancelled`.
    let (tmp, bin) = write_fake(
        r"#!/bin/sh
trap 'true' TERM
while true; do
    j=0
    while [ $j -lt 100000 ]; do
        j=$((j+1))
    done
done
",
    );
    let cancel = Arc::new(Notify::new());
    let result = tokio::time::timeout(
        Duration::from_secs(6),
        get_title_cancellable(
            &bin,
            "https://example.com/timeout",
            Duration::from_millis(300),
            None,
            None,
            None,
            cancel,
        ),
    )
    .await
    .expect("must return within 6s");

    match result {
        Err(BridgeError::ExitedWithError { .. }) => {}
        other => panic!("timeout must surface as ExitedWithError, got {other:?}"),
    }
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_forwards_cookies_and_deno_args() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // AC#13, AC#16: when the caller passes cookies + js_runtime, the bridge
    // appends `--cookies-from-browser <name>` and `--js-runtimes deno:<path>`
    // to yt-dlp's argv. The fake echoes its argv into stdout where the bridge
    // would normally read the title — close enough to verify forwarding.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
# Echo the argv so the assertion can verify the flag presence.
echo "argv:$*"
"#,
    );
    let deno = std::path::PathBuf::from("/usr/local/bin/deno");
    let title = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        Some("chrome"),
        Some(&deno),
        None,
    )
    .await
    .expect("get_title");
    assert!(
        title.contains("--cookies-from-browser"),
        "argv echo must include --cookies-from-browser flag: {title}"
    );
    assert!(
        title.contains("chrome"),
        "argv echo must include the browser arg value: {title}"
    );
    assert!(
        title.contains("--js-runtimes"),
        "argv echo must include --js-runtimes flag: {title}"
    );
    assert!(
        title.contains("deno:/usr/local/bin/deno"),
        "argv echo must include the resolved deno path: {title}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

// -- UC 17 ----------------------------------------------------------------

/// Helper: stage a fake ffmpeg file inside a tempdir and return both. The
/// directory form of `--ffmpeg-location` (parent of the file) is what the
/// bridge appends — keeping both lifetimes alive lets tests assert on the
/// parent path without a stale-path race.
fn stage_fake_ffmpeg() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("ffmpeg tempdir");
    let file = tmp.path().join("ffmpeg");
    std::fs::write(&file, b"#!/bin/sh\nexit 0\n").expect("stage ffmpeg");
    (tmp, file)
}

#[tokio::test]
async fn get_title_forwards_ffmpeg_location_arg_when_set() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // AC#2: get_title must thread `--ffmpeg-location <parent_dir>` through
    // when the caller passes Some(<file>). The fake echoes argv as the
    // "title" so the assertion works on the same channel as
    // `get_title_forwards_cookies_and_deno_args`.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let (ffmpeg_tmp, ffmpeg_file) = stage_fake_ffmpeg();
    let parent = ffmpeg_file.parent().unwrap().to_path_buf();
    let title = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        Some(&ffmpeg_file),
    )
    .await
    .expect("get_title");
    assert!(
        title.contains("--ffmpeg-location"),
        "argv must include --ffmpeg-location: {title}"
    );
    assert!(
        title.contains(parent.to_str().unwrap()),
        "argv must include the ffmpeg parent dir ({}): {title}",
        parent.display()
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
    drop(ffmpeg_tmp); // keep fake ffmpeg alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_omits_ffmpeg_location_arg_when_none() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let title = get_title(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect("get_title");
    assert!(
        !title.contains("--ffmpeg-location"),
        "argv must NOT include --ffmpeg-location when ffmpeg_path = None: {title}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_title_cancellable_forwards_ffmpeg_location_arg_when_set() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let (ffmpeg_tmp, ffmpeg_file) = stage_fake_ffmpeg();
    let parent = ffmpeg_file.parent().unwrap().to_path_buf();
    let cancel = Arc::new(Notify::new());
    let title = get_title_cancellable(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        Some(&ffmpeg_file),
        cancel,
    )
    .await
    .expect("get_title_cancellable");
    assert!(
        title.contains("--ffmpeg-location"),
        "argv must include --ffmpeg-location: {title}"
    );
    assert!(
        title.contains(parent.to_str().unwrap()),
        "argv must include the ffmpeg parent dir: {title}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
    drop(ffmpeg_tmp); // keep fake ffmpeg alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn get_thumbnail_url_forwards_ffmpeg_location_arg_when_set() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "argv:$*"
"#,
    );
    let (ffmpeg_tmp, ffmpeg_file) = stage_fake_ffmpeg();
    let parent = ffmpeg_file.parent().unwrap().to_path_buf();
    let out = get_thumbnail_url(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        Some(&ffmpeg_file),
    )
    .await
    .expect("get_thumbnail_url");
    assert!(
        out.contains("--ffmpeg-location"),
        "argv must include --ffmpeg-location: {out}"
    );
    assert!(
        out.contains(parent.to_str().unwrap()),
        "argv must include the ffmpeg parent dir: {out}"
    );
    drop(tmp); // keep fake yt-dlp alive across .await: Rust 2024 async-drop
    drop(ffmpeg_tmp); // keep fake ffmpeg alive across .await: Rust 2024 async-drop
}

#[tokio::test]
async fn expand_playlist_forwards_ffmpeg_location_arg_when_set() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // expand_playlist's stdout is consumed as JSON, so we cannot echo argv
    // there. Use a sidecar file instead (same pattern as
    // `cookies_and_js_runtime_args_reach_yt_dlp` in download_fake_binary.rs).
    // dest kept alive via drop below — argv.log lives in this tempdir.
    let dest = tempfile::tempdir().unwrap();
    let argv_log = dest.path().join("argv.log");
    let script = format!(
        "#!/bin/sh\necho \"$@\" > '{}'\nexit 1\n",
        argv_log.display()
    );
    let (tmp, bin) = write_fake(&script);
    let (ffmpeg_tmp, ffmpeg_file) = stage_fake_ffmpeg();
    let parent = ffmpeg_file.parent().unwrap().to_path_buf();

    let _ = expand_playlist(
        &bin,
        "https://example.com/playlist",
        None,
        None,
        Some(&ffmpeg_file),
    )
    .await;

    let logged = std::fs::read_to_string(&argv_log).expect("argv.log written by fake");
    assert!(
        logged.contains("--ffmpeg-location"),
        "argv must include --ffmpeg-location: {logged}"
    );
    assert!(
        logged.contains(parent.to_str().unwrap()),
        "argv must include the ffmpeg parent dir: {logged}"
    );
    // keep all three tempdirs alive across .await: Rust 2024 async-drop
    drop(tmp);
    drop(ffmpeg_tmp);
    drop(dest);
}

// -- UC 27 ----------------------------------------------------------------

#[tokio::test]
async fn fetch_metadata_returns_title_thumbnail_and_duration() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"title":"Sample","thumbnail":"https://i.example.com/t.jpg","duration":180}'
"#,
    );
    let cancel = Arc::new(Notify::new());
    let meta = fetch_metadata(
        &bin,
        "https://example.com/v",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("fetch_metadata");
    assert_eq!(meta.title.as_deref(), Some("Sample"));
    assert_eq!(
        meta.thumbnail.as_deref(),
        Some("https://i.example.com/t.jpg")
    );
    assert_eq!(meta.duration_s, Some(180));
    drop(tmp);
}

#[tokio::test]
async fn fetch_metadata_handles_float_duration() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"title":"Float","duration":182.45}'
"#,
    );
    let cancel = Arc::new(Notify::new());
    let meta = fetch_metadata(
        &bin,
        "https://example.com/v",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("fetch_metadata");
    assert_eq!(
        meta.duration_s,
        Some(182),
        "float floors to integer seconds"
    );
    drop(tmp);
}

#[tokio::test]
async fn fetch_metadata_empty_stdout_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake("#!/bin/sh\nexit 0\n");
    let cancel = Arc::new(Notify::new());
    let err = fetch_metadata(
        &bin,
        "https://example.com/v",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect_err("empty stdout must fail");
    assert!(matches!(err, BridgeError::ExitedWithError { .. }));
    drop(tmp);
}

#[tokio::test]
async fn fetch_metadata_bot_check_stderr_yields_auth_required() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "ERROR: [youtube] abc: Sign in to confirm you're not a bot. Use --cookies-from-browser." >&2
exit 1
"#,
    );
    let cancel = Arc::new(Notify::new());
    let err = fetch_metadata(
        &bin,
        "https://example.com/v",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect_err("bot-check must surface");
    assert!(
        matches!(err, BridgeError::AuthRequired { .. }),
        "expected BridgeError::AuthRequired, got {err:?}"
    );
    drop(tmp);
}

#[tokio::test]
async fn fetch_metadata_cancel_returns_cancelled_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Fake hangs with a SIGTERM trap so terminate_with_grace must escalate
    // to SIGKILL. Mirrors the get_title_cancellable cancel test.
    let (tmp, bin) = write_fake(
        r"#!/bin/sh
trap 'echo got_term >&2; true' TERM
while true; do
    j=0
    while [ $j -lt 100000 ]; do
        j=$((j+1))
    done
done
",
    );
    let cancel = Arc::new(Notify::new());
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel_clone.notify_one();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        fetch_metadata(
            &bin,
            "https://example.com/cancel-me",
            Duration::from_mins(1),
            None,
            None,
            None,
            cancel,
        ),
    )
    .await
    .expect("must return within 5s");
    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "expected BridgeError::Cancelled, got {result:?}"
    );
    drop(tmp);
}

#[tokio::test]
async fn fetch_metadata_malformed_json_is_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake("#!/bin/sh\necho 'not json'\n");
    let cancel = Arc::new(Notify::new());
    let err = fetch_metadata(
        &bin,
        "https://example.com/v",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect_err("malformed JSON must fail");
    assert!(
        matches!(err, BridgeError::Json(_)),
        "expected BridgeError::Json, got {err:?}"
    );
    drop(tmp);
}

#[tokio::test]
async fn enumerate_playlist_cancellable_single_line_matching_input_returns_single_video() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Mirrors expand_playlist's single-video fall-through: one JSON line
    // whose URL matches the input → EnumerationOutcome::SingleVideo.
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"webpage_url":"https://example.com/single","title":"X"}'
"#,
    );
    let cancel = Arc::new(Notify::new());
    let outcome = enumerate_playlist_cancellable(
        &bin,
        "https://example.com/single",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("enumerate_playlist_cancellable");
    assert!(
        matches!(outcome, EnumerationOutcome::SingleVideo),
        "single matching line → SingleVideo"
    );
    drop(tmp);
}

#[tokio::test]
async fn enumerate_playlist_cancellable_multi_line_returns_playlist() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo '{"webpage_url":"https://example.com/p1","title":"P1"}'
echo '{"webpage_url":"https://example.com/p2","title":null}'
echo '{"webpage_url":"https://example.com/p3","title":"P3"}'
"#,
    );
    let cancel = Arc::new(Notify::new());
    let outcome = enumerate_playlist_cancellable(
        &bin,
        "https://example.com/playlist",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("enumerate_playlist_cancellable");
    match outcome {
        EnumerationOutcome::Playlist(entries) => {
            assert_eq!(entries.len(), 3, "all three entries returned");
            assert_eq!(entries[0].url, "https://example.com/p1");
            assert_eq!(entries[1].title, None);
            assert_eq!(entries[2].title.as_deref(), Some("P3"));
        }
        other @ EnumerationOutcome::SingleVideo => panic!("expected Playlist, got {other:?}"),
    }
    drop(tmp);
}

#[tokio::test]
async fn enumerate_playlist_cancellable_empty_stdout_yields_single_video() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    // Defensive: yt-dlp returned nothing parseable. Treat as single video
    // so the caller's metadata fallback runs against the input URL.
    let (tmp, bin) = write_fake("#!/bin/sh\nexit 0\n");
    let cancel = Arc::new(Notify::new());
    let outcome = enumerate_playlist_cancellable(
        &bin,
        "https://example.com/x",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect("enumerate_playlist_cancellable");
    assert!(matches!(outcome, EnumerationOutcome::SingleVideo));
    drop(tmp);
}

#[tokio::test]
async fn enumerate_playlist_cancellable_cancel_returns_cancelled_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r"#!/bin/sh
trap 'echo got_term >&2; true' TERM
while true; do
    j=0
    while [ $j -lt 100000 ]; do
        j=$((j+1))
    done
done
",
    );
    let cancel = Arc::new(Notify::new());
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel_clone.notify_one();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        enumerate_playlist_cancellable(
            &bin,
            "https://example.com/cancel-enum",
            Duration::from_mins(1),
            None,
            None,
            None,
            cancel,
        ),
    )
    .await
    .expect("must return within 5s");
    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "expected BridgeError::Cancelled, got {result:?}"
    );
    drop(tmp);
}

#[tokio::test]
async fn enumerate_playlist_cancellable_bot_check_yields_auth_required() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().await;
    let (tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "ERROR: [youtube] B10ECkQXQtU: Sign in to confirm you're not a bot. Use --cookies-from-browser." >&2
exit 1
"#,
    );
    let cancel = Arc::new(Notify::new());
    let err = enumerate_playlist_cancellable(
        &bin,
        "https://www.youtube.com/watch?v=B10ECkQXQtU",
        Duration::from_secs(5),
        None,
        None,
        None,
        cancel,
    )
    .await
    .expect_err("bot-check must surface");
    assert!(
        matches!(err, BridgeError::AuthRequired { .. }),
        "expected BridgeError::AuthRequired, got {err:?}"
    );
    drop(tmp);
}
