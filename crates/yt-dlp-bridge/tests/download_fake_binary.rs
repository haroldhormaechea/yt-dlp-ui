//! Integration tests for `start` (download supervisor) using a fake `yt-dlp`
//! binary that emits the bridge's progress markers.
//!
//! Skipped on Windows; the fake is a POSIX shell script.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::Notify;
use yt_dlp_bridge::{BridgeError, DownloadEvent, DownloadRequest, FormatPref, start};

// Serialises test bodies to prevent the Linux fork+exec ETXTBSY race:
// a concurrent test's brief write-FD on its own fake yt-dlp binary can be
// inherited by another test's fork, and then exec-ing that binary while the
// inherited FD is still open triggers "Text file busy" (ETXTBSY).
static FAKE_BIN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write_fake(script: &str) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = tmp.path().join("yt-dlp");
    fs::write(&bin, script).expect("write fake binary");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    (tmp, bin)
}

fn make_request(dir: &std::path::Path) -> DownloadRequest {
    DownloadRequest {
        url: "https://example.com/x".to_string(),
        format: FormatPref::BestHeuristic,
        dest_dir: dir.to_path_buf(),
        cookies_browser: None,
        js_runtime_path: None,
        ffmpeg_path: None,
    }
}

#[tokio::test]
async fn happy_path_emits_started_progress_finished() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // Fake yt-dlp emits a few progress lines, the after_move filepath marker,
    // then exits 0. The progress prefix must match what the bridge expects.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "yt-dlp-ui-progress 100 1000 1024 60"
echo "yt-dlp-ui-progress 500 1000 1024 30"
echo "yt-dlp-ui-progress 1000 1000 1024 0"
echo "yt-dlp-ui-filepath /tmp/out.mp4"
exit 0
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel);

    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    let result = handle.await.expect("handle join");
    assert!(result.is_ok(), "supervisor should succeed");

    // First event must be Started.
    assert!(matches!(events.first(), Some(DownloadEvent::Started)));
    // We must observe at least one Progress event.
    let progress_count = events
        .iter()
        .filter(|e| matches!(e, DownloadEvent::Progress { .. }))
        .count();
    assert!(progress_count >= 1, "at least one progress event");
    // Last event must be Finished with the file path. UC 08 widens the
    // variant with a `bytes: Option<u64>` snapshotted from the last
    // Progress's `total_bytes` (1000 here).
    if let Some(DownloadEvent::Finished { file_path, bytes }) = events.last() {
        assert_eq!(
            file_path.as_deref(),
            Some(std::path::Path::new("/tmp/out.mp4"))
        );
        assert_eq!(
            *bytes,
            Some(1000),
            "Finished.bytes must carry the last Progress's total_bytes (1000)"
        );
    } else {
        panic!("last event is not Finished: {:?}", events.last());
    }
}

#[tokio::test]
async fn finished_bytes_is_none_when_total_was_na() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // UC 08: when no Progress line ever carried a known total_bytes (live
    // streams or extractors that don't expose size), Finished.bytes is None.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "yt-dlp-ui-progress 100 NA 1024 60"
echo "yt-dlp-ui-progress 200 NA 1024 30"
echo "yt-dlp-ui-filepath /tmp/out.mp4"
exit 0
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel);

    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    let _ = handle.await.expect("handle join");

    if let Some(DownloadEvent::Finished { bytes, .. }) = events.last() {
        assert_eq!(*bytes, None, "no known total → Finished.bytes is None");
    } else {
        panic!("last event is not Finished: {:?}", events.last());
    }
}

#[tokio::test]
async fn nonzero_exit_emits_error_and_supervisor_returns_exited_with_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "fatal: nope" >&2
exit 2
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel);

    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    let result = handle.await.expect("handle join");
    let err = result.expect_err("non-zero exit must fail");
    if let BridgeError::ExitedWithError { code, stderr_tail } = err {
        assert_eq!(code, Some(2));
        assert!(stderr_tail.contains("fatal: nope"));
    } else {
        panic!("wrong error variant: {err:?}");
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Error { .. })),
        "Error event must appear in stream"
    );
}

#[tokio::test]
async fn cancel_mid_stream_emits_error_and_returns_cancelled() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // Fake yt-dlp keeps running until killed. The test cancels after a small
    // delay, expecting a Cancelled supervisor result.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "yt-dlp-ui-progress 100 1000 1024 60"
sleep 30
echo "yt-dlp-ui-filepath /tmp/out.mp4"
exit 0
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel.clone());

    // Wait for the first progress event so we know the supervisor is running,
    // then cancel.
    let drain = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(evt) = rx.recv().await {
            events.push(evt);
        }
        events
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel.notify_one();

    let result = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("supervisor must exit fast on cancel");
    let result = result.expect("handle join");
    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "supervisor must return BridgeError::Cancelled"
    );
    let events = drain.await.expect("drain join");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Error { message } if message == "cancelled")),
        "Error{{message='cancelled'}} must appear",
    );
}

// -- UC 05 ----------------------------------------------------------------

#[tokio::test]
async fn bot_check_stderr_yields_auth_required_not_exited_with_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // AC#1, AC#2: the supervisor must classify a yt-dlp bot-check stderr as
    // `BridgeError::AuthRequired` and NOT `ExitedWithError`. This test pins
    // that branching at the supervisor entry point.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "ERROR: Sign in to confirm you're not a bot. Use --cookies-from-browser for auth." >&2
exit 1
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel);

    while rx.recv().await.is_some() {}
    let err = handle
        .await
        .expect("handle join")
        .expect_err("bot-check must surface as an error");
    assert!(
        matches!(err, BridgeError::AuthRequired { .. }),
        "expected BridgeError::AuthRequired, got {err:?}"
    );
}

#[tokio::test]
async fn cookies_and_js_runtime_args_reach_yt_dlp() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // AC#13, AC#16: when DownloadRequest carries cookies_browser and
    // js_runtime_path, the supervisor must accept both fields and append
    // matching flags to yt-dlp's argv. We can't echo argv to STDERR here —
    // the bridge's bot-check matcher would treat any "--cookies-from-browser"
    // token in stderr as AuthRequired. Instead, the fake writes argv to a
    // sidecar file and exits 1 with empty stderr; the test reads the file.
    // dest kept alive via drop below — argv.log lives in this tempdir.
    let dest = tempfile::tempdir().unwrap();
    let argv_log = dest.path().join("argv.log");
    let script = format!(
        "#!/bin/sh\necho \"$@\" > '{}'\nexit 1\n",
        argv_log.display()
    );
    let (_tmp, bin) = write_fake(&script);
    let cancel = Arc::new(Notify::new());

    let req = DownloadRequest {
        url: "https://example.com/x".to_string(),
        format: FormatPref::BestHeuristic,
        dest_dir: dest.path().to_path_buf(),
        cookies_browser: Some("firefox".to_string()),
        js_runtime_path: Some(std::path::PathBuf::from("/opt/deno")),
        ffmpeg_path: None,
    };
    let (mut rx, handle) = start(&bin, req, cancel);

    while rx.recv().await.is_some() {}
    let err = handle.await.expect("handle join").expect_err("exit 1");
    assert!(
        matches!(err, BridgeError::ExitedWithError { .. }),
        "exit 1 with empty stderr must surface as ExitedWithError, got {err:?}"
    );

    let logged = std::fs::read_to_string(&argv_log).expect("argv.log written by fake");
    assert!(
        logged.contains("--cookies-from-browser"),
        "argv must include --cookies-from-browser: {logged}"
    );
    assert!(
        logged.contains("firefox"),
        "argv must include browser value: {logged}"
    );
    assert!(
        logged.contains("--js-runtimes"),
        "argv must include --js-runtimes: {logged}"
    );
    assert!(
        logged.contains("deno:/opt/deno"),
        "argv must include deno:<path> token: {logged}"
    );
    drop(dest); // explicit keep-alive: dest must outlive the await so read_to_string sees argv.log
}

// -- UC 02: two-stage cancel ---------------------------------------------

#[tokio::test]
async fn cancel_observes_sigterm_and_exits_within_grace() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // The fake yt-dlp installs a SIGTERM trap that exits 0 immediately. The
    // bridge's two-stage cancel body must observe the SIGTERM-triggered exit
    // BEFORE the 2 s grace timer fires, surface
    // `BridgeError::Cancelled`, and report it within ~1 s.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
trap 'exit 0' TERM
echo "yt-dlp-ui-progress 100 1000 1024 60"
# Sleep loop so SIGTERM has something to interrupt. POSIX sh's `sleep` is
# not interruptible on every platform, so loop short sleeps.
i=0
while [ $i -lt 600 ]; do
    sleep 0.1
    i=$((i+1))
done
exit 1
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel.clone());

    // Wait for the first Progress event (proves the trap is installed and
    // the shell is past startup) before firing cancel. Time-based waits
    // were flaky under heavy parallel test load.
    let mut got_progress = false;
    while let Some(evt) = rx.recv().await {
        if matches!(evt, DownloadEvent::Progress { .. }) {
            got_progress = true;
            break;
        }
    }
    assert!(
        got_progress,
        "fake yt-dlp must emit a Progress event before cancel"
    );
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });

    let before_cancel = std::time::Instant::now();
    cancel.notify_one();

    let result = tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("supervisor must exit fast on SIGTERM-honoring child");
    let elapsed = before_cancel.elapsed();
    let _ = drain.await;

    let result = result.expect("handle join");
    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "supervisor must return BridgeError::Cancelled (got {result:?})"
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "child honored SIGTERM, so cancel must complete well under the 2s grace; took {elapsed:?}"
    );
}

#[tokio::test]
async fn cancel_escalates_to_sigkill_after_grace() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // The fake catches SIGTERM with an active no-op handler that loops
    // forever, simulating a child that ignores SIGTERM (the empty `trap ''
    // TERM` form is unreliable across POSIX shells — bash on macOS exits on
    // SIGTERM during a `sleep` even with the trap in place; an active handler
    // forces the trap to actually run and stay alive). Only SIGKILL tears
    // the child down, so the bridge must wait the 2 s grace, escalate, and
    // surface `BridgeError::Cancelled`. Bounded at ~5 s so a regression
    // that misses the escalation path fails loud.
    //
    // The test waits for the first Progress event before firing cancel —
    // that proves the shell has finished startup AND installed the SIGTERM
    // trap (the trap line is BEFORE the `echo`). Time-based waits proved
    // flaky under heavy parallel test load.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
trap 'echo got_term >&2; true' TERM
echo "yt-dlp-ui-progress 100 1000 1024 60"
while true; do
    j=0
    while [ $j -lt 100000 ]; do
        j=$((j+1))
    done
done
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel.clone());

    // Wait for the first Progress event (proves the trap is installed),
    // then drain the rest in the background.
    let mut got_progress = false;
    while let Some(evt) = rx.recv().await {
        if matches!(evt, DownloadEvent::Progress { .. }) {
            got_progress = true;
            break;
        }
    }
    assert!(
        got_progress,
        "fake yt-dlp must emit a Progress event before cancel — without it, the trap may not be installed yet"
    );
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });

    let before_cancel = std::time::Instant::now();
    cancel.notify_one();

    let result = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("SIGKILL escalation must reap within 5s");
    let elapsed = before_cancel.elapsed();
    let _ = drain.await;

    let result = result.expect("handle join");
    assert!(
        matches!(result, Err(BridgeError::Cancelled)),
        "supervisor must still surface Cancelled after SIGKILL escalation"
    );
    // Grace period is 2 s; SIGKILL fires after that. Allow plenty of slack
    // for slow CI but verify we DID wait the grace (not under 1.5 s).
    assert!(
        elapsed >= Duration::from_millis(1500),
        "SIGTERM-ignoring child must NOT exit before grace expired; elapsed = {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(4),
        "SIGKILL escalation must have fired by 4s; elapsed = {elapsed:?}"
    );
}

#[tokio::test]
async fn destination_line_emits_partial_file_path_event() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // UC 02 AC#17: the bridge must capture `[download] Destination: <path>`
    // from yt-dlp's stdout and forward it as a `PartialFilePath` event so
    // the app can persist `partial_file_path` for later Remove cleanup.
    let (_tmp, bin) = write_fake(
        r#"#!/bin/sh
echo "[download] Destination: /tmp/clip.mp4.part"
echo "yt-dlp-ui-progress 100 1000 1024 60"
echo "yt-dlp-ui-filepath /tmp/clip.mp4"
exit 0
"#,
    );
    let dest = tempfile::tempdir().unwrap();
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bin, make_request(dest.path()), cancel);

    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    let result = handle.await.expect("handle join");
    assert!(result.is_ok(), "supervisor should succeed");

    let partial = events.iter().find_map(|e| match e {
        DownloadEvent::PartialFilePath { path } => Some(path.clone()),
        _ => None,
    });
    assert_eq!(
        partial.as_deref(),
        Some(std::path::Path::new("/tmp/clip.mp4.part")),
        "PartialFilePath must carry the path captured from stdout (events: {events:?})",
    );
}

#[tokio::test]
async fn spawn_failure_returns_spawn_error() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // Use a path that does not exist as the binary.
    let dest = tempfile::tempdir().unwrap();
    let bogus = dest.path().join("does-not-exist");
    let cancel = Arc::new(Notify::new());
    let (mut rx, handle) = start(&bogus, make_request(dest.path()), cancel);

    // Drain events (there will be none).
    while rx.recv().await.is_some() {}
    let err = handle
        .await
        .expect("handle join")
        .expect_err("spawn failure must propagate");
    assert!(
        matches!(err, BridgeError::Spawn(_)),
        "expected BridgeError::Spawn (got {err:?})"
    );
}

// -- UC 17 ----------------------------------------------------------------

#[tokio::test]
async fn ffmpeg_location_arg_reaches_yt_dlp_when_set() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // AC#2: when DownloadRequest carries ffmpeg_path = Some(<file>), the
    // supervisor must append `--ffmpeg-location <parent_dir>` to argv. The
    // directory form (parent of the binary) is intentional: a future ffprobe
    // dropped next to ffmpeg gets picked up without an argv-builder change.
    // dest kept alive via drop below — argv.log lives in this tempdir.
    let dest = tempfile::tempdir().unwrap();
    let argv_log = dest.path().join("argv.log");
    let script = format!(
        "#!/bin/sh\necho \"$@\" > '{}'\nexit 1\n",
        argv_log.display()
    );
    let (_tmp, bin) = write_fake(&script);
    let cancel = Arc::new(Notify::new());

    // Use a real directory + a fake ffmpeg file within it so the parent
    // resolution returns a deterministic path.
    let ffmpeg_dir = tempfile::tempdir().unwrap();
    let ffmpeg_file = ffmpeg_dir.path().join("ffmpeg");
    std::fs::write(&ffmpeg_file, b"#!/bin/sh\nexit 0\n").unwrap();

    let req = DownloadRequest {
        url: "https://example.com/x".to_string(),
        format: FormatPref::BestHeuristic,
        dest_dir: dest.path().to_path_buf(),
        cookies_browser: None,
        js_runtime_path: None,
        ffmpeg_path: Some(ffmpeg_file.clone()),
    };
    let (mut rx, handle) = start(&bin, req, cancel);

    while rx.recv().await.is_some() {}
    let _ = handle.await.expect("handle join");

    let logged = std::fs::read_to_string(&argv_log).expect("argv.log written by fake");
    assert!(
        logged.contains("--ffmpeg-location"),
        "argv must include --ffmpeg-location: {logged}"
    );
    let parent = ffmpeg_file.parent().unwrap().display().to_string();
    assert!(
        logged.contains(&parent),
        "argv must include the ffmpeg parent directory ({parent}): {logged}"
    );
    // The bare file path should NOT appear as a token after the flag —
    // this is the directory-form contract.
    let bare_file = ffmpeg_file.display().to_string();
    let flag_token = format!("--ffmpeg-location {bare_file}");
    assert!(
        !logged.contains(&flag_token),
        "argv must use directory form, not file form: {logged}"
    );
    drop(dest); // explicit keep-alive: dest must outlive the await so read_to_string sees argv.log
}

#[tokio::test]
async fn ffmpeg_location_arg_absent_when_path_none() {
    let _fake_bin_guard = FAKE_BIN_LOCK.lock().expect("FAKE_BIN_LOCK poisoned");
    // AC#2 inverse: when DownloadRequest.ffmpeg_path = None, no
    // --ffmpeg-location flag is appended. yt-dlp falls back to its own
    // PATH lookup (production code refuses to spawn in that case via
    // download_mgr's ffmpeg gate, so this is mostly a defensive guard).
    // dest kept alive via drop below — argv.log lives in this tempdir.
    let dest = tempfile::tempdir().unwrap();
    let argv_log = dest.path().join("argv.log");
    let script = format!(
        "#!/bin/sh\necho \"$@\" > '{}'\nexit 1\n",
        argv_log.display()
    );
    let (_tmp, bin) = write_fake(&script);
    let cancel = Arc::new(Notify::new());

    let req = DownloadRequest {
        url: "https://example.com/x".to_string(),
        format: FormatPref::BestHeuristic,
        dest_dir: dest.path().to_path_buf(),
        cookies_browser: None,
        js_runtime_path: None,
        ffmpeg_path: None,
    };
    let (mut rx, handle) = start(&bin, req, cancel);

    while rx.recv().await.is_some() {}
    let _ = handle.await.expect("handle join");

    let logged = std::fs::read_to_string(&argv_log).expect("argv.log written by fake");
    assert!(
        !logged.contains("--ffmpeg-location"),
        "argv must NOT include --ffmpeg-location when ffmpeg_path is None: {logged}"
    );
    drop(dest); // explicit keep-alive: dest must outlive the await so read_to_string sees argv.log
}
