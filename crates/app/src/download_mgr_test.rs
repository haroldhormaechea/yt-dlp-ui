//! Tests for [`crate::download_mgr::DownloadManager`].
//!
//! Uses an in-memory fake [`super::BridgeOps`] impl to drive the manager
//! without spawning a real `yt-dlp` binary.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::{Mutex as TokioMutex, Notify, mpsc};
use yt_dlp_bridge::{BridgeError, DownloadEvent, DownloadRequest, FormatPref, PlaylistEntry};

use super::{AddError, AddOutcome, BridgeOps, DownloadManager, UiEvent};
use crate::browsers::Browser;
use crate::db::Db;
use crate::db::settings;
use crate::model::QueueStatus;

/// Per-call outcome the fake bridge serves for `start_download`. Pulled from
/// `FakeBehavior::download_outcomes` in FIFO order; if the queue is empty the
/// fake falls back to "wait for `release_one`, then succeed" (the original
/// behavior preserved for UC 01–04 tests).
#[derive(Clone)]
enum DownloadOutcome {
    /// Emit `DownloadEvent::Error { stderr_tail }` and return `BridgeError::AuthRequired`.
    AuthRequired { stderr_tail: String },
}

/// Per-call outcome for `expand_playlist`. The default behavior is to return
/// `playlist_entries`. UC 05 needs the explicit `AuthRequired` branch.
#[derive(Clone)]
enum ExpandOutcome {
    AuthRequired { stderr_tail: String },
}

/// Records a captured `DownloadRequest` for assertion (UC 05 retry
/// verification, UC 16 destination resolution, UC 17 ffmpeg propagation).
#[derive(Clone, Debug)]
struct CapturedRequest {
    cookies_browser: Option<String>,
    js_runtime_path: Option<PathBuf>,
    ffmpeg_path: Option<PathBuf>,
    dest_dir: PathBuf,
}

/// Configurable behavior the fake bridge will apply per-method.
#[derive(Default)]
struct FakeBehavior {
    /// `expand_playlist` returns these entries; an empty Vec means
    /// "single-video fallback".
    playlist_entries: Vec<PlaylistEntry>,
    /// When set, `expand_playlist` returns this error instead of `playlist_entries`.
    expand_error: Option<&'static str>,
    /// Per-call expand outcomes (FIFO). Drained on each call; falls back to
    /// `expand_error` then `playlist_entries` when empty.
    expand_outcomes: Vec<ExpandOutcome>,
    /// Title returned by `fetch_title` (default "Real Title").
    title: Option<String>,
    /// When set, `fetch_title` returns this error.
    fetch_error: Option<&'static str>,
    /// How many `start_download` calls have been observed; updated by the
    /// fake. Wrapped in `TokioMutex` so the assertion side can read it.
    download_calls: u64,
    /// Active downloads — incremented when `start_download` runs, decremented
    /// when the supervisor exits. Used to assert concurrency cap.
    active_now: u64,
    /// Maximum concurrent downloads observed in this run.
    peak_active: u64,
    /// Per-call download outcomes (FIFO). Drained on each call; if empty,
    /// the fake holds the supervisor on `release_one`.
    download_outcomes: Vec<DownloadOutcome>,
    /// Captured `DownloadRequest` for each `start_download` call (in order).
    captured_requests: Vec<CapturedRequest>,
    /// UC 02: when set, `fetch_title_cancellable` sleeps this long before
    /// returning, giving tests a window in which to fire `cancel_one` and
    /// observe the metadata-cancel path. When `None`, the cancellable
    /// variant resolves immediately (matching `fetch_title`'s shape so
    /// pre-UC-02 tests stay green).
    metadata_cancel_dwell: Option<Duration>,
}

#[derive(Clone)]
struct FakeBridge {
    behavior: Arc<TokioMutex<FakeBehavior>>,
    /// Channel each spawned download holds open until the supervisor is told
    /// to "finish". Test code calls `release_one` to let one in-flight download
    /// complete.
    release: Arc<tokio::sync::Semaphore>,
}

impl FakeBridge {
    fn new(behavior: FakeBehavior) -> Self {
        Self {
            behavior: Arc::new(TokioMutex::new(behavior)),
            release: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }

    /// Releases one in-flight download — the next finishing supervisor will
    /// emit `Finished` and exit.
    fn release_one(&self) {
        self.release.add_permits(1);
    }
}

impl BridgeOps for FakeBridge {
    fn fetch_title(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&std::path::Path>,
        _ffmpeg_path: Option<&std::path::Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send {
        let behavior = self.behavior.clone();
        async move {
            let b = behavior.lock().await;
            if let Some(err) = b.fetch_error {
                Err(BridgeError::ExitedWithError {
                    code: Some(1),
                    stderr_tail: err.to_string(),
                })
            } else {
                Ok(b.title.clone().unwrap_or_else(|| "Real Title".to_string()))
            }
        }
    }

    fn expand_playlist(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&std::path::Path>,
        _ffmpeg_path: Option<&std::path::Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<Vec<PlaylistEntry>>> + Send {
        let behavior = self.behavior.clone();
        async move {
            let mut b = behavior.lock().await;
            if !b.expand_outcomes.is_empty() {
                let outcome = b.expand_outcomes.remove(0);
                return match outcome {
                    ExpandOutcome::AuthRequired { stderr_tail } => {
                        Err(BridgeError::AuthRequired { stderr_tail })
                    }
                };
            }
            if let Some(err) = b.expand_error {
                return Err(BridgeError::ExitedWithError {
                    code: Some(1),
                    stderr_tail: err.to_string(),
                });
            }
            Ok(b.playlist_entries.clone())
        }
    }

    async fn fetch_thumbnail_url(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&std::path::Path>,
        _ffmpeg_path: Option<&std::path::Path>,
    ) -> yt_dlp_bridge::Result<String> {
        // UC 08: tests in this file don't exercise the thumbnail pipeline
        // (covered by `tests/thumbnail_pipeline.rs`). Returning an error keeps
        // existing UC 01-05 behaviour: the fetcher logs at WARN, the row
        // keeps its gradient, and these tests' assertions are unaffected.
        Err(BridgeError::ExitedWithError {
            code: Some(1),
            stderr_tail: "fake bridge: thumbnail fetch not configured".to_string(),
        })
    }

    fn fetch_title_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&std::path::Path>,
        _ffmpeg_path: Option<&std::path::Path>,
        cancel: Arc<Notify>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send {
        // UC 02: mirrors `fetch_title` semantics but races the configured
        // outcome against the cancel notify. When `metadata_cancel.dwell`
        // is set, the fake holds for that long before resolving, giving
        // tests a window to fire `cancel_one` and assert the
        // metadata-cancel path. When unset, behaviour matches
        // `fetch_title` so existing UC 01-05 tests remain stable.
        let behavior = self.behavior.clone();
        async move {
            let (dwell, fetch_error, title) = {
                let b = behavior.lock().await;
                (b.metadata_cancel_dwell, b.fetch_error, b.title.clone())
            };
            if let Some(d) = dwell {
                tokio::select! {
                    () = tokio::time::sleep(d) => {}
                    () = cancel.notified() => {
                        return Err(BridgeError::Cancelled);
                    }
                }
            }
            if let Some(err) = fetch_error {
                Err(BridgeError::ExitedWithError {
                    code: Some(1),
                    stderr_tail: err.to_string(),
                })
            } else {
                Ok(title.unwrap_or_else(|| "Real Title".to_string()))
            }
        }
    }

    fn start_download(
        &self,
        req: DownloadRequest,
        cancel: Arc<Notify>,
    ) -> (
        mpsc::Receiver<DownloadEvent>,
        tokio::task::JoinHandle<yt_dlp_bridge::Result<()>>,
    ) {
        let (tx, rx) = mpsc::channel(8);
        let release = self.release.clone();
        let behavior = self.behavior.clone();
        // Capture the request first.
        let captured = CapturedRequest {
            cookies_browser: req.cookies_browser.clone(),
            js_runtime_path: req.js_runtime_path.clone(),
            ffmpeg_path: req.ffmpeg_path.clone(),
            dest_dir: req.dest_dir.clone(),
        };
        let handle = tokio::spawn(async move {
            // Bookkeeping + capture.
            let outcome: Option<DownloadOutcome> = {
                let mut b = behavior.lock().await;
                b.download_calls += 1;
                b.active_now += 1;
                if b.active_now > b.peak_active {
                    b.peak_active = b.active_now;
                }
                b.captured_requests.push(captured);
                if b.download_outcomes.is_empty() {
                    None
                } else {
                    Some(b.download_outcomes.remove(0))
                }
            };
            let _ = tx.send(DownloadEvent::Started).await;
            match outcome {
                Some(DownloadOutcome::AuthRequired { stderr_tail }) => {
                    let _ = tx
                        .send(DownloadEvent::Error {
                            message: stderr_tail.clone(),
                        })
                        .await;
                    {
                        let mut b = behavior.lock().await;
                        b.active_now -= 1;
                    }
                    Err(BridgeError::AuthRequired { stderr_tail })
                }
                None => {
                    // Default: hold the slot until either the test releases
                    // it (success path) or the cancel notify fires
                    // (UC 02 in-flight cancel path). Whichever wins decides
                    // the supervisor's terminal outcome.
                    let cancelled = tokio::select! {
                        permit = release.acquire() => {
                            permit.expect("release semaphore not closed").forget();
                            false
                        }
                        () = cancel.notified() => true,
                    };
                    if cancelled {
                        let _ = tx
                            .send(DownloadEvent::Error {
                                message: "cancelled".to_string(),
                            })
                            .await;
                        {
                            let mut b = behavior.lock().await;
                            b.active_now -= 1;
                        }
                        Err(BridgeError::Cancelled)
                    } else {
                        let _ = tx
                            .send(DownloadEvent::Finished {
                                file_path: None,
                                bytes: Some(1024),
                            })
                            .await;
                        {
                            let mut b = behavior.lock().await;
                            b.active_now -= 1;
                        }
                        Ok(())
                    }
                }
            }
        });
        (rx, handle)
    }
}

struct TestEnv {
    _tmp: TempDir,
    db: Db,
    bridge: FakeBridge,
    _ui_tx: mpsc::Sender<UiEvent>,
    ui_rx: Arc<TokioMutex<mpsc::Receiver<UiEvent>>>,
    manager: DownloadManager<FakeBridge>,
}

fn setup(behavior: FakeBehavior, cap: u32) -> TestEnv {
    setup_full(behavior, cap, Vec::new(), None)
}

fn setup_full(
    behavior: FakeBehavior,
    cap: u32,
    detected_browsers: Vec<Browser>,
    js_runtime_path: Option<PathBuf>,
) -> TestEnv {
    // UC 17: stage a dummy bundled-ffmpeg path so the manager's pre-spawn
    // ffmpeg gate (download_mgr.rs § "UC 17: unconditional ffmpeg gate")
    // does not short-circuit every queued row to `error`. Tests that
    // explicitly verify the gate (e.g. `manager_with_no_ffmpeg_marks_row_error_no_spawn`)
    // build their TestEnv directly via `setup_full_with_ffmpeg` and pass `None`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dummy_ffmpeg = tmp.path().join("ffmpeg");
    std::fs::write(&dummy_ffmpeg, b"#!/bin/sh\nexit 0\n").expect("stage dummy ffmpeg");
    setup_full_with_ffmpeg(
        behavior,
        cap,
        detected_browsers,
        js_runtime_path,
        Some(dummy_ffmpeg),
        tmp,
    )
}

fn setup_full_with_ffmpeg(
    behavior: FakeBehavior,
    cap: u32,
    detected_browsers: Vec<Browser>,
    js_runtime_path: Option<PathBuf>,
    ffmpeg_path: Option<PathBuf>,
    tmp: TempDir,
) -> TestEnv {
    let db_path = tmp.path().join("db.sqlite");
    let db = Db::open(&db_path).expect("open db");

    // Seed dest_dir setting so we don't depend on $HOME.
    db.with_conn(|c| settings::set_dest_dir(c, tmp.path()))
        .unwrap();

    let bridge = FakeBridge::new(behavior);
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(64);
    let thumbnail_cache_dir = tmp.path().join("thumbnails");
    let manager = DownloadManager::new(
        db.clone(),
        bridge.clone(),
        ui_tx.clone(),
        cap,
        detected_browsers,
        js_runtime_path,
        ffmpeg_path,
        thumbnail_cache_dir,
    );

    TestEnv {
        _tmp: tmp,
        db,
        bridge,
        _ui_tx: ui_tx,
        ui_rx: Arc::new(TokioMutex::new(ui_rx)),
        manager,
    }
}

async fn drain_ui(rx: &Arc<TokioMutex<mpsc::Receiver<UiEvent>>>) {
    let mut rx = rx.lock().await;
    while rx.try_recv().is_ok() {}
}

#[tokio::test]
async fn add_url_single_video_inserts_one_row() {
    let env = setup(FakeBehavior::default(), 1);
    let outcome = env
        .manager
        .add_url("https://example.com/single".to_string(), None)
        .await
        .expect("add_url");
    assert!(matches!(outcome, AddOutcome::Inserted { count: 1 }));

    // Wait briefly for the title fetch task.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let rows = env.manager.list_ui_rows().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].url, "https://example.com/single");
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn add_url_duplicate_returns_duplicate_url_error() {
    let env = setup(FakeBehavior::default(), 1);
    let url = "https://example.com/dup".to_string();
    env.manager
        .add_url(url.clone(), None)
        .await
        .expect("first add");

    let err = env
        .manager
        .add_url(url.clone(), None)
        .await
        .expect_err("second add must fail");
    assert!(matches!(err, AddError::DuplicateUrl(ref u) if u == &url));

    let rows = env.manager.list_ui_rows().await.unwrap();
    assert_eq!(rows.len(), 1, "no duplicate row created");
}

#[tokio::test]
async fn add_url_playlist_inserts_n_rows() {
    let entries = vec![
        PlaylistEntry {
            url: "https://example.com/p1".into(),
            title: Some("p1 title".into()),
            thumbnail: None,
        },
        PlaylistEntry {
            url: "https://example.com/p2".into(),
            title: Some("p2 title".into()),
            thumbnail: None,
        },
        PlaylistEntry {
            url: "https://example.com/p3".into(),
            title: None,
            thumbnail: None,
        },
    ];
    let env = setup(
        FakeBehavior {
            playlist_entries: entries,
            ..Default::default()
        },
        1,
    );

    let outcome = env
        .manager
        .add_url("https://example.com/playlist".to_string(), None)
        .await
        .expect("add_url");
    assert!(matches!(outcome, AddOutcome::Inserted { count: 3 }));

    // Allow the title-fetch task for the entry with no title to run.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let rows = env.manager.list_ui_rows().await.unwrap();
    let urls: Vec<String> = rows.iter().map(|r| r.url.clone()).collect();
    assert_eq!(urls.len(), 3);
    assert!(urls.iter().any(|u| u == "https://example.com/p1"));
    assert!(urls.iter().any(|u| u == "https://example.com/p2"));
    assert!(urls.iter().any(|u| u == "https://example.com/p3"));
}

#[tokio::test]
async fn playlist_skips_duplicates_within_run() {
    let entries = vec![
        PlaylistEntry {
            url: "https://example.com/x".into(),
            title: Some("x".into()),
            thumbnail: None,
        },
        PlaylistEntry {
            url: "https://example.com/x".into(),
            title: Some("x dup".into()),
            thumbnail: None,
        },
    ];
    let env = setup(
        FakeBehavior {
            playlist_entries: entries,
            ..Default::default()
        },
        1,
    );
    let outcome = env
        .manager
        .add_url("https://example.com/playlist".to_string(), None)
        .await
        .expect("add_url");
    assert!(matches!(outcome, AddOutcome::Inserted { count: 1 }));
}

#[tokio::test]
async fn add_url_propagates_bridge_error_on_expansion_failure() {
    let env = setup(
        FakeBehavior {
            expand_error: Some("boom"),
            ..Default::default()
        },
        1,
    );
    let err = env
        .manager
        .add_url("https://example.com/x".to_string(), None)
        .await
        .expect_err("expand_playlist failure must surface");
    assert!(matches!(err, AddError::Bridge(_)));
}

#[tokio::test]
async fn concurrency_cap_is_enforced() {
    // Cap = 2; add 5 distinct URLs (single-video each). Without releasing any,
    // exactly 2 should ever be in_flight at once.
    let env = setup(FakeBehavior::default(), 2);
    for i in 0..5 {
        let url = format!("https://example.com/item-{i}");
        env.manager.add_url(url, None).await.expect("add_url");
    }

    // Give the runner time to promote.
    tokio::time::sleep(Duration::from_millis(150)).await;

    {
        let b = env.bridge.behavior.lock().await;
        assert!(
            b.peak_active <= 2,
            "peak_active = {} must not exceed cap of 2",
            b.peak_active
        );
        assert_eq!(b.active_now, 2, "exactly 2 are in-flight while held");
    }

    // Release all so the test exits cleanly.
    for _ in 0..5 {
        env.bridge.release_one();
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn start_download_marks_row_in_flight() {
    let env = setup(FakeBehavior::default(), 1);
    env.manager
        .add_url("https://example.com/single".to_string(), None)
        .await
        .expect("add");

    // Give the manager runner time to promote and call start_download.
    tokio::time::sleep(Duration::from_millis(120)).await;

    let row = env.manager.list_ui_rows().await.unwrap();
    assert_eq!(row.len(), 1);
    // The fake holds the download until release_one — the row should be
    // in_flight at this point.
    assert_eq!(row[0].status, QueueStatus::InFlight);

    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(120)).await;

    let row = env.manager.list_ui_rows().await.unwrap();
    assert_eq!(row[0].status, QueueStatus::Done);
}

#[tokio::test]
async fn format_snapshot_at_enqueue_dest_resolved_at_spawn() {
    // UC 16 AC#5 — pins BOTH clauses in one test:
    //   - Items already `in_flight` keep their ORIGINAL dest_dir even when
    //     settings change mid-flight ("before" row).
    //   - Items still `queued` pick up the NEW dest_dir at spawn time
    //     ("after" row).
    //
    // `format_pref` continues to be snapshotted at enqueue time (UC 01
    // semantics, unchanged by UC 16) — assert this on both rows.
    //
    // Note on the deterministic ordering: with `cap = 1`, adding two URLs
    // back-to-back leaves "before" in_flight (holding the only semaphore
    // permit) and "after" queued. We change settings between the in_flight
    // confirmation and the first `release_one`, so:
    //   - "before"'s supervisor has already run `resolve_and_validate_dest_dir`
    //     against the OLD setting and persisted it on the row;
    //   - "after"'s supervisor has not yet run, and will resolve against the
    //     NEW setting once `release_one` frees the semaphore.
    let env = setup(FakeBehavior::default(), 1);

    // The post-change dest must be a real, writable directory because
    // `resolve_and_validate_dest_dir` performs a writability touch-test
    // before persisting (AC #8). We use a sibling tempdir so it is created
    // by the same TempDir lifetime as the seeded default.
    #[allow(clippy::used_underscore_binding)]
    let alt_dest = env._tmp.path().join("alt-destination");
    std::fs::create_dir_all(&alt_dest).expect("mkdir alt_dest");

    // Default format = BestHeuristic, dest = the seeded tmpdir.
    env.manager
        .add_url("https://example.com/before".to_string(), None)
        .await
        .expect("add before");

    // Wait for the runner to promote "before" to in_flight (and for the
    // supervisor to persist the OLD dest on the row). With cap=1, the
    // next `add_url` will land queued.
    let mut before_in_flight = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/before"))
            .unwrap()
            .expect("before row");
        if matches!(row.status, QueueStatus::InFlight) {
            before_in_flight = true;
            break;
        }
    }
    assert!(before_in_flight, "before row must reach in_flight");

    // Change `format_pref` BEFORE adding "after" so its enqueue snapshot
    // differs from "before" — that's the format-snapshot-at-enqueue half
    // of the test name.
    env.db
        .with_conn(|c| settings::set_format_pref(c, FormatPref::BestAudioMp3))
        .unwrap();

    // Add "after" — snapshots BestAudioMp3 (and the still-original dest)
    // at enqueue time. Lands queued because the cap-1 semaphore is held
    // by "before".
    env.manager
        .add_url("https://example.com/after".to_string(), None)
        .await
        .expect("add after");

    // Now change `dest_dir` while "before" is in_flight and "after" is
    // queued. The dest change must NOT touch "before" (in_flight = locked
    // to its spawn-time resolution) and MUST be picked up by "after"
    // (queued = re-resolved at spawn).
    env.db
        .with_conn(|c| settings::set_dest_dir(c, &alt_dest))
        .unwrap();

    // Release "before" — its supervisor exits with the OLD dest still
    // persisted. The semaphore frees and "after" is promoted, which kicks
    // off the spawn-time resolve against the NEW setting.
    env.bridge.release_one();

    // Wait for "after" to reach in_flight (post-change dest persisted by
    // its supervisor).
    let mut after_in_flight = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/after"))
            .unwrap()
            .expect("after row");
        if matches!(row.status, QueueStatus::InFlight) {
            after_in_flight = true;
            break;
        }
    }
    assert!(after_in_flight, "after row must reach in_flight");

    // Read the rows back via the queue DAO. We assert against the DB-
    // persisted `dest_dir` (not just the captured `DownloadRequest`)
    // per the AC #5 contract that the row reflects the actual landing
    // folder.
    let items = env
        .db
        .with_conn(crate::db::queue::list_all)
        .expect("list_all");
    let before = items
        .iter()
        .find(|i| i.url == "https://example.com/before")
        .expect("before row");
    let after = items
        .iter()
        .find(|i| i.url == "https://example.com/after")
        .expect("after row");

    // Format snapshot survives — locked at enqueue time, not re-read.
    assert_eq!(before.format_pref, FormatPref::BestHeuristic);
    assert_eq!(after.format_pref, FormatPref::BestAudioMp3);

    // AC #5 second clause — in_flight at change → immune.
    #[allow(clippy::used_underscore_binding)]
    let initial_dest = env._tmp.path().to_path_buf();
    assert_eq!(
        before.dest_dir, initial_dest,
        "before was in_flight when settings changed; dest_dir must remain the pre-change value"
    );
    // AC #5 first clause — queued at change → picked up.
    assert_eq!(
        after.dest_dir, alt_dest,
        "after was queued when settings changed; dest_dir must be re-resolved at spawn to the post-change value"
    );

    // The captured DownloadRequest for the second supervisor must also
    // carry the post-change dest (defense-in-depth: row write and request
    // arg are sourced from the same resolution).
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(
            b.captured_requests.len(),
            2,
            "exactly two start_download calls"
        );
        assert_eq!(
            b.captured_requests[0].dest_dir, initial_dest,
            "first capture is the pre-change dest"
        );
        assert_eq!(
            b.captured_requests[1].dest_dir, alt_dest,
            "second capture is the post-change dest"
        );
    }

    // Release "after" so the test exits cleanly.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(80)).await;
    drain_ui(&env.ui_rx).await;
}

// -- UC 19: per-URL audio-only override on AddBar --------------------------

#[tokio::test]
async fn add_url_single_video_with_audio_only_override() {
    // UC 19 AC #2 — when the AddBar's audio-only toggle is on, the per-URL
    // override flows through `add_url(url, Some(BestAudioM4a))` and is
    // persisted on the row's `format_pref` column at enqueue time.
    let env = setup(FakeBehavior::default(), 1);

    env.manager
        .add_url(
            "https://example.com/audio-only-single".to_string(),
            Some(FormatPref::BestAudioM4a),
        )
        .await
        .expect("add_url");

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/audio-only-single"))
        .unwrap()
        .expect("row exists");
    assert_eq!(
        row.format_pref,
        FormatPref::BestAudioM4a,
        "per-URL override must land on the row at enqueue"
    );

    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn add_url_playlist_threads_audio_only_override_to_every_entry() {
    // UC 19 AC #2 across playlist expansion — the override applies to every
    // expanded entry, not just the first one.
    let entries = vec![
        PlaylistEntry {
            url: "https://example.com/pl-a".into(),
            title: Some("a".into()),
            thumbnail: None,
        },
        PlaylistEntry {
            url: "https://example.com/pl-b".into(),
            title: Some("b".into()),
            thumbnail: None,
        },
        PlaylistEntry {
            url: "https://example.com/pl-c".into(),
            title: Some("c".into()),
            thumbnail: None,
        },
    ];
    let env = setup(
        FakeBehavior {
            playlist_entries: entries,
            ..Default::default()
        },
        1,
    );

    env.manager
        .add_url(
            "https://example.com/audio-only-playlist".to_string(),
            Some(FormatPref::BestAudioM4a),
        )
        .await
        .expect("add_url");

    let items = env
        .db
        .with_conn(crate::db::queue::list_all)
        .expect("list_all");
    let entry_urls = [
        "https://example.com/pl-a",
        "https://example.com/pl-b",
        "https://example.com/pl-c",
    ];
    for url in entry_urls {
        let row = items
            .iter()
            .find(|i| i.url == url)
            .unwrap_or_else(|| panic!("playlist row {url} must exist"));
        assert_eq!(
            row.format_pref,
            FormatPref::BestAudioM4a,
            "playlist entry {url} must carry the per-URL override"
        );
    }

    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn add_url_with_none_override_uses_settings_default() {
    // UC 19 backward-compat — when no per-URL override is supplied, the row's
    // `format_pref` must fall back to the Settings default (NOT to a hardcoded
    // `BestVideo`/`BestHeuristic`). This locks in the existing UC 01 behavior
    // that pre-existing call sites (e.g. integration tests, future callers
    // that don't yet plumb the toggle) should keep observing.
    let env = setup(FakeBehavior::default(), 1);

    // Seed a non-default Settings format so we can distinguish "fall back to
    // settings" from "fall back to hardcoded variant default".
    env.db
        .with_conn(|c| settings::set_format_pref(c, FormatPref::BestAudioMp3))
        .unwrap();

    env.manager
        .add_url("https://example.com/none-override".to_string(), None)
        .await
        .expect("add_url");

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/none-override"))
        .unwrap()
        .expect("row exists");
    assert_eq!(
        row.format_pref,
        FormatPref::BestAudioMp3,
        "None override must defer to the Settings format_pref (backward-compat)"
    );

    drain_ui(&env.ui_rx).await;
}

// -- UC 16: download destination resolution --------------------------------

#[tokio::test]
async fn dest_dir_resolves_to_per_os_default_when_unset() {
    // UC 16 AC#1 — fresh install (no `dest_dir` setting) must land in the
    // per-OS default Downloads folder (or app-data fallback). Never in `cwd`.
    //
    // We do not seed `settings.dest_dir`, so `add_url`'s `default_root` is
    // computed via `paths::default_download_dir_or_app_data` and the row's
    // initial `dest_dir` reflects that resolution.
    //
    // We assert against the row immediately after `add_url` returns,
    // BEFORE the supervisor's `resolve_and_validate_dest_dir` runs. The
    // per-OS default folder may or may not exist in the test environment
    // (`~/Downloads/yt-dlp-ui` is created on demand at app boot, not at
    // test boot), so we cannot rely on the row reaching in_flight. The
    // enqueue-time snapshot is sufficient to pin AC#1 — it confirms the
    // resolution path used by `add_url` returns the per-OS default and
    // never `cwd`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");
    let db = Db::open(&db_path).expect("open db");

    // Deliberately DO NOT call `settings::set_dest_dir`, so the read path
    // sees an absent key and falls back to `default_root`.
    // UC 17: stage a dummy ffmpeg file so the manager's ffmpeg gate doesn't
    // short-circuit the row to `error` before the dest path is exercised.
    let dummy_ffmpeg = tmp.path().join("ffmpeg");
    std::fs::write(&dummy_ffmpeg, b"#!/bin/sh\nexit 0\n").expect("stage dummy ffmpeg");
    let bridge = FakeBridge::new(FakeBehavior::default());
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(64);
    let manager = DownloadManager::new(
        db.clone(),
        bridge.clone(),
        ui_tx,
        1,
        Vec::new(),
        None,
        Some(dummy_ffmpeg),
        tmp.path().join("thumbnails"),
    );

    manager
        .add_url("https://example.com/fresh-install".to_string(), None)
        .await
        .expect("add");

    let row = db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/fresh-install"))
        .unwrap()
        .expect("row inserted");

    // Per-OS default ends with the app suffix `yt-dlp-ui`. The fallback
    // path (`<app_data>/downloads`) ends with `downloads`. Either is
    // acceptable — what we are pinning is "NOT cwd".
    let last = row
        .dest_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .unwrap_or_default();
    assert!(
        last == "yt-dlp-ui" || last == "downloads",
        "fresh-install dest must end with 'yt-dlp-ui' (default) or 'downloads' (fallback); got {:?}",
        row.dest_dir
    );

    // Hard guard against AC #1's prohibited fallback: never `.` or the
    // current working directory.
    let cwd = std::env::current_dir().expect("cwd");
    assert_ne!(row.dest_dir, PathBuf::from("."), "must not be cwd literal");
    assert_ne!(
        row.dest_dir, cwd,
        "must not be the current working directory"
    );
    assert!(
        row.dest_dir.is_absolute(),
        "per-OS default must be an absolute path; got {:?}",
        row.dest_dir
    );

    // Drop drain helpers without holding rx open — the supervisor may or
    // may not still be running depending on whether the per-OS default
    // dir exists at test time; either way the manager is owned by this
    // scope and cleaned up on drop.
    drop(manager);
    drop(bridge);
    drop(ui_rx);
}

#[tokio::test]
async fn in_flight_destination_immune_to_setting_change() {
    // UC 16 AC#5 second clause — once a row is in_flight, its `dest_dir`
    // is locked to the value resolved at spawn time and is NOT re-read
    // from settings when the user changes the destination mid-flight.
    //
    // This is a focused test: the renamed
    // `format_snapshot_at_enqueue_dest_resolved_at_spawn` covers the same
    // clause incidentally, but this one has the smallest possible
    // arrange/act/assert so a regression here surfaces in isolation.
    let env = setup(FakeBehavior::default(), 1);

    #[allow(clippy::used_underscore_binding)]
    let initial_dest = env._tmp.path().to_path_buf();
    #[allow(clippy::used_underscore_binding)]
    let alt_dest = env._tmp.path().join("post-change");
    std::fs::create_dir_all(&alt_dest).expect("mkdir alt_dest");

    env.manager
        .add_url("https://example.com/locked".to_string(), None)
        .await
        .expect("add");

    // Wait for in_flight (spawn-time resolve has run).
    let mut in_flight = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/locked"))
            .unwrap()
            .expect("row");
        if matches!(row.status, QueueStatus::InFlight) {
            in_flight = true;
            break;
        }
    }
    assert!(in_flight, "row must reach in_flight before assertion");

    // User changes the destination AFTER the row is in_flight.
    env.db
        .with_conn(|c| settings::set_dest_dir(c, &alt_dest))
        .unwrap();

    // The row's persisted dest_dir must remain the pre-change value.
    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/locked"))
        .unwrap()
        .expect("row");
    assert_eq!(
        row.dest_dir, initial_dest,
        "in_flight row's dest must NOT be rewritten by a subsequent setting change"
    );

    // The captured DownloadRequest for the (only) start_download call must
    // also carry the pre-change dest. Defense-in-depth against a future
    // refactor that decouples the row write from the bridge arg.
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.captured_requests.len(), 1);
        assert_eq!(
            b.captured_requests[0].dest_dir, initial_dest,
            "DownloadRequest.dest_dir at spawn must be the pre-change value"
        );
    }

    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(80)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn missing_destination_marks_row_error_no_spawn() {
    // UC 16 AC#8 — when the destination folder does not exist (or is not
    // writable) at spawn time, the row transitions to `error` with a
    // user-visible message; `start_download` is never called; no
    // auto-mkdir, no silent fallback.
    let env = setup(FakeBehavior::default(), 1);

    // Seed a destination that definitely does not exist on the filesystem.
    // We use a child of the test tempdir so the path is plausible but
    // missing.
    #[allow(clippy::used_underscore_binding)]
    let missing = env._tmp.path().join("does-not-exist");
    assert!(!missing.exists(), "fixture: missing path must not exist");
    env.db
        .with_conn(|c| settings::set_dest_dir(c, &missing))
        .unwrap();

    env.manager
        .add_url("https://example.com/missing-dest".to_string(), None)
        .await
        .expect("add");

    // Wait for the row to reach a terminal state. With a missing dest, the
    // supervisor short-circuits in `resolve_and_validate_dest_dir` and the
    // row should land in `error`. Bound the wait so a hang surfaces as a
    // failed assertion rather than a test timeout.
    let mut final_status: Option<QueueStatus> = None;
    let mut row_error: Option<String> = None;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/missing-dest"))
            .unwrap()
            .expect("row");
        if matches!(
            row.status,
            QueueStatus::Error | QueueStatus::Done | QueueStatus::Cancelled
        ) {
            final_status = Some(row.status);
            row_error = row.error_msg.clone();
            break;
        }
    }
    assert_eq!(
        final_status,
        Some(QueueStatus::Error),
        "missing dest must terminate the row at error"
    );
    let msg = row_error.expect("error_msg must be populated for AC#8 user message");
    assert!(
        !msg.is_empty(),
        "error_msg must not be empty (AC#8 'user-visible message')"
    );

    // No `start_download` should have been called — the supervisor's
    // dest validation runs BEFORE the bridge spawn (per AC#8 'no auto-mkdir').
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(
            b.download_calls, 0,
            "start_download must NOT have been invoked for a missing-dest row"
        );
        assert!(
            b.captured_requests.is_empty(),
            "no DownloadRequest must be captured when dest validation fails"
        );
    }

    // The folder must NOT have been auto-created by the validator.
    assert!(
        !missing.exists(),
        "AC#8 forbids auto-mkdir on a missing destination"
    );

    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn unicode_and_space_paths_round_trip() {
    // UC 16 AC#6 / AC#7 — paths with spaces, Unicode, and emoji must round-
    // trip through settings, the spawn-time resolve, the row write, and
    // the captured DownloadRequest unchanged. We compare via `PathBuf`
    // equality (NOT `String`) because Windows path canonicalization may
    // normalize separators, but the tempdir-derived path stays stable
    // either way.
    let env = setup(FakeBehavior::default(), 1);

    #[allow(clippy::used_underscore_binding)]
    let exotic = env._tmp.path().join("spaces and 漢字 and 🎬");
    std::fs::create_dir_all(&exotic).expect("mkdir exotic");
    env.db
        .with_conn(|c| settings::set_dest_dir(c, &exotic))
        .unwrap();

    env.manager
        .add_url("https://example.com/exotic-path".to_string(), None)
        .await
        .expect("add");

    // Wait for in_flight so the supervisor has resolved + persisted dest.
    let mut row_dest: Option<PathBuf> = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/exotic-path"))
            .unwrap()
            .expect("row");
        if matches!(row.status, QueueStatus::InFlight) {
            row_dest = Some(row.dest_dir);
            break;
        }
    }
    let row_dest = row_dest.expect("row must reach in_flight");
    assert_eq!(
        row_dest, exotic,
        "row's persisted dest_dir must equal the Unicode/space path verbatim"
    );

    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.captured_requests.len(), 1);
        assert_eq!(
            b.captured_requests[0].dest_dir, exotic,
            "captured DownloadRequest.dest_dir must equal the Unicode/space path verbatim"
        );
    }

    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(80)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn requeue_pending_title_fetches_walks_pending_rows() {
    let env = setup(FakeBehavior::default(), 1);

    // Insert a row with title_status = pending directly via the DAO.
    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/pending".into(),
                    title: None,
                    title_status: crate::model::TitleStatus::Pending,
                    format_pref: FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();

    env.manager.requeue_pending_title_fetches().await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;

    let item = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .expect("row exists");
    assert_eq!(item.title.as_deref(), Some("Real Title"));
    assert_eq!(item.title_status, crate::model::TitleStatus::Ok);

    // Drain so the in-flight download (the pending row was queued) doesn't
    // hang the test.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(120)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn fetch_title_error_records_title_error() {
    let env = setup(
        FakeBehavior {
            fetch_error: Some("title fetch boom"),
            ..Default::default()
        },
        1,
    );
    env.manager
        .add_url("https://example.com/no-title".to_string(), None)
        .await
        .expect("add");
    tokio::time::sleep(Duration::from_millis(80)).await;

    let item = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/no-title"))
        .unwrap()
        .expect("row exists");
    assert_eq!(item.title_status, crate::model::TitleStatus::Error);
    assert!(item.title_error.is_some(), "error tail captured");

    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(120)).await;
    drain_ui(&env.ui_rx).await;
}

// -- UC 05: bot-check recovery -----------------------------------------------

#[tokio::test]
async fn add_url_returns_auth_required_when_expand_hits_bot_check() {
    // AC#1, AC#2, AC#3: a bot-check during metadata fetch surfaces as a typed
    // BridgeError::AuthRequired bubbled up via AddError::Bridge.
    let env = setup(
        FakeBehavior {
            expand_outcomes: vec![ExpandOutcome::AuthRequired {
                stderr_tail: "Sign in to confirm you're not a bot".to_string(),
            }],
            ..Default::default()
        },
        1,
    );

    let err = env
        .manager
        .add_url("https://www.youtube.com/watch?v=test".to_string(), None)
        .await
        .expect_err("expand_playlist AuthRequired must surface");

    match err {
        AddError::Bridge(BridgeError::AuthRequired { .. }) => {}
        other => panic!("expected AddError::Bridge(AuthRequired), got {other:?}"),
    }
}

#[tokio::test]
async fn auth_required_during_download_opens_dialog_then_retries_with_cookies() {
    // AC#3, AC#4, AC#7-#10: a download row hits AuthRequired; the manager
    // emits ShowBotCheckDialog; the user's pick triggers a retry with the
    // cookies arg threaded through.
    let env = setup_full(
        FakeBehavior {
            // First start_download: AuthRequired. Second start_download
            // (the retry): default = release semaphore = success.
            download_outcomes: vec![DownloadOutcome::AuthRequired {
                stderr_tail: "Use --cookies-from-browser".to_string(),
            }],
            ..Default::default()
        },
        1,
        vec![Browser::Chrome, Browser::Firefox],
        None,
    );

    env.manager
        .add_url("https://www.youtube.com/watch?v=row".to_string(), None)
        .await
        .expect("add");

    // Wait for the row to be promoted, the first download to fail with
    // AuthRequired, and the dialog to open.
    let mut saw_dialog = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                saw_dialog = true;
            }
        }
        drop(rx);
        if saw_dialog {
            break;
        }
    }
    assert!(saw_dialog, "ShowBotCheckDialog must be emitted");

    // User picks chrome (no remember).
    env.manager
        .bot_check_coordinator()
        .user_picked(Browser::Chrome, false, &env.db)
        .await
        .expect("user_picked");

    // Release the retry so it finishes cleanly.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Two start_download calls (initial + retry).
    let b = env.bridge.behavior.lock().await;
    assert_eq!(b.download_calls, 2, "exactly one retry after AuthRequired");
    assert_eq!(b.captured_requests.len(), 2);
    assert_eq!(
        b.captured_requests[0].cookies_browser, None,
        "first attempt has no cookies"
    );
    assert_eq!(
        b.captured_requests[1].cookies_browser.as_deref(),
        Some("chrome"),
        "retry must carry the user's pick as cookies arg"
    );
}

#[tokio::test]
async fn second_auth_required_in_same_supervisor_does_not_reprompt() {
    // AC#10: a row that retries with cookies and ALSO hits AuthRequired the
    // second time falls through to error — no infinite re-prompt within the
    // same supervisor incarnation.
    let env = setup_full(
        FakeBehavior {
            download_outcomes: vec![
                DownloadOutcome::AuthRequired {
                    stderr_tail: "Use --cookies-from-browser".to_string(),
                },
                DownloadOutcome::AuthRequired {
                    stderr_tail: "Use --cookies-from-browser".to_string(),
                },
            ],
            ..Default::default()
        },
        1,
        vec![Browser::Chrome],
        None,
    );

    env.manager
        .add_url("https://www.youtube.com/watch?v=again".to_string(), None)
        .await
        .expect("add");

    // Wait for the first dialog and user-pick, which triggers the retry.
    let mut dialog_count: u32 = 0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                dialog_count += 1;
            }
        }
        drop(rx);
        if dialog_count >= 1 {
            break;
        }
    }
    assert!(dialog_count >= 1, "first AuthRequired must open the dialog");

    // User picks chrome → retry happens, but the retry ALSO returns AuthRequired.
    env.manager
        .bot_check_coordinator()
        .user_picked(Browser::Chrome, false, &env.db)
        .await
        .expect("user_picked");

    // Wait long enough for the retry to fail. There must NOT be a second dialog.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                dialog_count += 1;
            }
        }
    }
    assert_eq!(
        dialog_count, 1,
        "second AuthRequired must NOT cause a second dialog (got {dialog_count})"
    );

    // The row should be in `error` status.
    let rows = env.manager.list_ui_rows().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, QueueStatus::Error);
}

#[tokio::test]
async fn multi_row_batching_picks_apply_to_all_rows_atomically() {
    // AC#4: 3 rows all hit AuthRequired. First report opens a dialog;
    // subsequent reports register and wait. User's single pick retries all 3
    // with cookies.
    let env = setup_full(
        FakeBehavior {
            // 3 initial AuthRequired (one per row) + 3 default (the retries).
            download_outcomes: vec![
                DownloadOutcome::AuthRequired {
                    stderr_tail: "Use --cookies-from-browser".to_string(),
                },
                DownloadOutcome::AuthRequired {
                    stderr_tail: "Use --cookies-from-browser".to_string(),
                },
                DownloadOutcome::AuthRequired {
                    stderr_tail: "Use --cookies-from-browser".to_string(),
                },
            ],
            ..Default::default()
        },
        3, // concurrency cap = 3 so all rows go in-flight at once
        vec![Browser::Firefox],
        None,
    );

    for i in 0..3 {
        let url = format!("https://www.youtube.com/watch?v=row-{i}");
        env.manager.add_url(url, None).await.expect("add");
    }

    // Wait for the first dialog to surface AND for the other two rows to
    // have hit AuthRequired and queued behind the open dialog.
    let mut dialog_count: u32 = 0;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                dialog_count += 1;
            }
        }
        drop(rx);
        let b = env.bridge.behavior.lock().await;
        if b.download_calls >= 3 {
            break;
        }
    }
    assert_eq!(dialog_count, 1, "exactly ONE dialog for the whole batch");

    // User picks firefox (no remember).
    let drained = env
        .manager
        .bot_check_coordinator()
        .user_picked(Browser::Firefox, false, &env.db)
        .await
        .expect("user_picked");
    assert_eq!(drained.len(), 3, "all 3 rows must be drained by one pick");

    // Release the 3 retries.
    for _ in 0..3 {
        env.bridge.release_one();
    }
    // Poll deterministically for the 3 retry start_download calls. A fixed
    // sleep (e.g. 300 ms) is flaky on slow Windows GHA runners — the retry
    // spawn → start_download dispatch can race past the assertion window.
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let b = env.bridge.behavior.lock().await;
        if b.download_calls >= 6 {
            break;
        }
    }

    let b = env.bridge.behavior.lock().await;
    assert_eq!(
        b.download_calls, 6,
        "3 initial + 3 retries = 6 start_download calls"
    );
    // Last 3 captures must all carry firefox.
    for cap in b.captured_requests.iter().skip(3).take(3) {
        assert_eq!(
            cap.cookies_browser.as_deref(),
            Some("firefox"),
            "every retry must carry firefox cookies, got {cap:?}"
        );
    }
}

#[tokio::test]
async fn js_runtime_path_threads_through_to_download_request() {
    // AC#16: when js_runtime_path is set on the manager, every spawned
    // DownloadRequest must carry it.
    let deno_path = PathBuf::from("/path/to/deno");
    let env = setup_full(
        FakeBehavior::default(),
        1,
        Vec::new(),
        Some(deno_path.clone()),
    );

    env.manager
        .add_url("https://example.com/with-deno".to_string(), None)
        .await
        .expect("add");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let b = env.bridge.behavior.lock().await;
    assert!(!b.captured_requests.is_empty(), "must spawn at least once");
    assert_eq!(
        b.captured_requests[0].js_runtime_path.as_deref(),
        Some(deno_path.as_path()),
        "DownloadRequest must carry the resolved deno path"
    );
    drop(b);
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(120)).await;
    drain_ui(&env.ui_rx).await;
}

// -- UC 17: ffmpeg path threading + missing-ffmpeg gate ----------------------

#[tokio::test]
async fn ffmpeg_path_threads_through_to_download_request() {
    // UC 17 AC#2: when ffmpeg_path is set on the manager, every spawned
    // DownloadRequest must carry it. Mirrors
    // `js_runtime_path_threads_through_to_download_request`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ffmpeg_file = tmp.path().join("ffmpeg");
    std::fs::write(&ffmpeg_file, b"#!/bin/sh\nexit 0\n").expect("stage ffmpeg");

    let env = setup_full_with_ffmpeg(
        FakeBehavior::default(),
        1,
        Vec::new(),
        None,
        Some(ffmpeg_file.clone()),
        tmp,
    );

    env.manager
        .add_url("https://example.com/with-ffmpeg".to_string(), None)
        .await
        .expect("add");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let b = env.bridge.behavior.lock().await;
    assert!(!b.captured_requests.is_empty(), "must spawn at least once");
    assert_eq!(
        b.captured_requests[0].ffmpeg_path.as_deref(),
        Some(ffmpeg_file.as_path()),
        "DownloadRequest must carry the resolved ffmpeg path"
    );
    drop(b);
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(120)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn manager_with_no_ffmpeg_marks_row_error_no_spawn() {
    // UC 17 AC#9: if the manager is constructed with ffmpeg_path = None
    // (broken/incomplete install), every queued row must transition to
    // `error` with a user-visible message AND `start_download` must NEVER
    // be invoked. Release vs dev branches surface different remediation
    // copy; this test asserts the message is non-empty and contains
    // "ffmpeg" so a future copy refactor doesn't silently emit an empty
    // string.
    let tmp = tempfile::tempdir().expect("tempdir");
    let env = setup_full_with_ffmpeg(
        FakeBehavior::default(),
        1,
        Vec::new(),
        None,
        None, // <-- the gate trigger
        tmp,
    );

    env.manager
        .add_url("https://example.com/no-ffmpeg".to_string(), None)
        .await
        .expect("add");

    // Wait for the row to reach a terminal state.
    let mut final_status: Option<QueueStatus> = None;
    let mut row_error: Option<String> = None;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/no-ffmpeg"))
            .unwrap()
            .expect("row");
        if matches!(
            row.status,
            QueueStatus::Error | QueueStatus::Done | QueueStatus::Cancelled
        ) {
            final_status = Some(row.status);
            row_error = row.error_msg.clone();
            break;
        }
    }
    assert_eq!(
        final_status,
        Some(QueueStatus::Error),
        "missing ffmpeg must terminate the row at error"
    );
    let msg = row_error.expect("AC#9 'user-visible message' must be populated");
    assert!(
        !msg.is_empty(),
        "AC#9 error_msg must not be empty (got empty string)"
    );
    assert!(
        msg.to_lowercase().contains("ffmpeg"),
        "AC#9 error_msg must mention ffmpeg so the user knows what's missing (got: {msg:?})"
    );

    // start_download must NEVER have been called — the gate runs BEFORE
    // the bridge spawn. This is the AC#9 "no spawn" half.
    let b = env.bridge.behavior.lock().await;
    assert_eq!(
        b.download_calls, 0,
        "start_download must NOT be invoked when ffmpeg is missing"
    );
    assert!(
        b.captured_requests.is_empty(),
        "no DownloadRequest may be captured when the ffmpeg gate fires"
    );
    drop(b);
    drain_ui(&env.ui_rx).await;
}

// -- UC 02: cancel / remove / restart -----------------------------------

/// Reads a row's status directly from the DB. Avoids racing the UI bridge.
fn row_status(db: &Db, id: i64) -> QueueStatus {
    db.with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .expect("row exists")
        .status
}

/// Polls the row's status until it matches `expected` or the deadline
/// expires. Returns the final observed status. Useful when the cancel /
/// remove flows are asynchronous (the bridge supervisor may take a few ms
/// to confirm the subprocess is dead and flip the row to `cancelled`).
async fn await_status(db: &Db, id: i64, expected: QueueStatus, timeout: Duration) -> QueueStatus {
    let deadline = std::time::Instant::now() + timeout;
    let mut current = row_status(db, id);
    while current != expected && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
        current = row_status(db, id);
    }
    current
}

#[tokio::test]
async fn cancel_one_on_queued_flips_to_cancelled_without_starting_download() {
    // AC#2: when the row is still in `queued` (no supervisor task yet),
    // cancel must flip it directly to `cancelled` and the row's terminal
    // state stays `cancelled` (no error_msg, no supervisor transition).
    //
    // This test pins the *row terminal state* — the race-safe guard at the
    // SQL layer (`try_promote_to_in_flight` refusing a cancelled row) is
    // covered by `db::queue::queue_tests::try_promote_to_in_flight_refuses_cancelled_row`.
    // We do not assert `download_calls == 0` here because the runner's
    // initial wake-after-construction can race the test's insert under
    // heavy parallel load: the runner may promote the row to in_flight
    // before our insert+cancel sequence, in which case `start_download`
    // is called and the supervisor's cancel branch flips the row to
    // `cancelled` (still the correct terminal state, just via a different
    // path). The race-prevention guarantee is encoded at the SQL guard,
    // not measurable from here.
    let env = setup(FakeBehavior::default(), 1);
    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/queued-cancel".to_string(),
                    title: Some("seed".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();

    env.manager.cancel_one(id).await;

    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(2)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.error_msg.is_none(),
        "cancel-while-queued must not taint error_msg (got {:?})",
        row.error_msg
    );

    // Release any in-flight downloads so test exits cleanly even if the
    // runner raced ahead.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_one_on_in_flight_transitions_to_cancelled_via_cancelling() {
    // AC#3, AC#15: in_flight → cancelling → cancelled. The bridge's
    // supervisor task gets the cancel notify, returns `BridgeError::Cancelled`,
    // and the manager's TerminalReason::Cancelled branch flips the row to
    // `cancelled`. No `error_msg` written.
    let env = setup(FakeBehavior::default(), 1);
    env.manager
        .add_url("https://example.com/in-flight".to_string(), None)
        .await
        .expect("add");

    // Wait for the runner to promote and the supervisor to be in the
    // events.recv() loop.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/in-flight"))
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(row_status(&env.db, id), QueueStatus::InFlight);

    env.manager.cancel_one(id).await;

    // Supervisor races the cancel; expect terminal cancelled within 1s.
    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(2)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.error_msg.is_none(),
        "user-cancel must NOT taint error_msg (got {:?})",
        row.error_msg
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_one_during_title_fetch_resets_title_status_to_pending() {
    // AC#4: cancelling a row whose `title_status = fetching` must kill the
    // metadata subprocess immediately and reset title_status to pending so
    // a future Restart re-issues the fetch cleanly.
    let env = setup(
        FakeBehavior {
            // Set a long dwell so cancel_one fires WHILE the cancellable
            // title fetch is parked inside the fake.
            metadata_cancel_dwell: Some(Duration::from_secs(5)),
            ..Default::default()
        },
        0,
    );
    env.manager
        .add_url("https://example.com/cancel-fetch".to_string(), None)
        .await
        .expect("add");

    // Wait for the title-fetch task to set title_status=fetching.
    let id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/cancel-fetch"))
        .unwrap()
        .unwrap()
        .id;
    // 5 s: Windows CI process startup is slow under load.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
            .unwrap()
            .unwrap();
        if matches!(row.title_status, crate::model::TitleStatus::Fetching) {
            break;
        }
        assert!(
            std::time::Instant::now() <= deadline,
            "title_status never reached Fetching (current = {:?})",
            row.title_status
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    env.manager.cancel_one(id).await;

    // Row must be cancelled; title_status reset to pending (NOT error).
    // 8 s: Windows CI process teardown is slow under load; 2 s is too tight.
    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(8)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert_eq!(
        row.title_status,
        crate::model::TitleStatus::Pending,
        "metadata-cancel must reset title_status to Pending so Restart can re-issue cleanly"
    );
    assert!(
        row.title_error.is_none(),
        "metadata-cancel must NOT write a title_error"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_one_on_in_flight_with_title_fetching_fires_both_tokens() {
    // Challenger flag A: a row that is BOTH in_flight (download spawned)
    // AND title_status = fetching (slow title fetch still resolving) must
    // have BOTH cancel tokens fired and land in `cancelled`.
    let env = setup(
        FakeBehavior {
            metadata_cancel_dwell: Some(Duration::from_secs(5)),
            ..Default::default()
        },
        1,
    );
    env.manager
        .add_url("https://example.com/both-tokens".to_string(), None)
        .await
        .expect("add");

    // Wait for the runner to promote (status = InFlight) AND the title
    // fetch to be in-flight (title_status = Fetching).
    let id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/both-tokens"))
        .unwrap()
        .unwrap()
        .id;
    // 5 s: Windows CI process startup is slow under load.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row = env
            .db
            .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
            .unwrap()
            .unwrap();
        let in_flight = matches!(row.status, QueueStatus::InFlight);
        let fetching = matches!(row.title_status, crate::model::TitleStatus::Fetching);
        if in_flight && fetching {
            break;
        }
        assert!(
            std::time::Instant::now() <= deadline,
            "row never reached (InFlight, Fetching); got status={:?}, title_status={:?}",
            row.status,
            row.title_status
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    env.manager.cancel_one(id).await;

    // 8 s: Windows CI process teardown is slow under load; 2 s is too tight.
    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(8)).await;
    assert_eq!(
        final_status,
        QueueStatus::Cancelled,
        "in_flight + fetching row must terminate at cancelled"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_all_transitions_mixed_queue_and_skips_terminal_rows() {
    // AC#6, AC#7: cancel_all must transition queued/in_flight rows to
    // cancelled (or cancelling → cancelled for in_flight) and leave done /
    // error / cancelled rows untouched.
    let env = setup(FakeBehavior::default(), 1);

    // Seed rows in distinct terminal & non-terminal states. We use direct
    // DAO calls rather than going through add_url so we have predictable
    // status fixtures.
    let mut ids = std::collections::HashMap::<&'static str, i64>::new();
    for (label, status) in [
        ("queued", QueueStatus::Queued),
        ("done", QueueStatus::Done),
        ("error", QueueStatus::Error),
        ("cancelled", QueueStatus::Cancelled),
    ] {
        let id = env
            .db
            .with_conn(|c| {
                crate::db::queue::insert(
                    c,
                    crate::model::NewQueueItem {
                        url: format!("https://example.com/{label}"),
                        title: Some("seed".to_string()),
                        title_status: crate::model::TitleStatus::Ok,
                        format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                        dest_dir: PathBuf::from("/tmp"),
                    },
                )
            })
            .unwrap();
        if !matches!(status, QueueStatus::Queued) {
            env.db
                .with_conn(|c| crate::db::queue::update_status(c, id, status))
                .unwrap();
        }
        ids.insert(label, id);
    }

    env.manager.cancel_all().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(
        row_status(&env.db, ids["queued"]),
        QueueStatus::Cancelled,
        "queued → cancelled"
    );
    assert_eq!(
        row_status(&env.db, ids["done"]),
        QueueStatus::Done,
        "done untouched"
    );
    assert_eq!(
        row_status(&env.db, ids["error"]),
        QueueStatus::Error,
        "error untouched"
    );
    assert_eq!(
        row_status(&env.db, ids["cancelled"]),
        QueueStatus::Cancelled,
        "already-cancelled untouched"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_one_on_cancelling_row_is_a_noop() {
    // AC#15 manager-side guard: a Cancel click on an already-cancelling
    // row (transient state set after the first Cancel) must not double-fire.
    let env = setup(FakeBehavior::default(), 1);

    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/cancelling".to_string(),
                    title: Some("seed".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();
    env.db
        .with_conn(|c| crate::db::queue::update_status(c, id, QueueStatus::Cancelling))
        .unwrap();

    env.manager.cancel_one(id).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        row_status(&env.db, id),
        QueueStatus::Cancelling,
        "manager-side guard must leave the row in Cancelling — no DB writes"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_one_during_auth_required_terminates_at_cancelled() {
    // The supervisor's AuthRequired branch waits on a oneshot channel for
    // the user's browser pick. A Cancel click while waiting must surface
    // through the cancel.notified() arm of the tokio::select! and land the
    // row at `cancelled` (NOT error).
    let env = setup_full(
        FakeBehavior {
            download_outcomes: vec![DownloadOutcome::AuthRequired {
                stderr_tail: "Use --cookies-from-browser".to_string(),
            }],
            ..Default::default()
        },
        1,
        vec![Browser::Chrome],
        None,
    );

    env.manager
        .add_url(
            "https://www.youtube.com/watch?v=cancel-during-auth".to_string(),
            None,
        )
        .await
        .expect("add");

    // Wait for the dialog to surface (which means the supervisor is parked
    // on the bot-check oneshot).
    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::find_by_url(c, "https://www.youtube.com/watch?v=cancel-during-auth")
        })
        .unwrap()
        .unwrap()
        .id;

    let mut saw_dialog = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                saw_dialog = true;
            }
        }
        drop(rx);
        if saw_dialog {
            break;
        }
    }
    assert!(saw_dialog, "dialog must open before cancel test");

    // Cancel while the supervisor is parked on the oneshot.
    env.manager.cancel_one(id).await;

    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(2)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.error_msg.is_none(),
        "Cancel during AuthRequired must not taint error_msg"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn remove_one_on_cancelled_row_with_partial_file_deletes_file_and_row() {
    // AC#10, AC#11: Remove on a cancelled row deletes the .part file from
    // disk and the DB row. RowRemoved is emitted to the UI.
    let env = setup(FakeBehavior::default(), 1);

    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/remove-cancelled".to_string(),
                    title: Some("seed".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();
    env.db
        .with_conn(|c| crate::db::queue::update_status(c, id, QueueStatus::Cancelled))
        .unwrap();

    // Stage a real .part file in the test tempdir.
    #[allow(clippy::used_underscore_binding)]
    let part = env._tmp.path().join("clip.mp4.part");
    std::fs::write(&part, b"partial bytes").unwrap();
    env.db
        .with_conn(|c| crate::db::queue::update_partial_path(c, id, &part))
        .unwrap();
    assert!(
        part.exists(),
        "fixture .part must exist on disk before remove"
    );

    env.manager.remove_one(id).await.expect("remove_one");

    assert!(
        !part.exists(),
        ".part file must be deleted from disk on Remove"
    );
    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap();
    assert!(row.is_none(), "DB row must be deleted");

    // RowRemoved must be emitted (drain whole channel and look for it).
    let mut saw_removed = false;
    let mut rx = env.ui_rx.lock().await;
    while let Ok(evt) = rx.try_recv() {
        if matches!(evt, UiEvent::RowRemoved(target) if target == id) {
            saw_removed = true;
        }
    }
    assert!(saw_removed, "RowRemoved event must be emitted");
}

#[tokio::test]
async fn remove_one_on_in_flight_row_cancels_then_deletes() {
    // AC#10: Removing a queued/in_flight row cancels first, then deletes.
    let env = setup(FakeBehavior::default(), 1);
    env.manager
        .add_url("https://example.com/remove-in-flight".to_string(), None)
        .await
        .expect("add");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/remove-in-flight"))
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(row_status(&env.db, id), QueueStatus::InFlight);

    env.manager.remove_one(id).await.expect("remove_one");

    // Row should be gone from the DB.
    let after = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap();
    assert!(after.is_none(), "row deleted after cancel-then-remove");
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn restart_one_on_cancelled_row_clears_progress_and_re_promotes() {
    // AC#13: Restart resets progress fields, flips to queued, and the
    // queue runner promotes the row to in_flight on the next tick.
    let env = setup(FakeBehavior::default(), 1);

    // Seed a cancelled row directly.
    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/restart-row".to_string(),
                    title: Some("Restart Title".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();
    env.db
        .with_conn(|c| -> crate::db::Result<()> {
            crate::db::queue::update_status(c, id, QueueStatus::InFlight)?;
            crate::db::queue::update_progress(
                c,
                id,
                Some(50.0),
                Some(1024),
                Some(30),
                Some(500_000),
                Some(1_000_000),
            )?;
            crate::db::queue::update_status(c, id, QueueStatus::Cancelled)?;
            Ok(())
        })
        .unwrap();

    env.manager.restart_one(id).await.expect("restart_one");

    // Wait for the runner to promote the row.
    let final_status =
        await_status(&env.db, id, QueueStatus::InFlight, Duration::from_secs(1)).await;
    assert_eq!(final_status, QueueStatus::InFlight);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.progress_pct.is_none() || row.progress_pct == Some(0.0),
        "progress_pct cleared on restart (got {:?})",
        row.progress_pct
    );
    assert!(row.error_msg.is_none(), "error_msg cleared on restart");
    assert_eq!(
        row.size_bytes,
        Some(1_000_000),
        "size_bytes preserved across restart for the resume mono line"
    );

    // Release so the test exits cleanly.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn auth_required_followed_by_cancel_lands_at_cancelled_without_error_msg() {
    // Variant: even when the FakeBridge is configured to return AuthRequired
    // (so the supervisor enters the bot-check waiting state), Cancel must
    // still terminate the row at `cancelled` without writing an error_msg.
    // Pinned because UC 02 reroutes the supervisor's terminal state through
    // `TerminalReason` rather than writing directly.
    let env = setup_full(
        FakeBehavior {
            download_outcomes: vec![DownloadOutcome::AuthRequired {
                stderr_tail: "Use --cookies-from-browser".to_string(),
            }],
            ..Default::default()
        },
        1,
        vec![Browser::Firefox],
        None,
    );

    env.manager
        .add_url(
            "https://www.youtube.com/watch?v=auth-then-cancel".to_string(),
            None,
        )
        .await
        .expect("add");
    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::find_by_url(c, "https://www.youtube.com/watch?v=auth-then-cancel")
        })
        .unwrap()
        .unwrap()
        .id;

    // Wait until the supervisor has parked on the bot-check oneshot
    // (signalled by the dialog event). cancel_one only fires the cancel
    // notify; we rely on the supervisor's `cancel.notified()` arm to do the
    // terminal-state assignment.
    let mut saw_dialog = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                saw_dialog = true;
            }
        }
        drop(rx);
        if saw_dialog {
            break;
        }
    }
    assert!(saw_dialog);

    env.manager.cancel_one(id).await;
    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(2)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);
    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.error_msg.is_none(),
        "AuthRequired-then-cancel must NOT taint error_msg"
    );
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn cancel_followed_by_runner_wake_lands_at_cancelled_terminal() {
    // Challenger flag B (manager-level smoke test): when a row is cancelled
    // first and the runner is woken afterwards, the row's terminal state
    // stays `cancelled`. The atomic SQL guard prevents the runner from
    // overwriting `cancelled → in_flight` — its precise mechanics are
    // covered in `db::queue::queue_tests::try_promote_to_in_flight_*`.
    //
    // We do NOT assert `download_calls == 0` here for the same reason as
    // `cancel_one_on_queued_flips_to_cancelled_without_starting_download`:
    // under parallel load the runner's initial wake can race the insert,
    // potentially leading to a start_download call before the cancel takes
    // effect. The terminal-state guarantee survives either way.
    //
    // UC 14 note: the runner-wake source here is a second `add_url` call
    // (which wakes the runner via `wake()`), NOT `start_all` — UC 14
    // broadened `start_all` to revive `cancelled` rows by design, so it
    // would defeat the point of this race-protection test.
    let env = setup(FakeBehavior::default(), 1);

    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: "https://example.com/race".to_string(),
                    title: Some("seed".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();

    env.manager.cancel_one(id).await;
    // Wake the runner via a side-effecting path that is NOT start_all:
    // adding a different URL calls `wake()` internally on completion.
    env.manager
        .add_url("https://example.com/wake-trigger".to_string(), None)
        .await
        .expect("add wake-trigger");

    let final_status =
        await_status(&env.db, id, QueueStatus::Cancelled, Duration::from_secs(2)).await;
    assert_eq!(final_status, QueueStatus::Cancelled);

    let row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap()
        .unwrap();
    assert!(
        row.error_msg.is_none(),
        "race-window cancel must not taint error_msg (got {:?})",
        row.error_msg
    );

    // Release any spawned download so the test exits cleanly.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drain_ui(&env.ui_rx).await;
}

#[tokio::test]
async fn detected_browsers_accessor_returns_seeded_list() {
    let env = setup_full(
        FakeBehavior::default(),
        1,
        vec![Browser::Brave, Browser::Edge],
        None,
    );
    let detected = env.manager.detected_browsers();
    assert_eq!(detected, vec![Browser::Brave, Browser::Edge]);
}

/// UC 14 — mixed-state seed helper for `start_all_*` tests below. Inserts a
/// row, flips its status to `status`, optionally writes `progress` / `err`.
fn seed_row_with_status(
    db: &Db,
    url: &str,
    status: QueueStatus,
    progress: Option<f32>,
    err: Option<&str>,
) -> i64 {
    let id = db
        .with_conn(|c| {
            crate::db::queue::insert(
                c,
                crate::model::NewQueueItem {
                    url: url.to_string(),
                    title: Some("seed".to_string()),
                    title_status: crate::model::TitleStatus::Ok,
                    format_pref: yt_dlp_bridge::FormatPref::BestHeuristic,
                    dest_dir: PathBuf::from("/tmp"),
                },
            )
        })
        .unwrap();
    if status != QueueStatus::Queued {
        db.with_conn(|c| crate::db::queue::update_status(c, id, status))
            .unwrap();
    }
    if let Some(pct) = progress {
        db.with_conn(|c| {
            crate::db::queue::update_progress(c, id, Some(pct), None, None, None, None)
        })
        .unwrap();
    }
    if let Some(msg) = err {
        db.with_conn(|c| crate::db::queue::set_error_msg(c, id, msg))
            .unwrap();
    }
    id
}

/// UC 14 — mixed-state queue: only `cancelled` and `error` rows transition
/// (back to `queued`) and have their progress / error fields cleared by
/// `clear_for_restart`. `in_flight`, `cancelling`, `done`, `paused`, and
/// already-`queued` rows are untouched. AC #1, #3, #4.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn start_all_resumes_cancelled_and_retries_error_rows_only() {
    // Cap = 1. We seed an in-flight placeholder that holds the only
    // semaphore permit, so the wake() at the end of `start_all` cannot
    // promote any of the seeded `queued` rows out from under the assertions
    // before we read DB state. (`try_promote_queued` is non-blocking on the
    // semaphore: with no permit available it returns without touching the
    // queued rows.)
    let env = setup(FakeBehavior::default(), 1);

    // Placeholder that holds the only slot.
    env.manager
        .add_url("https://example.com/holder".to_string(), None)
        .await
        .expect("add holder");
    // Wait for the runner to promote the holder to in_flight.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let holder_id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/holder"))
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(row_status(&env.db, holder_id), QueueStatus::InFlight);

    let queued_id = seed_row_with_status(
        &env.db,
        "https://example.com/queued",
        QueueStatus::Queued,
        None,
        None,
    );
    let cancelled_id = seed_row_with_status(
        &env.db,
        "https://example.com/cancelled",
        QueueStatus::Cancelled,
        Some(40.0),
        None,
    );
    let error_id = seed_row_with_status(
        &env.db,
        "https://example.com/error",
        QueueStatus::Error,
        Some(70.0),
        Some("HTTP Error 403"),
    );
    let cancelling_id = seed_row_with_status(
        &env.db,
        "https://example.com/cancelling",
        QueueStatus::Cancelling,
        Some(15.0),
        None,
    );
    let done_id = seed_row_with_status(
        &env.db,
        "https://example.com/done",
        QueueStatus::Done,
        Some(100.0),
        None,
    );
    let paused_id = seed_row_with_status(
        &env.db,
        "https://example.com/paused",
        QueueStatus::Paused,
        Some(25.0),
        None,
    );

    env.manager.start_all().await.expect("start_all");

    // Holder is still in_flight — Start all must not stomp the active row.
    assert_eq!(row_status(&env.db, holder_id), QueueStatus::InFlight);
    // Already-queued row stays queued (no-op for that branch).
    assert_eq!(row_status(&env.db, queued_id), QueueStatus::Queued);
    // Cancelled + error rows reset to queued.
    assert_eq!(row_status(&env.db, cancelled_id), QueueStatus::Queued);
    assert_eq!(row_status(&env.db, error_id), QueueStatus::Queued);
    // The non-resumable terminal/transient/paused states are untouched.
    assert_eq!(row_status(&env.db, cancelling_id), QueueStatus::Cancelling);
    assert_eq!(row_status(&env.db, done_id), QueueStatus::Done);
    assert_eq!(row_status(&env.db, paused_id), QueueStatus::Paused);

    // `clear_for_restart` semantics on the rows that DID transition:
    // progress_pct cleared, error_msg cleared. Per-row Restart applies the
    // same reset, so AC #3 (per-row semantic equivalence) holds when both
    // funnel through `clear_for_restart`.
    let cancelled_row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, cancelled_id))
        .unwrap()
        .unwrap();
    assert!(
        cancelled_row.progress_pct.is_none(),
        "cancelled row's progress_pct must be cleared (got {:?})",
        cancelled_row.progress_pct
    );
    assert!(
        cancelled_row.error_msg.is_none(),
        "cancelled row's error_msg must be cleared (got {:?})",
        cancelled_row.error_msg
    );
    let error_row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, error_id))
        .unwrap()
        .unwrap();
    assert!(
        error_row.progress_pct.is_none(),
        "error row's progress_pct must be cleared (got {:?})",
        error_row.progress_pct
    );
    assert!(
        error_row.error_msg.is_none(),
        "error row's error_msg must be cleared (got {:?})",
        error_row.error_msg
    );

    // Done / paused / cancelling rows keep whatever they had — Start all
    // never reads or writes them. Spot-check the done row's progress
    // (left at 100% to prove non-touching).
    let done_row = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, done_id))
        .unwrap()
        .unwrap();
    assert_eq!(done_row.progress_pct, Some(100.0));

    // Release the holder so the test exits cleanly.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(150)).await;
    drain_ui(&env.ui_rx).await;
}

/// UC 14 — concurrency-cap test: with cap=2 and 5 resumable rows in mixed
/// `cancelled` / `error` states, calling `start_all` resets all 5 to
/// `queued` but only 2 spawn immediately; as each released slot frees a
/// permit a third / fourth / fifth promotes. AC #2, #4, #6.
#[tokio::test]
async fn start_all_promotes_resumable_rows_under_cap() {
    let env = setup(FakeBehavior::default(), 2);

    // Seed 5 rows: 3 cancelled + 2 error.
    let ids = vec![
        seed_row_with_status(
            &env.db,
            "https://example.com/c1",
            QueueStatus::Cancelled,
            None,
            None,
        ),
        seed_row_with_status(
            &env.db,
            "https://example.com/c2",
            QueueStatus::Cancelled,
            None,
            None,
        ),
        seed_row_with_status(
            &env.db,
            "https://example.com/c3",
            QueueStatus::Cancelled,
            None,
            None,
        ),
        seed_row_with_status(
            &env.db,
            "https://example.com/e1",
            QueueStatus::Error,
            None,
            None,
        ),
        seed_row_with_status(
            &env.db,
            "https://example.com/e2",
            QueueStatus::Error,
            None,
            None,
        ),
    ];

    env.manager.start_all().await.expect("start_all");

    // Give the runner time to promote up to the cap.
    tokio::time::sleep(Duration::from_millis(200)).await;

    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(
            b.peak_active, 2,
            "exactly 2 must spawn immediately under cap=2 (peak_active = {})",
            b.peak_active
        );
        assert_eq!(b.active_now, 2, "active_now must be 2 with no slot freed");
    }

    // Three rows must still be queued; two must be in_flight. We can't
    // tell which two without observing — count by status.
    let mut in_flight_count = 0u32;
    let mut queued_count = 0u32;
    for id in &ids {
        match row_status(&env.db, *id) {
            QueueStatus::InFlight => in_flight_count += 1,
            QueueStatus::Queued => queued_count += 1,
            other => panic!("unexpected status for id={id}: {other:?}"),
        }
    }
    assert_eq!(in_flight_count, 2);
    assert_eq!(queued_count, 3);

    // Release one slot — the 3rd row must promote.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.peak_active, 2, "peak must remain 2 across the run");
        assert_eq!(b.active_now, 2, "still 2 in flight after one promotion");
        assert_eq!(b.download_calls, 3, "third start_download fired");
    }

    // Release a second slot — 4th promotes.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.peak_active, 2);
        assert_eq!(b.active_now, 2);
        assert_eq!(b.download_calls, 4);
    }

    // Release a third — 5th and final row promotes. Three finishers + two
    // still-active = active_now stays at 2; download_calls climbs to 5 as
    // the final row's supervisor enters start_download. AC #6 (semaphore
    // reuse): the same two semaphore permits cycle through five distinct
    // supervisors without ever exceeding the cap.
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.peak_active, 2);
        assert_eq!(b.active_now, 2);
        assert_eq!(b.download_calls, 5, "all 5 rows have spawned");
    }

    // Drain the remaining two in-flight supervisors. The last two finishers
    // do not promote anyone (queue is empty), so active_now goes 2 → 1 → 0
    // and download_calls stays at 5.
    env.bridge.release_one();
    env.bridge.release_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    {
        let b = env.bridge.behavior.lock().await;
        assert_eq!(b.active_now, 0, "all downloads finished");
        assert_eq!(b.peak_active, 2, "peak_active never breached cap=2");
        assert_eq!(b.download_calls, 5, "no extra spawns after queue drains");
    }
    drain_ui(&env.ui_rx).await;
}

// =====================================================================
// UC 12 — `remove_all` manager-level tests.
// =====================================================================

/// UC 12 AC #7 + AC #14 — calling `remove_all` on a queue with rows in
/// every state mix empties the DB and emits `RowRemoved` for every seeded
/// id. The "active" branch is exercised by a single in-flight row promoted
/// via `add_url` (so the supervisor exists and reacts to the cancel-token);
/// the "terminal" branch is exercised by directly-seeded rows in the four
/// non-active states (cancelled / done / error / paused) plus a queued row
/// that never promotes.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn remove_all_empties_mixed_queue() {
    // Cap = 1 with the slot held by the in-flight `add_url` row, so the
    // direct-seeded `queued` row stays queued (the runner cannot promote it
    // without a free permit) and the assertions are deterministic.
    let env = setup(FakeBehavior::default(), 1);

    // 1. Real in-flight row (the only one whose supervisor is alive).
    env.manager
        .add_url("https://example.com/in-flight".to_string(), None)
        .await
        .expect("add in_flight");
    tokio::time::sleep(Duration::from_millis(150)).await;
    let in_flight_id = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url(c, "https://example.com/in-flight"))
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(
        row_status(&env.db, in_flight_id),
        QueueStatus::InFlight,
        "the holder must be in_flight before remove_all"
    );

    // 2. Direct-seeded rows covering every other status.
    let queued_id = seed_row_with_status(
        &env.db,
        "https://example.com/queued",
        QueueStatus::Queued,
        None,
        None,
    );
    let cancelled_id = seed_row_with_status(
        &env.db,
        "https://example.com/cancelled",
        QueueStatus::Cancelled,
        Some(40.0),
        None,
    );
    let done_id = seed_row_with_status(
        &env.db,
        "https://example.com/done",
        QueueStatus::Done,
        Some(100.0),
        None,
    );
    let error_id = seed_row_with_status(
        &env.db,
        "https://example.com/error",
        QueueStatus::Error,
        Some(70.0),
        Some("HTTP Error 403"),
    );
    let paused_id = seed_row_with_status(
        &env.db,
        "https://example.com/paused",
        QueueStatus::Paused,
        Some(25.0),
        None,
    );

    let seeded_ids: std::collections::HashSet<i64> = [
        in_flight_id,
        queued_id,
        cancelled_id,
        done_id,
        error_id,
        paused_id,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        env.db.with_conn(crate::db::queue::list_all).unwrap().len(),
        6,
        "fixture must seed exactly six rows"
    );

    // 3. Bulk remove — must succeed and empty the queue.
    env.manager.remove_all().await.expect("remove_all");

    // 4. AC #7 — every row is deleted from the DB.
    let after = env.db.with_conn(crate::db::queue::list_all).unwrap();
    assert!(
        after.is_empty(),
        "remove_all must clear every row regardless of starting state (got {after:?})"
    );

    // 5. AC #14 — `RowRemoved` emitted for every seeded id.
    let mut removed_ids = std::collections::HashSet::<i64>::new();
    let mut rx = env.ui_rx.lock().await;
    while let Ok(evt) = rx.try_recv() {
        if let UiEvent::RowRemoved(id) = evt {
            removed_ids.insert(id);
        }
    }
    assert_eq!(
        removed_ids, seeded_ids,
        "RowRemoved must be emitted for every seeded id (missing: {:?})",
        seeded_ids
            .difference(&removed_ids)
            .copied()
            .collect::<Vec<_>>()
    );
}

/// UC 12 AC #9 — when `remove_all` runs while a row is parked at the
/// bot-check oneshot (registered with
/// `BotCheckCoordinator::report_auth_required`), the supervisor's
/// `cancel.notified()` arm MUST call `coordinator.withdraw(id)` so no
/// `oneshot::Sender` leaks. Regression net for the v1 → v2 withdraw-
/// ordering fix.
///
/// The supervisor's terminal state for a cancel-during-AuthRequired path
/// is `Cancelled` (not `Error`); we additionally pin that the row's
/// `error_msg` stays `None` after the deletion settles, matching the
/// existing `cancel_one_during_auth_required_terminates_at_cancelled`
/// pattern.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn remove_all_withdraws_pending_bot_check_rows() {
    let env = setup_full(
        FakeBehavior {
            download_outcomes: vec![DownloadOutcome::AuthRequired {
                stderr_tail: "Use --cookies-from-browser".to_string(),
            }],
            ..Default::default()
        },
        1,
        vec![Browser::Chrome],
        None,
    );

    env.manager
        .add_url("https://www.youtube.com/watch?v=remove-all-bot-check".to_string(), None)
        .await
        .expect("add");

    let id = env
        .db
        .with_conn(|c| {
            crate::db::queue::find_by_url(
                c,
                "https://www.youtube.com/watch?v=remove-all-bot-check",
            )
        })
        .unwrap()
        .unwrap()
        .id;

    // Wait until the supervisor is parked on the bot-check oneshot (the
    // dialog event proves coordinator.report_auth_required has registered
    // the row's retry_tx). Mirrors the existing `cancel_one_during_…` test.
    let mut saw_dialog = false;
    let mut last_status_for_id: Option<&'static str> = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut rx = env.ui_rx.lock().await;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, UiEvent::ShowBotCheckDialog { .. }) {
                saw_dialog = true;
            }
            if let UiEvent::RowUpserted(ref row) = evt
                && row.id == id
            {
                last_status_for_id = Some(row.status.as_str());
            }
        }
        drop(rx);
        if saw_dialog {
            break;
        }
    }
    assert!(saw_dialog, "supervisor must park on the bot-check oneshot");

    // Pre-condition: coordinator has the row registered.
    let coord = env.manager.bot_check_coordinator();
    assert_eq!(
        coord.pending_count().await,
        1,
        "coordinator must hold exactly one registered row before remove_all"
    );

    // Fire remove_all. The supervisor's cancel.notified() arm calls
    // coordinator.withdraw(id), sets terminal = Cancelled, and exits;
    // the post-loop block writes status = Cancelled and emits RowUpserted.
    env.manager.remove_all().await.expect("remove_all");

    // AC #9 (the whole point of this test): the registered oneshot is gone.
    assert_eq!(
        coord.pending_count().await,
        0,
        "coordinator.withdraw(id) must run for every parked row before its DB delete"
    );

    // The row was eventually deleted as part of the bulk transaction.
    let after = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, id))
        .unwrap();
    assert!(after.is_none(), "row must be deleted after remove_all");

    // Drain and inspect the channel. The supervisor's cancel.notified() arm
    // sets terminal = Cancelled (NOT Error) before exiting; the post-loop
    // block then writes status = 'cancelled' and emits RowUpserted; only
    // afterwards does remove_all bulk-delete and emit RowRemoved. Pin both
    // invariants:
    //   (a) at least one RowUpserted{status='cancelled'} was emitted for
    //       the row (the supervisor's terminal write — proves AC #9's
    //       cancel-not-error terminal posture);
    //   (b) no RowUpserted{status='error'} was emitted for the row (a
    //       regression that wrote `error` instead would surface here);
    //   (c) RowRemoved was emitted.
    // Together with the `pending_count == 0` assertion above, this is the
    // full regression net for the v1→v2 withdraw-ordering fix.
    let _ = last_status_for_id; // captured during the dialog wait, retained for debug
    let mut saw_cancelled = false;
    let mut saw_error = false;
    let mut saw_removed = false;
    let mut rx = env.ui_rx.lock().await;
    while let Ok(evt) = rx.try_recv() {
        match evt {
            UiEvent::RowUpserted(row) if row.id == id => match row.status.as_str() {
                "cancelled" => saw_cancelled = true,
                "error" => saw_error = true,
                _ => {}
            },
            UiEvent::RowRemoved(target) if target == id => {
                saw_removed = true;
            }
            _ => {}
        }
    }
    drop(rx);
    assert!(saw_removed, "RowRemoved must be emitted for the deleted row");
    assert!(
        saw_cancelled,
        "the supervisor's cancel.notified() arm must write status = 'cancelled' \
         before remove_all deletes the row (AC #9 + the v1→v2 withdraw-ordering fix)"
    );
    assert!(
        !saw_error,
        "a Cancel-during-AuthRequired must NEVER write status = 'error' for the row — \
         that would leak through TerminalReason::Error and tarnish error_msg too"
    );
}

/// UC 12 AC #10 (pragmatic reading) — the bulk-delete transaction prunes
/// `history` rows referencing `done` queue items first, so the FK cascade
/// at `delete_by_id` (covered by
/// `db::queue::queue_tests::delete_by_id_cascades_history_rows`) is honored
/// when the `DELETE FROM queue_items` runs. Without this, `PRAGMA
/// foreign_keys = ON` would refuse the bulk delete for any `done` row that
/// carries a history entry.
///
/// The strict "history is append-only" reading of AC #10 is impossible
/// while the FK is enabled and the transaction has to succeed; the
/// developer documented this trade-off (the spirit-of-AC is preserved —
/// completed-download history for a row that no longer exists in the
/// queue would be orphan rows that point at a missing FK target).
#[tokio::test]
async fn remove_all_cascades_history_for_done_rows() {
    let env = setup(FakeBehavior::default(), 1);

    // Seed a `done` row plus a referencing history entry.
    let done_id = seed_row_with_status(
        &env.db,
        "https://example.com/done-with-history",
        QueueStatus::Done,
        Some(100.0),
        None,
    );
    env.db
        .with_conn(|c| -> crate::db::Result<()> {
            c.execute(
                "INSERT INTO history (queue_item_id, file_path, bytes, completed_at)
                 VALUES (?, '/tmp/done-with-history.mp4', 1024, CURRENT_TIMESTAMP)",
                [done_id],
            )?;
            Ok(())
        })
        .unwrap();

    // Sanity — history fixture exists.
    let history_before: i64 = env
        .db
        .with_conn(|c| -> crate::db::Result<i64> {
            let n = c.query_row(
                "SELECT COUNT(*) FROM history WHERE queue_item_id = ?",
                [done_id],
                |r| r.get::<_, i64>(0),
            )?;
            Ok(n)
        })
        .unwrap();
    assert_eq!(
        history_before, 1,
        "fixture must seed one history row before remove_all"
    );

    env.manager.remove_all().await.expect("remove_all");

    // AC #7 — queue row gone.
    let queue_after = env
        .db
        .with_conn(|c| crate::db::queue::find_by_url_by_id_internal(c, done_id))
        .unwrap();
    assert!(
        queue_after.is_none(),
        "the done row must be deleted by remove_all"
    );

    // AC #10 (pragmatic) — history row pruned for the done row, matching
    // `queue::delete_by_id` cascade semantics.
    let history_after: i64 = env
        .db
        .with_conn(|c| -> crate::db::Result<i64> {
            let n = c.query_row(
                "SELECT COUNT(*) FROM history WHERE queue_item_id = ?",
                [done_id],
                |r| r.get::<_, i64>(0),
            )?;
            Ok(n)
        })
        .unwrap();
    assert_eq!(
        history_after, 0,
        "history rows for deleted done queue items must be pruned (FK cascade compatibility)"
    );

    drain_ui(&env.ui_rx).await;
}
