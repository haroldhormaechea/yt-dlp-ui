//! Download orchestration: queue-walking, concurrency cap, per-row task lifecycle.
//!
//! The manager is the bridge between the durable `SQLite` queue (`crate::db`)
//! and the per-download `yt-dlp` subprocess started by `yt_dlp_bridge`. It
//! owns:
//! - a `tokio::sync::Semaphore` of size `concurrency_cap`,
//! - an `mpsc::Sender<UiEvent>` for surfacing changes to the UI bridge,
//! - a `HashMap<i64, Arc<Notify>>` of cancel tokens for the future cancel UC,
//! - an internal "wake" channel that triggers the queue-runner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

#[cfg(test)]
#[path = "download_mgr_test.rs"]
mod download_mgr_tests;
use tokio::sync::{Mutex, Notify, Semaphore, mpsc, oneshot};
use yt_dlp_bridge::{
    BridgeError, DownloadEvent, DownloadRequest, FormatPref, PlaylistEntry, get_thumbnail_url,
    get_title, get_title_cancellable,
};

use crate::bot_check::{BotCheckCoordinator, CoordinatorOutcome, RetryDecision};
use crate::browsers::Browser;
use crate::db::{Db, DbError, queue, settings};
use crate::model::{NewQueueItem, QueueItem, QueueStatus, TitleStatus, UiQueueRow};
use crate::paths;

/// How long we let `yt-dlp --print %(title)s` run before timing out. Generous
/// to allow extractor warm-up; not so generous that a hung site stalls the UI.
const TITLE_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum concurrent yt-dlp subprocesses dedicated to resolving thumbnail
/// URLs (UC 08 startup re-issue + per-row single-video adds). Separate from
/// the download semaphore so thumbnail resolution does not contend with
/// active downloads. Sized well below the macOS default 256-fd ulimit so a
/// queue of N rows with NULL thumbnails cannot exhaust process file
/// descriptors at startup (regression: fd-exhaustion crash observed when
/// every row spawned a subprocess concurrently).
const THUMBNAIL_RESOLVE_CONCURRENCY: usize = 4;

/// Outcome of a successful `add_url`.
#[derive(Debug, Clone)]
pub enum AddOutcome {
    /// Number of new rows inserted (1 for a single video, N for a playlist).
    Inserted { count: usize },
}

/// Errors raised by `add_url`.
#[derive(Debug, Error)]
pub enum AddError {
    /// The URL is already present in the queue.
    #[error("already in queue: {0}")]
    DuplicateUrl(String),

    /// The bridge failed to fetch metadata (title or playlist expansion).
    #[error(transparent)]
    Bridge(#[from] BridgeError),

    /// A DB write failed.
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Trait over the bridge functions used by the manager. Real impl delegates
/// to `yt_dlp_bridge`. A `#[cfg(test)]` fake impl is QA's responsibility.
pub trait BridgeOps: Send + Sync + 'static {
    /// Fetch the title for a single video URL.
    fn fetch_title(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send;
    /// Expand a playlist URL into entries; empty Vec means single-video fallback.
    fn expand_playlist(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<Vec<PlaylistEntry>>> + Send;
    /// Spawn a download. Returns the event receiver and the supervisor handle.
    fn start_download(
        &self,
        req: DownloadRequest,
        cancel: Arc<Notify>,
    ) -> (
        mpsc::Receiver<DownloadEvent>,
        tokio::task::JoinHandle<yt_dlp_bridge::Result<()>>,
    );
    /// Resolve the upstream thumbnail URL for a single video URL (UC 08).
    /// Mirrors `fetch_title`'s shape; called from `add_url`'s single-video
    /// branch (and the startup re-issue path) before spawning the per-row
    /// download task.
    fn fetch_thumbnail_url(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send;
    /// Cancellable title fetch (UC 02). Same wire format as `fetch_title`;
    /// the extra `cancel` notify lets `cancel_one` tear the subprocess
    /// down while it is still resolving.
    fn fetch_title_cancellable(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
        cancel: Arc<Notify>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send;
}

/// Real bridge wrapper holding the path to the `yt-dlp` binary.
#[derive(Clone)]
pub struct RealBridge {
    yt_dlp_path: PathBuf,
}

impl RealBridge {
    /// Constructs a wrapper that uses `yt_dlp_path` for every spawn.
    #[must_use]
    pub fn new(yt_dlp_path: PathBuf) -> Self {
        Self { yt_dlp_path }
    }
}

impl BridgeOps for RealBridge {
    fn fetch_title(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send {
        let path = self.yt_dlp_path.clone();
        let url = url.to_string();
        let cookies = cookies_browser.map(str::to_string);
        let js_runtime = js_runtime_path.map(Path::to_path_buf);
        let ffmpeg = ffmpeg_path.map(Path::to_path_buf);
        async move {
            get_title(
                &path,
                &url,
                TITLE_TIMEOUT,
                cookies.as_deref(),
                js_runtime.as_deref(),
                ffmpeg.as_deref(),
            )
            .await
        }
    }
    fn expand_playlist(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<Vec<PlaylistEntry>>> + Send {
        let path = self.yt_dlp_path.clone();
        let url = url.to_string();
        let cookies = cookies_browser.map(str::to_string);
        let js_runtime = js_runtime_path.map(Path::to_path_buf);
        let ffmpeg = ffmpeg_path.map(Path::to_path_buf);
        async move {
            yt_dlp_bridge::expand_playlist(
                &path,
                &url,
                cookies.as_deref(),
                js_runtime.as_deref(),
                ffmpeg.as_deref(),
            )
            .await
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
        yt_dlp_bridge::start(&self.yt_dlp_path, req, cancel)
    }
    fn fetch_thumbnail_url(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send {
        let path = self.yt_dlp_path.clone();
        let url = url.to_string();
        let cookies = cookies_browser.map(str::to_string);
        let js_runtime = js_runtime_path.map(Path::to_path_buf);
        let ffmpeg = ffmpeg_path.map(Path::to_path_buf);
        async move {
            get_thumbnail_url(
                &path,
                &url,
                TITLE_TIMEOUT,
                cookies.as_deref(),
                js_runtime.as_deref(),
                ffmpeg.as_deref(),
            )
            .await
        }
    }
    fn fetch_title_cancellable(
        &self,
        url: &str,
        cookies_browser: Option<&str>,
        js_runtime_path: Option<&Path>,
        ffmpeg_path: Option<&Path>,
        cancel: Arc<Notify>,
    ) -> impl std::future::Future<Output = yt_dlp_bridge::Result<String>> + Send {
        let path = self.yt_dlp_path.clone();
        let url = url.to_string();
        let cookies = cookies_browser.map(str::to_string);
        let js_runtime = js_runtime_path.map(Path::to_path_buf);
        let ffmpeg = ffmpeg_path.map(Path::to_path_buf);
        async move {
            get_title_cancellable(
                &path,
                &url,
                TITLE_TIMEOUT,
                cookies.as_deref(),
                js_runtime.as_deref(),
                ffmpeg.as_deref(),
                cancel,
            )
            .await
        }
    }
}

/// Internal supervisor terminal-state classification (UC 02). The supervisor
/// in `spawn_download_for` records one of these on the way out of its loop;
/// the single post-loop match performs the actual DB write so user-cancel
/// flows do not taint the `error_msg` column.
enum TerminalReason {
    Cancelled,
    Error(String),
}

/// Events flowing from the manager to the UI bridge. Everything the UI ever
/// needs to redraw is one of these variants.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A new row was inserted (or a fetched title arrived for an existing row).
    /// The UI should refresh its model from `row`.
    RowUpserted(UiQueueRow),
    /// A row was removed (e.g. a duplicate was rejected; not used in UC 01,
    /// reserved for UC 02).
    RowRemoved(i64),
    /// A flash message — fed to the UI as a toast / status line.
    Flash { message: String, kind: FlashKind },
    /// Settings were changed; the UI re-reads via the relevant getter.
    SettingsChanged,
    /// Show the `YouTube` bot-check pop-up; `available` is the list of detected
    /// browsers to populate the dropdown with.
    ShowBotCheckDialog { available: Vec<Browser> },
    /// Update the modal's affected-row count (UC 10) — emitted after every
    /// `report_auth_required` (whether `OpenDialog` or `Append`) so the
    /// header copy "This applies to <N> queued items." pluralizes live as
    /// rows pile up while the user is still deciding. Also emitted after
    /// a row withdraws (cancel-during-bot-check) so the count decreases.
    BotCheckAffectedCount { count: u32 },
    /// Update the row's transient `waiting-on-user` flag without touching its
    /// persisted `status` (the row stays `in_flight` while waiting).
    RowWaitingOnUser { id: i64, waiting: bool },
    /// A per-row thumbnail fetch finished; the cached file is at `path`.
    /// The UI bridge sets the row's `thumbnail-path` and `thumbnail-loaded`
    /// fields so the gradient placeholder crossfades to the real image.
    ThumbnailReady { id: i64, path: PathBuf },
}

/// Severity of a UI flash message.
#[derive(Debug, Clone, Copy)]
pub enum FlashKind {
    Info,
    Duplicate,
    Error,
}

/// Concrete download manager.
///
/// Cloning is cheap — every owning field is wrapped in an `Arc`.
#[derive(Clone)]
pub struct DownloadManager<B: BridgeOps + Clone> {
    db: Db,
    bridge: B,
    ui_tx: mpsc::Sender<UiEvent>,
    semaphore: Arc<Semaphore>,
    cancel_tokens: Arc<Mutex<HashMap<i64, Arc<Notify>>>>,
    /// UC 02: parallel cancel-token map for in-flight title-fetch
    /// subprocesses. `cancel_one` and `cancel_all` fire whichever
    /// token(s) are present — a single row can have both alive at
    /// once when its title fetch is slow and the queue runner has
    /// already promoted it to `in_flight`.
    metadata_cancel_tokens: Arc<Mutex<HashMap<i64, Arc<Notify>>>>,
    wake_tx: mpsc::Sender<()>,
    bot_check: BotCheckCoordinator,
    detected_browsers: Arc<Vec<Browser>>,
    js_runtime_path: Arc<Option<PathBuf>>,
    /// UC 17: bundled ffmpeg path. `None` ⇒ ffmpeg unavailable; the
    /// spawn-time gate in `spawn_download_for` flips the row to `error`
    /// instead of attempting a download yt-dlp would fail on (DASH-merge
    /// formats need a working ffmpeg). The path is also forwarded into
    /// every yt-dlp invocation as `--ffmpeg-location <parent_dir>` so
    /// audio-only and progressive-format downloads pick up the bundled
    /// binary too.
    ffmpeg_path: Arc<Option<PathBuf>>,
    /// Per-row thumbnail cache directory (UC 08). Lives at
    /// `<app-data>/thumbnails/`. The manager creates the dir lazily on
    /// first fetch.
    thumbnail_cache_dir: Arc<PathBuf>,
    /// Bounds concurrent yt-dlp subprocesses spawned for thumbnail-URL
    /// resolution. Without this, `requeue_pending_thumbnail_fetches`
    /// fans out N tokio tasks at startup and exhausts the process fd
    /// limit when N is large.
    thumbnail_resolve_semaphore: Arc<Semaphore>,
}

impl<B: BridgeOps + Clone> DownloadManager<B> {
    /// Builds a manager and spawns the queue-runner task on the current
    /// tokio runtime. The runner stays alive for the duration of the
    /// process; it has no shutdown signal in UC 01 (the OS reaps it on app
    /// exit).
    ///
    /// `detected_browsers` is the fixed-at-startup list used to populate the
    /// bot-check dialog dropdown; `js_runtime_path` is the resolved deno
    /// path (if any) forwarded into every yt-dlp invocation;
    /// `thumbnail_cache_dir` is the on-disk thumbnail cache (UC 08).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        bridge: B,
        ui_tx: mpsc::Sender<UiEvent>,
        concurrency_cap: u32,
        detected_browsers: Vec<Browser>,
        js_runtime_path: Option<PathBuf>,
        ffmpeg_path: Option<PathBuf>,
        thumbnail_cache_dir: PathBuf,
    ) -> Self {
        let cap = concurrency_cap.clamp(1, 10) as usize;
        let semaphore = Arc::new(Semaphore::new(cap));
        let thumbnail_resolve_semaphore = Arc::new(Semaphore::new(THUMBNAIL_RESOLVE_CONCURRENCY));
        let (wake_tx, wake_rx) = mpsc::channel::<()>(16);
        let mgr = Self {
            db,
            bridge,
            ui_tx,
            semaphore,
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            metadata_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            wake_tx,
            bot_check: BotCheckCoordinator::new(),
            detected_browsers: Arc::new(detected_browsers),
            js_runtime_path: Arc::new(js_runtime_path),
            ffmpeg_path: Arc::new(ffmpeg_path),
            thumbnail_cache_dir: Arc::new(thumbnail_cache_dir),
            thumbnail_resolve_semaphore,
        };
        let runner = mgr.clone();
        tokio::spawn(async move {
            runner.run_loop(wake_rx).await;
        });
        mgr
    }

    /// Returns the coordinator handle so the UI bridge can wire pop-up
    /// callbacks into [`BotCheckCoordinator::user_picked`] /
    /// [`BotCheckCoordinator::user_cancelled`].
    #[must_use]
    pub fn bot_check_coordinator(&self) -> BotCheckCoordinator {
        self.bot_check.clone()
    }

    /// Returns a clone of the detected-browsers list captured at startup.
    #[must_use]
    pub fn detected_browsers(&self) -> Vec<Browser> {
        (*self.detected_browsers).clone()
    }

    /// Adds a URL to the queue, expanding playlists when applicable.
    ///
    /// Logic:
    /// 1. Whole-URL dedup against the DB.
    /// 2. Try `expand_playlist(url)`. Empty Vec → single-video path; insert
    ///    one row with `title_status = pending` and spawn `get_title`.
    /// 3. Non-empty Vec → for each entry, dedup-check; insert with
    ///    `title_status = ok` (or `pending` if the entry's title is `None`,
    ///    and we'll re-fetch via the startup path); skip duplicates silently
    ///    within the playlist.
    /// 4. Each row snapshots the current Settings (format + dest dir).
    ///
    /// # Errors
    ///
    /// See [`AddError`] variants.
    #[allow(clippy::too_many_lines)]
    pub async fn add_url(
        &self,
        url: String,
        format_override: Option<FormatPref>,
    ) -> Result<AddOutcome, AddError> {
        let already_in_queue = {
            let url_q = url.clone();
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || db.with_conn(|c| queue::find_by_url(c, &url_q)))
                .await
                .map_err(|e| AddError::Db(DbError::Decode(format!("join error: {e}"))))??
        };
        if already_in_queue.is_some() {
            return Err(AddError::DuplicateUrl(url));
        }

        let cookies_arg = self.read_cookies_arg().await?;
        let js_runtime = self.js_runtime_path.as_ref().clone();
        let ffmpeg = self.ffmpeg_path.as_ref().clone();

        let entries = match self
            .bridge
            .expand_playlist(
                &url,
                cookies_arg.as_deref(),
                js_runtime.as_deref(),
                ffmpeg.as_deref(),
            )
            .await
        {
            Ok(entries) => entries,
            Err(err) => return Err(AddError::Bridge(err)),
        };

        // UC 16: snapshot the active destination at enqueue time. If neither
        // the per-OS Downloads dir nor the app-data dir resolves, refuse to
        // insert the row instead of falling back to `cwd`. The destination
        // is RE-resolved at spawn time (see `resolve_and_validate_dest_dir`)
        // so a queued item still picks up later Settings changes; this
        // snapshot just satisfies the column's `NOT NULL` contract.
        let default_root = paths::default_download_dir_or_app_data().map_err(|e| {
            AddError::Db(DbError::Decode(format!(
                "could not resolve any download destination on this system: {e}"
            )))
        })?;
        let (format_pref, dest_dir) = {
            let db = self.db.clone();
            let default_root = default_root.clone();
            let (settings_format, dest_dir) =
                tokio::task::spawn_blocking(move || -> Result<(FormatPref, PathBuf), DbError> {
                    db.with_conn(|c| {
                        let f = settings::get_format_pref(c)?;
                        let d = settings::get_dest_dir(c, &default_root)?;
                        Ok((f, d))
                    })
                })
                .await
                .map_err(|e| AddError::Db(DbError::Decode(format!("join error: {e}"))))??;
            // UC 19: per-URL override (e.g. AddBar's "Audio only" toggle)
            // wins over the Settings default. `FormatPref` is `Copy`.
            (format_override.unwrap_or(settings_format), dest_dir)
        };

        if entries.is_empty() {
            // Single-video path.
            let new_item = NewQueueItem {
                url: url.clone(),
                title: None,
                title_status: TitleStatus::Pending,
                format_pref,
                dest_dir: dest_dir.clone(),
            };
            let id = self.insert_item(new_item.clone()).await?;
            self.emit_row_for(id).await;
            self.spawn_title_fetch(id, url.clone());
            // UC 08: resolve the upstream thumbnail URL via the bridge
            // (single subprocess) BEFORE spawning the per-row fetch task.
            // The fetch task itself never spawns yt-dlp.
            self.spawn_thumbnail_fetch_for_single_video(id, url);
            self.wake();
            Ok(AddOutcome::Inserted { count: 1 })
        } else {
            // Playlist path.
            let mut count = 0usize;
            for entry in entries {
                let entry_url = entry.url.clone();
                let entry_thumbnail = entry.thumbnail.clone();
                let already = {
                    let db = self.db.clone();
                    let q = entry_url.clone();
                    tokio::task::spawn_blocking(move || db.with_conn(|c| queue::find_by_url(c, &q)))
                        .await
                        .map_err(|e| AddError::Db(DbError::Decode(format!("join error: {e}"))))??
                };
                if already.is_some() {
                    continue;
                }
                let (title, title_status) = match entry.title {
                    Some(t) => (Some(t), TitleStatus::Ok),
                    None => (None, TitleStatus::Pending),
                };
                let new_item = NewQueueItem {
                    url: entry_url.clone(),
                    title,
                    title_status,
                    format_pref,
                    dest_dir: dest_dir.clone(),
                };
                let id = self.insert_item(new_item).await?;
                self.emit_row_for(id).await;
                if matches!(title_status, TitleStatus::Pending) {
                    self.spawn_title_fetch(id, entry_url.clone());
                }
                // UC 08: spawn a per-row HTTP fetch when the playlist entry
                // already carries a thumbnail URL. Many extractors leave it
                // `None` even with `--flat-playlist` — those rows fall back
                // to the gradient placeholder until a future refresh.
                if let Some(thumb_url) = entry_thumbnail {
                    self.spawn_thumbnail_fetch(id, thumb_url);
                }
                count += 1;
            }
            self.wake();
            Ok(AddOutcome::Inserted { count })
        }
    }

    /// Wakes the queue-runner targeting one specific row id. The row's
    /// `status = queued` precondition is checked by the runner itself; this
    /// method is a no-op signal.
    pub fn start_one(&self, _id: i64) {
        self.wake();
    }

    /// UC 02: cancel a single row. Behaviour depends on the row's current
    /// `status` and `title_status` — see `use-cases/02-cancel-remove-and-restart.md`.
    ///
    /// - `title_status == Fetching`: notify the metadata cancel-token,
    ///   wait (bounded) for it to be removed by the title-fetch task,
    ///   then transition the row directly to `cancelled`. If the row is
    ///   ALSO `in_flight` (queue runner promoted it while the fetch was
    ///   still resolving), the in-flight cancel-token is fired too —
    ///   challenger flag A.
    /// - `Queued`: flip the row to `cancelled` directly. The supervisor's
    ///   `try_promote_to_in_flight` call (challenger flag B) will fail
    ///   when the runner picks the row up, so no yt-dlp child is ever
    ///   spawned.
    /// - `InFlight`: flip to the `Cancelling` transient state, fire the
    ///   download cancel-token, and let the supervisor's
    ///   `TerminalReason::Cancelled` block do the final
    ///   `Cancelling → Cancelled` transition once the bridge confirms
    ///   the subprocess is dead.
    /// - Any other state (`Done`, `Cancelled`, `Error`, `Cancelling`,
    ///   `Paused`): no-op + warn-log.
    pub async fn cancel_one(&self, id: i64) {
        let row = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
        };
        let Ok(Ok(Some(item))) = row else {
            tracing::warn!(id, "cancel_one: row not found");
            return;
        };

        let title_fetching = matches!(item.title_status, TitleStatus::Fetching);

        // 1. If the row is in_flight, flip to cancelling synchronously and
        //    fire the download cancel-token. Also fire metadata cancel if
        //    a title-fetch is still resolving in parallel.
        if matches!(item.status, QueueStatus::InFlight) {
            let _ = tokio::task::spawn_blocking({
                let db = self.db.clone();
                move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Cancelling))
            })
            .await;
            emit_row(&self.db, &self.ui_tx, id).await;

            if let Some(tok) = self.cancel_tokens.lock().await.get(&id) {
                tok.notify_one();
            }
            if title_fetching && let Some(tok) = self.metadata_cancel_tokens.lock().await.get(&id) {
                tok.notify_one();
            }
            return;
        }

        // 2. Title-fetch-only path: row is `queued` (not yet promoted) but
        //    its title is still resolving. Tear the metadata subprocess
        //    down, wait for the title-fetch task to drop its token, then
        //    flip the row to `cancelled` directly — there is no download
        //    supervisor running to do it for us.
        if title_fetching {
            if let Some(tok) = self.metadata_cancel_tokens.lock().await.get(&id) {
                tok.notify_one();
            }
            // Wait for the title-fetch task's cleanup to remove the token
            // from the map. Bounded by 5 s so a hung subprocess does not
            // stall the UI; on timeout we proceed regardless.
            let map = self.metadata_cancel_tokens.clone();
            let wait = async move {
                while map.lock().await.contains_key(&id) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            };
            if tokio::time::timeout(Duration::from_secs(5), wait)
                .await
                .is_err()
            {
                tracing::warn!(
                    id,
                    "cancel_one: metadata cancel token did not clear within 5s"
                );
            }

            let _ = tokio::task::spawn_blocking({
                let db = self.db.clone();
                move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Cancelled))
            })
            .await;
            emit_row(&self.db, &self.ui_tx, id).await;
            return;
        }

        // 3. Pure `queued` path: just flip the row. If the runner has
        //    already snapped it up between read and write, its
        //    `try_promote_to_in_flight` will see `status != 'queued'`
        //    and bail out without starting yt-dlp.
        if matches!(item.status, QueueStatus::Queued) {
            let _ = tokio::task::spawn_blocking({
                let db = self.db.clone();
                move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Cancelled))
            })
            .await;
            emit_row(&self.db, &self.ui_tx, id).await;
            return;
        }

        tracing::warn!(id, status = ?item.status, "cancel_one: row not in a cancellable state");
    }

    /// UC 02: cancel every row whose status is `queued` or `in_flight`,
    /// plus every row whose `title_status = fetching`. Status writes happen
    /// inside a single `SQLite` transaction; cancel-tokens (download AND
    /// metadata) are fired afterwards for each affected row.
    pub async fn cancel_all(&self) {
        let rows: Vec<QueueItem> = {
            let db = self.db.clone();
            let res = tokio::task::spawn_blocking(move || db.with_conn(queue::list_all)).await;
            match res {
                Ok(Ok(items)) => items
                    .into_iter()
                    .filter(|r| {
                        matches!(r.status, QueueStatus::Queued | QueueStatus::InFlight)
                            || matches!(r.title_status, TitleStatus::Fetching)
                    })
                    .collect(),
                _ => return,
            }
        };

        if rows.is_empty() {
            return;
        }

        // Single transaction: bulk status update.
        let snapshot: Vec<(i64, QueueStatus, TitleStatus)> = rows
            .iter()
            .map(|r| (r.id, r.status, r.title_status))
            .collect();
        let snapshot_for_db = snapshot.clone();
        let _ = tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || -> Result<(), DbError> {
                db.with_conn_mut(|c| {
                    let tx = c.transaction()?;
                    for (id, status, _title_status) in &snapshot_for_db {
                        match status {
                            QueueStatus::Queued => {
                                tx.execute(
                                    "UPDATE queue_items SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP WHERE id = ?",
                                    [id],
                                )?;
                            }
                            QueueStatus::InFlight => {
                                tx.execute(
                                    "UPDATE queue_items SET status = 'cancelling' WHERE id = ?",
                                    [id],
                                )?;
                            }
                            // title-fetch-only rows: status untouched here;
                            // they transition to `cancelled` after the
                            // metadata token fires (mirrors `cancel_one`'s
                            // title-fetching branch).
                            _ => {}
                        }
                    }
                    tx.commit()?;
                    Ok(())
                })
            }
        })
        .await;

        // Fire whichever cancel-token(s) are present per row, then emit a
        // RowUpserted so the UI sees the synchronous transient state.
        for (id, _status, title_status) in &snapshot {
            if let Some(tok) = self.cancel_tokens.lock().await.get(id) {
                tok.notify_one();
            }
            if matches!(title_status, TitleStatus::Fetching)
                && let Some(tok) = self.metadata_cancel_tokens.lock().await.get(id)
            {
                tok.notify_one();
            }
            emit_row(&self.db, &self.ui_tx, *id).await;
        }
    }

    /// UC 02: remove a row from the queue. Active rows (`queued`,
    /// `in_flight`, `cancelling`, or with `title_status = fetching`) are
    /// cancelled first and then deleted; terminal rows are deleted
    /// directly. The row's `.part` file is deleted from disk if its
    /// `partial_file_path` is set; the finished media file of a `done`
    /// row is never touched.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] when the DB read or write fails.
    pub async fn remove_one(&self, id: i64) -> Result<(), DbError> {
        let row = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??
        };
        let Some(item) = row else {
            tracing::warn!(id, "remove_one: row not found");
            return Ok(());
        };

        let active = matches!(
            item.status,
            QueueStatus::Queued | QueueStatus::InFlight | QueueStatus::Cancelling
        ) || matches!(item.title_status, TitleStatus::Fetching);

        if active {
            self.cancel_one(id).await;
            // Wait until the row reaches `cancelled` (or terminal) before
            // touching the `.part` file — yt-dlp may still be flushing.
            if tokio::time::timeout(
                Duration::from_secs(5),
                wait_until_terminal(self.db.clone(), id),
            )
            .await
            .is_err()
            {
                tracing::warn!(
                    id,
                    "remove_one: cancel did not confirm within 5s; proceeding"
                );
            }
        }

        // Re-read the row so we pick up any partial_file_path that the
        // bridge persisted between the original read and the cancel
        // confirmation.
        let final_item = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??
        };

        if let Some(item) = final_item.as_ref()
            && let Some(part) = item.partial_file_path.as_ref()
            && tokio::fs::try_exists(part).await.unwrap_or(false)
            && let Err(err) = tokio::fs::remove_file(part).await
        {
            tracing::warn!(id, ?err, path = %part.display(), "remove_one: .part file removal failed");
        }

        let _ = tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || db.with_conn_mut(|c| queue::delete_by_id(c, id))
        })
        .await;

        let _ = self.ui_tx.send(UiEvent::RowRemoved(id)).await;
        Ok(())
    }

    /// UC 12: clear the entire queue.
    ///
    /// Flow:
    /// 1. List all rows and partition into "active" (rows the per-row cancel
    ///    pipeline must tear down — `Queued`, `InFlight`, `Cancelling`, or
    ///    `title_status = fetching`) and "terminal" (everything else,
    ///    deleted directly).
    /// 2. Fire `cancel_one` on each active id sequentially. The existing
    ///    pipeline handles each row's state transitions and — crucially for
    ///    AC #9 — invokes `BotCheckCoordinator::withdraw` on any
    ///    `waiting_on_user` row via the supervisor's `cancel.notified()`
    ///    arm. No new bot-check helper is added.
    /// 3. Wait (concurrently, capped at 5 s wall-clock) for every formerly-
    ///    active row to leave the `in_flight` / `cancelling` transient
    ///    states — yt-dlp may still be flushing.
    /// 4. Best-effort delete each row's `.part` file (re-read so any path
    ///    persisted by the bridge between steps 2 and 3 is picked up).
    /// 5. Single SQLite transaction: prune `history` rows referencing
    ///    `done` queue items (the `history.queue_item_id NOT NULL
    ///    REFERENCES queue_items(id)` FK with `PRAGMA foreign_keys = ON`
    ///    would otherwise reject the bulk delete), then `DELETE FROM
    ///    queue_items`. AC #10 — the rest of `history` is untouched, so
    ///    completed-download history remains append-only for any rows that
    ///    don't survive in the queue but whose history entries we keep.
    ///    (Today every `done` row carries at most one history entry; the
    ///    prune scope is exactly those.)
    /// 6. Emit `RowRemoved` per id so `ui_bridge` clears the Slint
    ///    VecModel and `recompute_counts` runs (AC #7).
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] when the seed read or the bulk-
    /// delete transaction fails. Partial state (cancels fired but the
    /// transaction failed) leaves cancelled rows visible in the queue —
    /// identical posture to a successful Cancel-all followed by a failed
    /// per-row Remove.
    #[allow(clippy::too_many_lines)]
    pub async fn remove_all(&self) -> Result<(), DbError> {
        let rows: Vec<QueueItem> = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || db.with_conn(queue::list_all))
                .await
                .map_err(|e| DbError::Decode(format!("join error: {e}")))??
        };

        if rows.is_empty() {
            return Ok(());
        }

        let mut active_ids: Vec<i64> = Vec::new();
        let mut all_ids: Vec<i64> = Vec::with_capacity(rows.len());
        for row in &rows {
            all_ids.push(row.id);
            let is_active = matches!(
                row.status,
                QueueStatus::Queued | QueueStatus::InFlight | QueueStatus::Cancelling
            ) || matches!(row.title_status, TitleStatus::Fetching);
            if is_active {
                active_ids.push(row.id);
            }
        }

        // Step 2: fire cancel-one per active id, sequentially. The cancel
        // pipeline holds locks on `cancel_tokens` / `metadata_cancel_tokens`;
        // sequential keeps the lock posture identical to a stream of
        // per-row Cancel clicks and avoids reordering the bridge's stdout
        // drain across rows.
        for &id in &active_ids {
            self.cancel_one(id).await;
        }

        // Step 3: wait for every active row to reach a terminal-ish state,
        // capped at 5 s wall-clock total (not per-row) via a single shared
        // deadline. Each wait runs on its own tokio task (`JoinSet`) so the
        // 100 ms polling intervals interleave instead of serializing —
        // wall-clock collapses from `Σ(per-row)` to `max(per-row, 5 s)`.
        if !active_ids.is_empty() {
            let mut join_set = tokio::task::JoinSet::new();
            for &id in &active_ids {
                let db = self.db.clone();
                join_set.spawn(async move {
                    wait_until_terminal(db, id).await;
                });
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            let mut timed_out = false;
            loop {
                match tokio::time::timeout_at(deadline, join_set.join_next()).await {
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => {
                        timed_out = true;
                        break;
                    }
                }
            }
            if timed_out {
                tracing::warn!("remove_all: not all cancels confirmed within 5s; proceeding");
                join_set.abort_all();
            }
        }

        // Step 4: best-effort delete any `.part` files. Re-read each row so
        // a `partial_file_path` persisted by the bridge between step 2 and
        // step 3 is observed; failures log at WARN and do not abort the
        // bulk delete (same posture as `remove_one`).
        for &id in &all_ids {
            let final_item = {
                let db = self.db.clone();
                let res = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
                })
                .await;
                match res {
                    Ok(Ok(item)) => item,
                    _ => None,
                }
            };
            if let Some(item) = final_item.as_ref()
                && let Some(part) = item.partial_file_path.as_ref()
                && tokio::fs::try_exists(part).await.unwrap_or(false)
                && let Err(err) = tokio::fs::remove_file(part).await
            {
                tracing::warn!(id, ?err, path = %part.display(), "remove_all: .part file removal failed");
            }
        }

        // Step 5: single transaction — prune history rows for `done`
        // queue items, then delete every queue row. The history prune
        // mirrors `queue::delete_by_id`'s per-row cascade. `PRAGMA
        // foreign_keys = ON` (set at connection init in `db/mod.rs`)
        // would otherwise reject the bulk `DELETE FROM queue_items` for
        // any `done` row with a history entry.
        tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || -> Result<(), DbError> {
                db.with_conn_mut(|c| {
                    let tx = c.transaction()?;
                    tx.execute(
                        "DELETE FROM history WHERE queue_item_id IN \
                         (SELECT id FROM queue_items WHERE status = 'done')",
                        [],
                    )?;
                    tx.execute("DELETE FROM queue_items", [])?;
                    tx.commit()?;
                    Ok(())
                })
            }
        })
        .await
        .map_err(|e| DbError::Decode(format!("join error: {e}")))??;

        // Step 6: emit RowRemoved per id so ui_bridge mirrors the DB
        // (AC #7). Sent on the existing bounded mpsc channel; the UI bridge
        // drains them serially.
        for &id in &all_ids {
            let _ = self.ui_tx.send(UiEvent::RowRemoved(id)).await;
        }

        Ok(())
    }

    /// UC 02: restart a cancelled row. Resets progress fields back to a
    /// fresh `queued` state while preserving `size_bytes` and
    /// `partial_file_path` so yt-dlp's `--continue` can resume from the
    /// existing `.part` file at the row's snapshotted `dest_dir`.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] when the DB read or write fails.
    pub async fn restart_one(&self, id: i64) -> Result<(), DbError> {
        let row = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??
        };
        let Some(item) = row else {
            tracing::warn!(id, "restart_one: row not found");
            return Ok(());
        };
        if !matches!(item.status, QueueStatus::Cancelled) {
            tracing::warn!(id, status = ?item.status, "restart_one: row is not cancelled");
            return Ok(());
        }

        let _ = tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || db.with_conn(|c| queue::clear_for_restart(c, id))
        })
        .await
        .map_err(|e| DbError::Decode(format!("join error: {e}")))?;

        emit_row(&self.db, &self.ui_tx, id).await;
        self.wake();
        Ok(())
    }

    /// UC 14: broaden the footer "Start all" button to also resume
    /// `cancelled` rows and retry `error` rows in addition to starting
    /// `queued` ones. For every non-queued row in the resumable set the
    /// row is reset via `queue::clear_for_restart` (same path used by
    /// per-row Restart) inside one transaction so a partial flip never
    /// leaves the DB inconsistent. Active states (`in_flight`,
    /// `cancelling`, plus rows whose title fetch is still resolving)
    /// are deliberately untouched. `wake()` then lets the existing
    /// queue runner promote rows oldest-first under the concurrency cap.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] when the seed read or the
    /// bulk-reset transaction fails.
    pub async fn start_all(&self) -> Result<(), DbError> {
        let rows: Vec<QueueItem> = {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || db.with_conn(queue::list_all))
                .await
                .map_err(|e| DbError::Decode(format!("join error: {e}")))??
        };

        let non_queued_ids: Vec<i64> = rows
            .iter()
            .filter(|r| matches!(r.status, QueueStatus::Cancelled | QueueStatus::Error))
            .map(|r| r.id)
            .collect();

        if !non_queued_ids.is_empty() {
            let ids_for_db = non_queued_ids.clone();
            tokio::task::spawn_blocking({
                let db = self.db.clone();
                move || -> Result<(), DbError> {
                    db.with_conn_mut(|c| {
                        let tx = c.transaction()?;
                        for id in &ids_for_db {
                            queue::clear_for_restart(&tx, *id)?;
                        }
                        tx.commit()?;
                        Ok(())
                    })
                }
            })
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??;
        }

        for id in &non_queued_ids {
            emit_row(&self.db, &self.ui_tx, *id).await;
        }

        self.wake();
        Ok(())
    }

    /// Returns the most up-to-date list of UI rows. Used by the UI to
    /// rebuild its model on initial paint and after settings panel toggles.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`].
    pub async fn list_ui_rows(&self) -> Result<Vec<UiQueueRow>, DbError> {
        let db = self.db.clone();
        let items = tokio::task::spawn_blocking(move || db.with_conn(queue::list_all))
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??;
        Ok(items.into_iter().map(to_ui_row).collect())
    }

    fn wake(&self) {
        // Best-effort; `try_send` is fine because a missed wake is recovered
        // by the next add or completion. The bound (16) is large enough that
        // loss is improbable, but the runner re-checks on every tick anyway.
        let _ = self.wake_tx.try_send(());
    }

    async fn insert_item(&self, item: NewQueueItem) -> Result<i64, AddError> {
        let db = self.db.clone();
        let id = tokio::task::spawn_blocking(move || db.with_conn(|c| queue::insert(c, item)))
            .await
            .map_err(|e| AddError::Db(DbError::Decode(format!("join error: {e}"))))??;
        Ok(id)
    }

    async fn emit_row_for(&self, id: i64) {
        let db = self.db.clone();
        let row = tokio::task::spawn_blocking(move || {
            db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
        })
        .await;
        if let Ok(Ok(Some(item))) = row {
            let _ = self.ui_tx.send(UiEvent::RowUpserted(to_ui_row(item))).await;
        }
    }

    async fn read_cookies_arg(&self) -> Result<Option<String>, AddError> {
        let db = self.db.clone();
        let choice =
            tokio::task::spawn_blocking(move || db.with_conn(settings::get_cookies_browser))
                .await
                .map_err(|e| AddError::Db(DbError::Decode(format!("join error: {e}"))))??;
        Ok(choice.map(|b| b.as_yt_dlp_arg().to_string()))
    }

    async fn read_cookies_arg_db_only(&self) -> Option<String> {
        let db = self.db.clone();
        let res =
            tokio::task::spawn_blocking(move || db.with_conn(settings::get_cookies_browser)).await;
        match res {
            Ok(Ok(opt)) => opt.map(|b| b.as_yt_dlp_arg().to_string()),
            _ => None,
        }
    }

    /// UC 16: read the active destination from settings, validate it exists
    /// and is writable, then persist it back onto the row. Returns the
    /// resolved path on success or a user-facing error message on failure.
    ///
    /// The persisted `dest_dir` is gated on `status = 'in_flight'` (see
    /// `queue::update_dest_dir`) so a Cancel that races the supervisor's
    /// promotion does not have its `dest_dir` overwritten under it.
    async fn resolve_and_validate_dest_dir(&self, id: i64) -> Result<PathBuf, String> {
        let default_root = paths::default_download_dir_or_app_data().map_err(|e| {
            format!("could not resolve any download destination on this system: {e}")
        })?;

        let db = self.db.clone();
        let resolved = tokio::task::spawn_blocking({
            let default_root = default_root.clone();
            move || db.with_conn(|c| settings::get_dest_dir(c, &default_root))
        })
        .await
        .map_err(|e| format!("could not read destination setting: {e}"))?
        .map_err(|e| format!("could not read destination setting: {e}"))?;

        match tokio::fs::metadata(&resolved).await {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "destination is not a directory: {}",
                    resolved.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "destination folder does not exist or is unreadable ({}): {e}",
                    resolved.display()
                ));
            }
        }

        validate_dest_dir_writable(&resolved, id).await?;

        let db = self.db.clone();
        let resolved_for_db = resolved.clone();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| queue::update_dest_dir(c, id, &resolved_for_db))
        })
        .await
        .map_err(|e| format!("could not persist destination on row: {e}"))?
        .map_err(|e| format!("could not persist destination on row: {e}"))?;

        Ok(resolved)
    }

    fn spawn_title_fetch(&self, id: i64, url: String) {
        let bridge = self.bridge.clone();
        let db = self.db.clone();
        let ui_tx = self.ui_tx.clone();
        let js_runtime = self.js_runtime_path.as_ref().clone();
        let ffmpeg = self.ffmpeg_path.as_ref().clone();
        let mgr = self.clone();
        let metadata_cancel_tokens = self.metadata_cancel_tokens.clone();
        tokio::spawn(async move {
            let db_for_set = db.clone();
            let _ = tokio::task::spawn_blocking(move || {
                db_for_set.with_conn(|c| queue::update_title(c, id, None, TitleStatus::Fetching))
            })
            .await;

            let cancel = Arc::new(Notify::new());
            metadata_cancel_tokens
                .lock()
                .await
                .insert(id, cancel.clone());

            let cookies_arg = mgr.read_cookies_arg_db_only().await;
            let result = bridge
                .fetch_title_cancellable(
                    &url,
                    cookies_arg.as_deref(),
                    js_runtime.as_deref(),
                    ffmpeg.as_deref(),
                    cancel,
                )
                .await;

            let db_for_write = db.clone();
            match result {
                Ok(title) => {
                    let title_clone = title.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        db_for_write.with_conn(|c| {
                            queue::update_title(c, id, Some(&title_clone), TitleStatus::Ok)
                        })
                    })
                    .await;
                }
                Err(BridgeError::Cancelled) => {
                    // UC 02: cancellation must NOT taint the row's
                    // title-fetch error column. Reset the row to
                    // `pending` so a future Restart-and-resume re-issues
                    // the fetch cleanly.
                    let _ = tokio::task::spawn_blocking(move || {
                        db_for_write
                            .with_conn(|c| queue::update_title(c, id, None, TitleStatus::Pending))
                    })
                    .await;
                }
                Err(err) => {
                    let msg = err.to_string();
                    let msg_clone = msg.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        db_for_write.with_conn(|c| queue::set_title_error(c, id, &msg_clone))
                    })
                    .await;
                }
            }

            // Always remove the metadata cancel-token AFTER the bridge call
            // returns so `cancel_one`'s wait-loop terminates.
            metadata_cancel_tokens.lock().await.remove(&id);

            // Tell the UI about the new state.
            let db_for_read = db;
            if let Ok(Ok(Some(item))) = tokio::task::spawn_blocking(move || {
                db_for_read.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
            {
                let _ = ui_tx.send(UiEvent::RowUpserted(to_ui_row(item))).await;
            }
        });
    }

    /// Re-issues `get_title` for every row whose title fetch is still
    /// `pending` or `fetching` after a restart.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] if the seed list cannot be read.
    pub async fn requeue_pending_title_fetches(&self) -> Result<(), DbError> {
        let db = self.db.clone();
        let rows: Vec<QueueItem> =
            tokio::task::spawn_blocking(move || db.with_conn(queue::list_titles_to_fetch))
                .await
                .map_err(|e| DbError::Decode(format!("join error: {e}")))??;
        for row in rows {
            self.spawn_title_fetch(row.id, row.url.clone());
        }
        Ok(())
    }

    /// Re-issues per-row thumbnail fetches for every row that has not yet
    /// cached one (UC 08). N rows × 1 yt-dlp subprocess each — bounded but
    /// visible at startup; documented in ADR 0008.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] if the seed list cannot be read.
    pub async fn requeue_pending_thumbnail_fetches(&self) -> Result<(), DbError> {
        let db = self.db.clone();
        let rows: Vec<QueueItem> = tokio::task::spawn_blocking(move || {
            db.with_conn(queue::list_pending_thumbnail_fetches)
        })
        .await
        .map_err(|e| DbError::Decode(format!("join error: {e}")))??;
        for row in rows {
            self.spawn_thumbnail_fetch_for_single_video(row.id, row.url.clone());
        }
        Ok(())
    }

    /// Resolves the upstream thumbnail URL via a single yt-dlp subprocess,
    /// then hands the URL to [`Self::spawn_thumbnail_fetch`] so the per-row
    /// HTTP fetcher can run without re-spawning yt-dlp. Failures are logged
    /// at WARN; the row keeps its gradient placeholder.
    fn spawn_thumbnail_fetch_for_single_video(&self, id: i64, url: String) {
        let bridge = self.bridge.clone();
        let js_runtime = self.js_runtime_path.as_ref().clone();
        let ffmpeg = self.ffmpeg_path.as_ref().clone();
        let semaphore = self.thumbnail_resolve_semaphore.clone();
        let mgr = self.clone();
        tokio::spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return;
            };
            let cookies_arg = mgr.read_cookies_arg_db_only().await;
            match bridge
                .fetch_thumbnail_url(
                    &url,
                    cookies_arg.as_deref(),
                    js_runtime.as_deref(),
                    ffmpeg.as_deref(),
                )
                .await
            {
                Ok(thumb_url) => {
                    mgr.spawn_thumbnail_fetch(id, thumb_url);
                }
                Err(err) => {
                    tracing::warn!(?err, %url, id, "thumbnail URL resolution failed");
                }
            }
        });
    }

    /// Spawns a per-row HTTP fetch task that downloads the upstream
    /// thumbnail and writes it to the on-disk cache. Emits
    /// [`UiEvent::ThumbnailReady`] on success so the UI bridge can flip
    /// the row's `thumbnail-loaded` to `true`. Errors log at WARN; the row
    /// keeps its gradient placeholder.
    fn spawn_thumbnail_fetch(&self, id: i64, thumb_url: String) {
        let cache_dir = self.thumbnail_cache_dir.as_ref().clone();
        let db = self.db.clone();
        let ui_tx = self.ui_tx.clone();
        tokio::spawn(async move {
            match crate::thumbnails::fetch_and_cache_thumbnail(&thumb_url, &cache_dir).await {
                Ok(path) => {
                    let path_clone = path.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        db.with_conn(|c| queue::set_thumbnail_path(c, id, &path_clone))
                    })
                    .await;
                    let _ = ui_tx.send(UiEvent::ThumbnailReady { id, path }).await;
                }
                Err(err) => {
                    tracing::warn!(?err, %thumb_url, id, "thumbnail fetch failed");
                }
            }
        });
    }

    async fn run_loop(self, mut wake_rx: mpsc::Receiver<()>) {
        // Initial wake so we pick up any rows that were already `queued`
        // when the manager started.
        let _ = self.wake_tx.try_send(());
        while wake_rx.recv().await.is_some() {
            self.try_promote_queued().await;
        }
    }

    async fn try_promote_queued(&self) {
        let db = self.db.clone();
        let queued = tokio::task::spawn_blocking(move || db.with_conn(queue::list_queued))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();

        for row in queued {
            let Ok(permit) = self.semaphore.clone().try_acquire_owned() else {
                // No permits available — leave the rest as `queued`; we get
                // re-woken when a permit is released.
                return;
            };
            self.spawn_download_for(&row, permit);
        }
    }

    #[allow(
        clippy::too_many_lines,
        clippy::needless_continue,
        clippy::match_same_arms
    )]
    fn spawn_download_for(&self, item: &QueueItem, permit: tokio::sync::OwnedSemaphorePermit) {
        let id = item.id;
        let url = item.url.clone();
        let format_pref = item.format_pref;

        let db = self.db.clone();
        let bridge = self.bridge.clone();
        let ui_tx = self.ui_tx.clone();
        let cancel_tokens = self.cancel_tokens.clone();
        let wake = self.wake_tx.clone();
        let coordinator = self.bot_check.clone();
        let detected = self.detected_browsers.clone();
        let js_runtime = self.js_runtime_path.as_ref().clone();
        let ffmpeg = self.ffmpeg_path.as_ref().clone();
        let mgr_cookies = self.clone();
        let mgr_dest = self.clone();

        tokio::spawn(async move {
            // UC 02: atomic promotion `queued → in_flight`. If the row is
            // not still `queued` (typically because `cancel_one` raced
            // ahead and flipped it to `cancelled`), bail out before
            // spawning yt-dlp. Drop the permit early so it returns to the
            // semaphore for a different row.
            let promoted = tokio::task::spawn_blocking({
                let db = db.clone();
                move || db.with_conn(|c| queue::try_promote_to_in_flight(c, id))
            })
            .await;
            let promoted = matches!(promoted, Ok(Ok(true)));
            if !promoted {
                drop(permit);
                emit_row(&db, &ui_tx, id).await;
                let _ = wake.try_send(());
                return;
            }
            emit_row(&db, &ui_tx, id).await;

            // UC 17: unconditional ffmpeg gate. yt-dlp falls back to
            // separate audio/video tracks when ffmpeg is missing AND the
            // requested format requires merging — and even audio-only
            // formats need ffmpeg for the M4A → MP3/Opus extract path.
            // Refusing to spawn here yields a clear, user-visible error
            // instead of a yt-dlp stderr blob that mentions ffmpeg only
            // in passing. Release vs dev branches surface different
            // remediation copy.
            if ffmpeg.is_none() {
                let msg = if cfg!(debug_assertions) {
                    "ffmpeg is missing from runtime-deps/. Run `just fetch-runtime-deps`."
                        .to_string()
                } else {
                    "Bundled ffmpeg is missing from this installation. Reinstall the app."
                        .to_string()
                };
                let msg_for_db = msg.clone();
                let _ = tokio::task::spawn_blocking({
                    let db = db.clone();
                    move || db.with_conn(|c| queue::set_error_msg(c, id, &msg_for_db))
                })
                .await;
                let _ = tokio::task::spawn_blocking({
                    let db = db.clone();
                    move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Error))
                })
                .await;
                emit_row(&db, &ui_tx, id).await;
                drop(permit);
                let _ = wake.try_send(());
                return;
            }

            // UC 16: re-resolve and validate the destination at SPAWN time
            // (not enqueue time) so queued items pick up subsequent
            // settings changes. Existence + writability are checked here;
            // failure routes the row to `error` without spawning yt-dlp,
            // without auto-mkdir, and without a silent fallback. The
            // resolved path is persisted onto the row via `update_dest_dir`
            // (gated on status = 'in_flight') so the UI and history reflect
            // the actual landing folder.
            let dest_dir = match mgr_dest.resolve_and_validate_dest_dir(id).await {
                Ok(p) => p,
                Err(msg) => {
                    let msg_for_db = msg.clone();
                    let _ = tokio::task::spawn_blocking({
                        let db = db.clone();
                        move || db.with_conn(|c| queue::set_error_msg(c, id, &msg_for_db))
                    })
                    .await;
                    let _ = tokio::task::spawn_blocking({
                        let db = db.clone();
                        move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Error))
                    })
                    .await;
                    emit_row(&db, &ui_tx, id).await;
                    drop(permit);
                    let _ = wake.try_send(());
                    return;
                }
            };

            let cancel = Arc::new(Notify::new());
            cancel_tokens.lock().await.insert(id, cancel.clone());

            let initial_cookies = mgr_cookies.read_cookies_arg_db_only().await;
            let mut req = DownloadRequest {
                url: url.clone(),
                format: format_pref,
                dest_dir: dest_dir.clone(),
                cookies_browser: initial_cookies,
                js_runtime_path: js_runtime.clone(),
                ffmpeg_path: ffmpeg.clone(),
            };
            let (mut events, mut handle) = bridge.start_download(req.clone(), cancel.clone());
            // Tracks whether the row already retried with cookies; once true,
            // a second AuthRequired falls through as an error rather than
            // re-prompting (AC#10 — no infinite re-prompt for the same row).
            let mut retried_with_cookies: bool = false;
            // UC 02: distinguish a user-cancelled terminal state from an
            // error terminal state so the post-loop block can flip the row
            // to `cancelled` without writing into `error_msg`. The
            // supervisor never writes a row directly to `error` from
            // inside the loop; it just records the reason and lets the
            // single post-loop match do the DB write.
            let mut terminal: Option<TerminalReason> = None;

            'supervisor: loop {
                while let Some(event) = events.recv().await {
                    match event {
                        DownloadEvent::Started | DownloadEvent::PostProcessing => {}
                        DownloadEvent::Progress {
                            pct,
                            speed_bps,
                            eta_s,
                            downloaded_bytes,
                            total_bytes,
                        } => {
                            let _ = tokio::task::spawn_blocking({
                                let db = db.clone();
                                move || {
                                    db.with_conn(|c| {
                                        queue::update_progress(
                                            c,
                                            id,
                                            pct,
                                            speed_bps,
                                            eta_s,
                                            downloaded_bytes,
                                            total_bytes,
                                        )
                                    })
                                }
                            })
                            .await;
                            emit_row(&db, &ui_tx, id).await;
                        }
                        DownloadEvent::Finished { bytes, .. } => {
                            let _ = tokio::task::spawn_blocking({
                                let db = db.clone();
                                move || db.with_conn(|c| queue::set_finished(c, id, bytes))
                            })
                            .await;
                            emit_row(&db, &ui_tx, id).await;
                        }
                        DownloadEvent::Error { .. } => {
                            // Defer the DB write to the post-await branch so
                            // a typed `AuthRequired` can re-route the row
                            // through the bot-check dialog instead of erroring.
                        }
                        DownloadEvent::PartialFilePath { path } => {
                            // UC 02: persist so Remove can clean up the
                            // `.part` file later. No `emit_row` — the path
                            // is invisible to the UI.
                            let _ = tokio::task::spawn_blocking({
                                let db = db.clone();
                                move || db.with_conn(|c| queue::update_partial_path(c, id, &path))
                            })
                            .await;
                        }
                    }
                }

                let join_result = (&mut handle).await;
                match join_result {
                    Ok(Ok(())) => break 'supervisor,
                    Ok(Err(BridgeError::Cancelled)) => {
                        terminal = Some(TerminalReason::Cancelled);
                        break 'supervisor;
                    }
                    Ok(Err(BridgeError::AuthRequired { .. })) => {
                        if retried_with_cookies {
                            terminal = Some(TerminalReason::Error(
                                "YouTube blocked this download even with cookies; check Settings → Cookies source.".to_string(),
                            ));
                            break 'supervisor;
                        }
                        let (retry_tx, retry_rx) = oneshot::channel::<RetryDecision>();
                        let outcome = coordinator.report_auth_required(id, retry_tx).await;
                        if matches!(outcome, CoordinatorOutcome::OpenDialog) {
                            let _ = ui_tx
                                .send(UiEvent::ShowBotCheckDialog {
                                    available: (*detected).clone(),
                                })
                                .await;
                        }
                        // UC 10: feed the modal header's affected-count
                        // copy. The count is the total pending registry
                        // size after this row's report.
                        let count = coordinator.pending_count().await;
                        let _ = ui_tx
                            .send(UiEvent::BotCheckAffectedCount {
                                count: u32::try_from(count).unwrap_or(u32::MAX),
                            })
                            .await;
                        let _ = ui_tx
                            .send(UiEvent::RowWaitingOnUser { id, waiting: true })
                            .await;

                        let decision = tokio::select! {
                            res = retry_rx => res.ok(),
                            () = cancel.notified() => {
                                coordinator.withdraw(id).await;
                                // UC 10: refresh the affected-count header
                                // copy — one fewer row is waiting now.
                                let count = coordinator.pending_count().await;
                                let _ = ui_tx
                                    .send(UiEvent::BotCheckAffectedCount {
                                        count: u32::try_from(count).unwrap_or(u32::MAX),
                                    })
                                    .await;
                                let _ = ui_tx.send(UiEvent::RowWaitingOnUser { id, waiting: false }).await;
                                terminal = Some(TerminalReason::Cancelled);
                                break 'supervisor;
                            }
                        };

                        let _ = ui_tx
                            .send(UiEvent::RowWaitingOnUser { id, waiting: false })
                            .await;

                        match decision {
                            Some(RetryDecision::PickedBrowser(arg)) => {
                                retried_with_cookies = true;
                                // UC 16: re-validate writability before the
                                // retry spawn. The dest_dir is reused (no
                                // settings re-read mid-supervisor), but the
                                // folder may have been deleted, made
                                // read-only, or unmounted while the user was
                                // in the bot-check dialog. A failure here
                                // terminates the row via the supervisor's
                                // single error sink.
                                if let Err(e) = validate_dest_dir_writable(&dest_dir, id).await {
                                    terminal = Some(TerminalReason::Error(e));
                                    break 'supervisor;
                                }
                                let new_cancel = Arc::new(Notify::new());
                                cancel_tokens.lock().await.insert(id, new_cancel.clone());
                                req = DownloadRequest {
                                    url: url.clone(),
                                    format: format_pref,
                                    dest_dir: dest_dir.clone(),
                                    cookies_browser: Some(arg),
                                    js_runtime_path: js_runtime.clone(),
                                    ffmpeg_path: ffmpeg.clone(),
                                };
                                let (new_events, new_handle) =
                                    bridge.start_download(req.clone(), new_cancel.clone());
                                events = new_events;
                                handle = new_handle;
                                continue 'supervisor;
                            }
                            Some(RetryDecision::Cancelled) | None => {
                                terminal = Some(TerminalReason::Error(
                                    "YouTube blocked this download. Set a Cookies source in Settings to retry.".to_string(),
                                ));
                                break 'supervisor;
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        terminal = Some(TerminalReason::Error(err.to_string()));
                        break 'supervisor;
                    }
                    Err(join_err) => {
                        terminal = Some(TerminalReason::Error(format!(
                            "download supervisor task failed: {join_err}"
                        )));
                        break 'supervisor;
                    }
                }
            }

            match terminal {
                Some(TerminalReason::Cancelled) => {
                    let _ = tokio::task::spawn_blocking({
                        let db = db.clone();
                        move || {
                            db.with_conn(|c| queue::update_status(c, id, QueueStatus::Cancelled))
                        }
                    })
                    .await;
                    emit_row(&db, &ui_tx, id).await;
                }
                Some(TerminalReason::Error(msg)) => {
                    let msg_for_db = msg.clone();
                    let _ = tokio::task::spawn_blocking({
                        let db = db.clone();
                        move || db.with_conn(|c| queue::set_error_msg(c, id, &msg_for_db))
                    })
                    .await;
                    let _ = tokio::task::spawn_blocking({
                        let db = db.clone();
                        move || db.with_conn(|c| queue::update_status(c, id, QueueStatus::Error))
                    })
                    .await;
                    emit_row(&db, &ui_tx, id).await;
                }
                None => {
                    // Success path — `set_finished` already ran on the
                    // `Finished` event. Nothing else to do.
                }
            }

            cancel_tokens.lock().await.remove(&id);
            drop(permit);
            // Wake the runner so the next queued item is picked up.
            let _ = wake.try_send(());
        });
    }
}

/// Polls the row until it leaves `in_flight` / `cancelling`. Shared by
/// `remove_one` (single-row case) and `remove_all` (bulk case).
///
/// The caller is responsible for the surrounding `tokio::time::timeout`
/// because the bulk path drives many of these concurrently under a single
/// outer timeout — embedding the timeout here would force a per-row
/// 5 s wall-clock floor instead of letting `tokio::task::JoinSet` collapse
/// the wait to `max(per-row, 5 s)`.
async fn wait_until_terminal(db: Db, id: i64) {
    loop {
        let snapshot = {
            let db = db.clone();
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
            })
            .await
        };
        match snapshot {
            Ok(Ok(Some(r)))
                if !matches!(r.status, QueueStatus::InFlight | QueueStatus::Cancelling) =>
            {
                return;
            }
            // Row already vanished (e.g. another path deleted it
            // concurrently) — nothing to wait on.
            Ok(Ok(None)) => return,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// UC 16 writability touch-test: create-and-remove a probe file in `dir`.
///
/// Used both at initial spawn and at the top of the bot-check retry path so a
/// destination that becomes unwritable mid-life surfaces as `error` rather
/// than as a yt-dlp stderr blob. The probe filename mixes the row `id` and
/// the current monotonic-nanosecond clock so concurrent supervisors cannot
/// collide on the same path. No `rand` dep is added (AC#11).
async fn validate_dest_dir_writable(dir: &Path, id: i64) -> Result<(), String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let probe = dir.join(format!(".yt-dlp-ui-write-probe-{id}-{nanos}"));
    match tokio::fs::File::create(&probe).await {
        Ok(_) => {
            // Best-effort cleanup; ignore errors (next run's probe will use a
            // distinct filename, so a leaked probe is harmless).
            let _ = tokio::fs::remove_file(&probe).await;
            Ok(())
        }
        Err(e) => Err(format!(
            "destination folder is not writable ({}): {e}",
            dir.display()
        )),
    }
}

async fn emit_row(db: &Db, ui_tx: &mpsc::Sender<UiEvent>, id: i64) {
    let db = db.clone();
    if let Ok(Ok(Some(item))) = tokio::task::spawn_blocking(move || {
        db.with_conn(|c| queue::find_by_url_by_id_internal(c, id))
    })
    .await
    {
        let _ = ui_tx.send(UiEvent::RowUpserted(to_ui_row(item))).await;
    }
}

fn to_ui_row(item: QueueItem) -> UiQueueRow {
    let title = item
        .title
        .clone()
        .unwrap_or_else(|| "Fetching…".to_string());
    UiQueueRow {
        id: item.id,
        url: item.url,
        title,
        title_status: item.title_status,
        title_error: item.title_error,
        status: item.status,
        progress_pct: item.progress_pct.unwrap_or(0.0),
        speed_bps: item.speed_bps,
        eta_s: item.eta_s,
        error_msg: item.error_msg,
        dest_dir: item.dest_dir,
        size_bytes: item.size_bytes,
        downloaded_bytes: item.downloaded_bytes,
        thumbnail_path: item.thumbnail_path,
    }
}
