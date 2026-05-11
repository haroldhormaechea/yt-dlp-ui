//! UC 08 AC#20, AC#21, AC#22 — End-to-end thumbnail pipeline.
//!
//! Drives `DownloadManager::add_url` via a custom `BridgeOps` impl that
//! returns a thumbnail URL pointing at a `mockito` server. Asserts:
//! - `UiEvent::ThumbnailReady` is emitted with the cached path,
//! - the file actually lands on disk under `<cache_dir>/<sha1>.<ext>`,
//! - `queue::set_thumbnail_path` persists the path to the DB row.
//!
//! Heavy test (mockito + tokio + filesystem) but the only way to pin the
//! whole pipeline at the public-API surface.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::{Notify, mpsc};
use yt_dlp_bridge::{
    BridgeError, DownloadEvent, DownloadRequest, EnumerationOutcome, FormatPref, PlaylistEntry,
    VideoMetadata,
};

use app::db::{Db, queue, settings};
use app::download_mgr::{BridgeOps, DownloadManager, UiEvent};
use app::model::{NewQueueItem, PlaceholderKind, TitleStatus};

/// `BridgeOps` impl that:
/// - returns an empty playlist (single-video path) on `expand_playlist`,
/// - returns a fixed title,
/// - never starts a download (the test cancels the spawned supervisor),
/// - returns the test-supplied thumbnail URL on `fetch_thumbnail_url`.
#[derive(Clone)]
struct ThumbBridge {
    thumb_url: String,
}

impl BridgeOps for ThumbBridge {
    async fn fetch_title(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<String> {
        Ok("title".to_string())
    }

    async fn expand_playlist(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<Vec<PlaylistEntry>> {
        // Empty Vec → manager takes the single-video path, which calls
        // fetch_thumbnail_url and chains into the HTTP fetch.
        Ok(Vec::new())
    }

    fn start_download(
        &self,
        _req: DownloadRequest,
        _cancel: Arc<Notify>,
    ) -> (
        mpsc::Receiver<DownloadEvent>,
        tokio::task::JoinHandle<yt_dlp_bridge::Result<()>>,
    ) {
        // Hold the supervisor open forever — the test never releases it,
        // so the download stays pending while we observe the thumbnail
        // pipeline. Cancellation happens automatically when the runtime
        // drops at end-of-test.
        let (tx, rx) = mpsc::channel::<DownloadEvent>(1);
        let _ = tx; // never sent
        let handle = tokio::spawn(async {
            // Park forever — the runtime drop unwinds it.
            std::future::pending::<()>().await;
            #[allow(unreachable_code)]
            Err(BridgeError::ExitedWithError {
                code: None,
                stderr_tail: "test holder".to_string(),
            })
        });
        (rx, handle)
    }

    async fn fetch_thumbnail_url(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<String> {
        Ok(self.thumb_url.clone())
    }

    async fn fetch_title_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<String> {
        // UC 02: this test exercises the thumbnail pipeline only; the
        // title-fetch cancellable path is covered in `download_mgr_test.rs`.
        // Mirror `fetch_title`'s shape so the row's title resolves cleanly.
        Ok("title".to_string())
    }

    async fn enumerate_playlist_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<EnumerationOutcome> {
        // UC 27: this test exercises the single-video thumbnail pipeline,
        // so report SingleVideo. The placeholder promotes to video and the
        // metadata fetch fills the title in.
        Ok(EnumerationOutcome::SingleVideo)
    }

    async fn fetch_metadata_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<VideoMetadata> {
        // UC 27 consolidated thumb-URL discovery into `fetch_metadata`;
        // the per-row `fetch_thumbnail_url` is now startup-replay only,
        // so the live add-URL path receives the thumbnail URL through
        // this metadata fetch.
        Ok(VideoMetadata {
            title: Some("title".to_string()),
            thumbnail: Some(self.thumb_url.clone()),
            duration_s: None,
        })
    }
}

fn setup_db(tmp: &Path) -> Db {
    let db_path = tmp.join("db.sqlite");
    let db = Db::open(&db_path).expect("open db");
    db.with_conn(|c| settings::set_dest_dir(c, tmp)).unwrap();
    db
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_url_fetches_and_caches_thumbnail() {
    let tmp = tempfile::tempdir().unwrap();
    let db = setup_db(tmp.path());

    // mockito server hosting a tiny "JPEG" payload (any bytes — the bridge
    // does not validate image format, it just writes the response body).
    let mut server = mockito::Server::new_async().await;
    let payload = b"\xFF\xD8\xFF\xE0FAKE-JPEG-PAYLOAD\xFF\xD9";
    let mock = server
        .mock("GET", "/thumb.jpg")
        .with_status(200)
        .with_header("content-type", "image/jpeg")
        .with_body(payload)
        .create_async()
        .await;
    let thumb_url = format!("{}/thumb.jpg", server.url());

    let cache_dir = tmp.path().join("thumbnails");
    let bridge = ThumbBridge {
        thumb_url: thumb_url.clone(),
    };
    let (ui_tx, mut ui_rx) = mpsc::channel::<UiEvent>(64);
    let manager = DownloadManager::new(
        db.clone(),
        bridge,
        ui_tx,
        1,
        Vec::new(),
        None,
        None,
        cache_dir.clone(),
    );

    let row_url = "https://example.com/test-video";
    manager
        .add_url(row_url.to_string(), None)
        .await
        .expect("add_url");

    // Wait up to ~10 s for the ThumbnailReady event.
    let mut got_ready: Option<PathBuf> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Ok(Some(UiEvent::ThumbnailReady { path, .. })) =
            tokio::time::timeout(Duration::from_millis(100), ui_rx.recv()).await
        {
            got_ready = Some(path);
            break;
        }
    }
    let ready_path = got_ready.expect("ThumbnailReady must be emitted within 10s");

    // File on disk.
    assert!(
        ready_path.exists(),
        "cached thumbnail file must exist on disk: {ready_path:?}"
    );
    assert!(
        ready_path.starts_with(&cache_dir),
        "cached file lives under <cache_dir>/: {ready_path:?} vs {cache_dir:?}"
    );
    let bytes = std::fs::read(&ready_path).unwrap();
    assert_eq!(bytes, payload, "cached file is the upstream response body");

    // Filename shape: 40-char SHA-1 hex + `.jpg` (Content-Type: image/jpeg).
    let name = ready_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(
        std::path::Path::new(&name)
            .extension()
            .and_then(|e| e.to_str()),
        Some("jpg"),
        "extension inferred from Content-Type: {name}"
    );
    assert_eq!(
        name.len(),
        40 + 1 + 3,
        "SHA-1 hex (40) + '.' + 'jpg' (3) = 44 chars: {name}"
    );

    // DB row carries thumbnail_path.
    let row = db
        .with_conn(|c| queue::find_by_url(c, row_url))
        .unwrap()
        .expect("row inserted");
    assert_eq!(
        row.thumbnail_path.as_deref(),
        Some(ready_path.as_path()),
        "DB row stores the cached path"
    );

    mock.assert_async().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_failure_is_non_fatal_row_keeps_gradient() {
    // mockito 500 → fetcher logs WARN; no ThumbnailReady event; row's
    // thumbnail_path remains NULL.
    let tmp = tempfile::tempdir().unwrap();
    let db = setup_db(tmp.path());

    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/broken.jpg")
        .with_status(500)
        .with_body("boom")
        .create_async()
        .await;
    let thumb_url = format!("{}/broken.jpg", server.url());

    let cache_dir = tmp.path().join("thumbnails");
    let bridge = ThumbBridge {
        thumb_url: thumb_url.clone(),
    };
    let (ui_tx, mut ui_rx) = mpsc::channel::<UiEvent>(64);
    let manager = DownloadManager::new(
        db.clone(),
        bridge,
        ui_tx,
        1,
        Vec::new(),
        None,
        None,
        cache_dir.clone(),
    );

    let row_url = "https://example.com/broken";
    manager
        .add_url(row_url.to_string(), None)
        .await
        .expect("add_url");

    // Drain UI events for a window — there must NOT be a ThumbnailReady.
    let deadline = std::time::Instant::now() + Duration::from_millis(800);
    while std::time::Instant::now() < deadline {
        if let Ok(Some(evt)) = tokio::time::timeout(Duration::from_millis(100), ui_rx.recv()).await
        {
            assert!(
                !matches!(evt, UiEvent::ThumbnailReady { .. }),
                "fetcher must not emit ThumbnailReady on 500 response"
            );
        }
    }

    let row = db
        .with_conn(|c| queue::find_by_url(c, row_url))
        .unwrap()
        .expect("row inserted");
    assert!(
        row.thumbnail_path.is_none(),
        "DB row must NOT carry a thumbnail_path on fetch failure (got: {:?})",
        row.thumbnail_path
    );
}

/// `BridgeOps` impl that tracks concurrent in-flight `fetch_thumbnail_url`
/// calls so the test can pin the semaphore-imposed cap. Returns `Err` so
/// the manager skips the HTTP fetch path (we are only exercising the
/// subprocess-spawn cap).
#[derive(Clone)]
struct ConcurrencyTrackingBridge {
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    fetch_dwell: Duration,
}

impl BridgeOps for ConcurrencyTrackingBridge {
    async fn fetch_title(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<String> {
        Ok("title".to_string())
    }

    async fn expand_playlist(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<Vec<PlaylistEntry>> {
        Ok(Vec::new())
    }

    fn start_download(
        &self,
        _req: DownloadRequest,
        _cancel: Arc<Notify>,
    ) -> (
        mpsc::Receiver<DownloadEvent>,
        tokio::task::JoinHandle<yt_dlp_bridge::Result<()>>,
    ) {
        let (tx, rx) = mpsc::channel::<DownloadEvent>(1);
        let _ = tx;
        let handle = tokio::spawn(async {
            std::future::pending::<()>().await;
            #[allow(unreachable_code)]
            Err(BridgeError::ExitedWithError {
                code: None,
                stderr_tail: "test holder".to_string(),
            })
        });
        (rx, handle)
    }

    async fn fetch_thumbnail_url(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
    ) -> yt_dlp_bridge::Result<String> {
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(cur, Ordering::SeqCst);
        tokio::time::sleep(self.fetch_dwell).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Err(BridgeError::ExitedWithError {
            code: None,
            stderr_tail: "no thumb url for test".to_string(),
        })
    }

    async fn fetch_title_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<String> {
        // UC 02: title fetches are not what this test measures (it pins
        // the thumbnail-resolve concurrency cap). Resolve cleanly so the
        // row's title isn't a moving part in the test.
        Ok("title".to_string())
    }

    async fn enumerate_playlist_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<EnumerationOutcome> {
        Ok(EnumerationOutcome::SingleVideo)
    }

    async fn fetch_metadata_cancellable(
        &self,
        _url: &str,
        _cookies_browser: Option<&str>,
        _js_runtime_path: Option<&Path>,
        _ffmpeg_path: Option<&Path>,
        _cancel: Arc<Notify>,
    ) -> yt_dlp_bridge::Result<VideoMetadata> {
        Ok(VideoMetadata {
            title: Some("title".to_string()),
            thumbnail: None,
            duration_s: None,
        })
    }
}

/// Regression for the macOS fd-exhaustion crash observed when launching the
/// app with hundreds of NULL-thumbnail rows in the queue:
/// `requeue_pending_thumbnail_fetches` previously fanned out N tokio tasks,
/// each spawning a yt-dlp subprocess concurrently, blowing past the default
/// 256-fd ulimit. With the per-manager `thumbnail_resolve_semaphore`, peak
/// concurrent fetches must stay at or below the cap.
///
/// Cap is `THUMBNAIL_RESOLVE_CONCURRENCY = 4` in `download_mgr.rs`. If that
/// const is bumped, update `EXPECTED_CAP` here in lockstep.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requeue_pending_thumbnail_fetches_is_concurrency_bounded() {
    const EXPECTED_CAP: usize = 4;
    const ROW_COUNT: usize = 20;

    let tmp = tempfile::tempdir().unwrap();
    let db = setup_db(tmp.path());
    let dest_dir = tmp.path().to_path_buf();

    // Seed ROW_COUNT rows with NULL thumbnail_path so requeue picks them up.
    for i in 0..ROW_COUNT {
        let item = NewQueueItem {
            url: format!("https://example.com/video-{i}"),
            title: Some(format!("title-{i}")),
            title_status: TitleStatus::Ok,
            format_pref: FormatPref::BestHeuristic,
            dest_dir: dest_dir.clone(),
            kind: PlaceholderKind::Video,
            display_order: 0,
        };
        db.with_conn(|c| queue::insert(c, item)).unwrap();
    }

    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let bridge = ConcurrencyTrackingBridge {
        in_flight: in_flight.clone(),
        peak: peak.clone(),
        // Long enough that multiple fetches overlap if unbounded; short
        // enough to keep total test runtime well under a second.
        fetch_dwell: Duration::from_millis(120),
    };
    let cache_dir = tmp.path().join("thumbnails");
    let (ui_tx, _ui_rx) = mpsc::channel::<UiEvent>(64);
    let manager = DownloadManager::new(
        db.clone(),
        bridge,
        ui_tx,
        1,
        Vec::new(),
        None,
        None,
        cache_dir,
    );

    // Sanity: every seeded row is visible to the requeue query.
    let pending_count = db
        .with_conn(queue::list_pending_thumbnail_fetches)
        .unwrap()
        .len();
    assert_eq!(
        pending_count, ROW_COUNT,
        "all seeded rows must show up as NULL-thumbnail"
    );

    manager
        .requeue_pending_thumbnail_fetches()
        .await
        .expect("requeue must succeed");

    // Wait for the first spawned task to actually enter the bridge call.
    // Without this, the drain loop below sees in_flight=0 (tasks haven't
    // started yet) and exits immediately, masking the real concurrency.
    let start_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while peak.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < start_deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        peak.load(Ordering::SeqCst) > 0,
        "no spawned task entered fetch_thumbnail_url within {:?} — tasks may not be scheduling at all",
        start_deadline.duration_since(std::time::Instant::now())
    );

    // Now wait for all spawned fetches to drain. With cap=4 and dwell=120ms,
    // 20 rows take roughly 5 batches × 120ms ≈ 600ms.
    let drain_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while in_flight.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < drain_deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        in_flight.load(Ordering::SeqCst),
        0,
        "all fetches must have drained within the deadline"
    );

    let observed_peak = peak.load(Ordering::SeqCst);
    assert!(
        observed_peak <= EXPECTED_CAP,
        "peak concurrent thumbnail fetches {observed_peak} exceeded cap {EXPECTED_CAP}; \
         the semaphore in download_mgr.rs::spawn_thumbnail_fetch_for_single_video is missing or wrong"
    );
    // Sanity check: with 20 rows and a 120ms dwell, the test must observe
    // at least *some* overlap, otherwise it's not actually verifying the
    // bound.
    assert!(
        observed_peak > 1,
        "test must observe overlap > 1 to be meaningful; got {observed_peak}"
    );
}
