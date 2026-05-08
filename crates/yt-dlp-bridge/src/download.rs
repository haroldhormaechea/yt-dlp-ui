//! Download orchestration around `yt-dlp`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use crate::auth::is_bot_check_stderr;
use crate::cancel::terminate_with_grace;
use crate::error::{BridgeError, Result};
use crate::format::FormatPref;
use crate::parser::{parse_destination_line, parse_progress_line};

/// Maximum bytes of stderr buffered by the bridge for the `ExitedWithError`
/// `stderr_tail` field. Older bytes are evicted on overflow.
const STDERR_TAIL_BUDGET: usize = 4096;

/// Bound on the events channel returned by [`start`]. Twenty events is enough
/// to absorb a brief consumer hiccup while still bounding memory.
const EVENT_CHANNEL_BUFFER: usize = 20;

/// `--print after_move:filepath` marker prefix used by the bridge to recognize
/// the final file path emitted by yt-dlp.
const FILEPATH_MARKER: &str = "yt-dlp-ui-filepath ";

/// Caller-supplied parameters for a single download.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub format: FormatPref,
    pub dest_dir: PathBuf,
    /// Optional yt-dlp `--cookies-from-browser` argument value (e.g. `"chrome"`,
    /// `"firefox"`). When `Some`, the bridge appends `--cookies-from-browser <name>`
    /// to the yt-dlp command line. Carried as a raw string (rather than a typed
    /// enum) so the bridge stays UI-agnostic — the `app` crate owns browser
    /// detection and converts to the yt-dlp arg string.
    pub cookies_browser: Option<String>,
    /// Optional path to a deno binary. When `Some`, the bridge appends
    /// `--js-runtimes deno:<path>` to the yt-dlp command line so yt-dlp's
    /// `YouTube` extractor can resolve signature challenges. When `None`,
    /// yt-dlp falls back to its own PATH lookup or prints its default
    /// "no JS runtime" warning.
    pub js_runtime_path: Option<PathBuf>,
    /// Optional path to a bundled `ffmpeg` binary (UC 17). When `Some`,
    /// the bridge appends `--ffmpeg-location <parent_dir>` to the yt-dlp
    /// command line so yt-dlp's DASH-merge / audio-extract postprocessors
    /// pick up the bundled binary instead of scanning `$PATH`.
    ///
    /// The directory form (parent dir of the binary) is used deliberately
    /// rather than the file form: a future ffprobe addition can be dropped
    /// next to ffmpeg without changing the argv builder.
    pub ffmpeg_path: Option<PathBuf>,
}

/// Stream of structured events emitted by an in-flight download.
///
/// The exact stream for a successful download is:
/// `Started` → zero or more `Progress` → optional `PostProcessing` → `Finished`.
/// On failure the stream ends with `Error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadEvent {
    /// The `yt-dlp` child process has been spawned successfully.
    Started,

    /// A progress update tick. Any field may be `None` when `yt-dlp` reports
    /// `NA` (typical for live streams or before total size is known).
    ///
    /// UC 08 widens this variant with the raw byte counts so downstream
    /// callers (the row delegate's `<downloaded> / <size>` mono line) don't
    /// have to back-derive them from `pct` alone.
    Progress {
        /// Completion percentage in `0.0..=100.0`, when both downloaded and
        /// total bytes are known.
        pct: Option<f32>,
        speed_bps: Option<u64>,
        eta_s: Option<u64>,
        /// Bytes downloaded so far.
        downloaded_bytes: Option<u64>,
        /// Total bytes once known. yt-dlp may report `NA` on live streams
        /// or before metadata resolution; bridge consumers should treat
        /// `None` as "size unknown".
        total_bytes: Option<u64>,
    },

    /// `yt-dlp` is post-processing (merging, transcoding, embedding metadata).
    PostProcessing,

    /// The download completed successfully.
    Finished {
        /// Final file path on disk, captured via `yt-dlp --print after_move:filepath`.
        /// `None` if the marker line was not observed (degraded but non-fatal).
        file_path: Option<PathBuf>,
        /// Final byte size, snapshotted from the last `Progress` event's
        /// `total_bytes`. `None` if no progress line ever carried a known
        /// total (e.g. live streams or extractors that don't expose size).
        bytes: Option<u64>,
    },

    /// The download failed. `message` carries the stderr tail or a synthetic
    /// reason; the typed error is also available on the `JoinHandle` returned
    /// by [`start`].
    Error { message: String },

    /// yt-dlp announced the chosen on-disk filename for the active download
    /// (matched against the `[download] Destination: <path>` stdout line).
    /// The app crate persists this path in `queue_items.partial_file_path`
    /// so a later Remove can delete the `.part` file from disk.
    ///
    /// Captured on stdout (yt-dlp's `screen` stream defaults to stdout when
    /// `--quiet` is not passed; the bridge does not pass `--quiet`).
    PartialFilePath { path: PathBuf },
}

/// Spawns a `yt-dlp` child for the given [`DownloadRequest`] and returns a
/// receiver of [`DownloadEvent`] plus a join handle for the supervisor task.
///
/// `cancel` is observed via [`tokio::sync::Notify::notified`]; on notify, the
/// child is killed (single-shot SIGKILL on Unix, `TerminateProcess` on
/// Windows) and the supervisor returns [`BridgeError::Cancelled`].
///
/// The receiver is dropped only when the supervisor returns. A consumer that
/// stops polling does not leak the child — the supervisor still drains stdio
/// and reaps the process.
///
/// # Errors
///
/// The returned [`JoinHandle`] resolves to:
/// - `Ok(())` on a clean exit.
/// - [`BridgeError::Spawn`] if the child cannot be created.
/// - [`BridgeError::ExitedWithError`] for non-zero exits with the captured
///   stderr tail.
/// - [`BridgeError::Cancelled`] when the supplied `cancel` was notified.
/// - [`BridgeError::Io`] for I/O surprises on the pipes.
#[must_use]
pub fn start(
    yt_dlp_path: &Path,
    req: DownloadRequest,
    cancel: Arc<Notify>,
) -> (mpsc::Receiver<DownloadEvent>, JoinHandle<Result<()>>) {
    let yt_dlp_path = yt_dlp_path.to_path_buf();
    let (tx, rx) = mpsc::channel(EVENT_CHANNEL_BUFFER);
    let handle = tokio::spawn(async move { run_download(yt_dlp_path, req, cancel, tx).await });
    (rx, handle)
}

#[allow(clippy::too_many_lines)]
async fn run_download(
    yt_dlp_path: PathBuf,
    req: DownloadRequest,
    cancel: Arc<Notify>,
    tx: mpsc::Sender<DownloadEvent>,
) -> Result<()> {
    let output_template = format!(
        "{}/%(title)s.%(ext)s",
        req.dest_dir.to_string_lossy().replace('\\', "/")
    );
    let progress_template = "yt-dlp-ui-progress %(progress.downloaded_bytes)d %(progress.total_bytes)d %(progress.speed)d %(progress.eta)d";
    let filepath_template = format!("after_move:{FILEPATH_MARKER}%(filepath)s");

    let mut cmd = Command::new(&yt_dlp_path);
    cmd.arg("--newline")
        .arg("--no-color")
        .arg("--progress-template")
        .arg(progress_template)
        .arg("--print")
        .arg(&filepath_template)
        .arg("-o")
        .arg(&output_template);
    for arg in req.format.to_yt_dlp_args() {
        cmd.arg(arg);
    }
    if let Some(browser) = req.cookies_browser.as_deref() {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    if let Some(deno) = req.js_runtime_path.as_deref() {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(ffmpeg) = req.ffmpeg_path.as_deref()
        && let Some(parent) = ffmpeg.parent()
    {
        // Directory form (not file form) so a future ffprobe dropped next
        // to ffmpeg gets picked up without an argv change.
        cmd.arg("--ffmpeg-location").arg(parent);
    }
    cmd.arg(&req.url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(BridgeError::Spawn)?;
    // Best-effort notification; if the consumer dropped the receiver we just
    // continue running so the child still drains and reaps cleanly.
    let _ = tx.send(DownloadEvent::Started).await;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BridgeError::Parse("child stdout missing".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BridgeError::Parse("child stderr missing".to_string()))?;

    let tx_stdout = tx.clone();
    // UC 08: snapshot the last-seen `total_bytes` so it can be propagated
    // verbatim into the `Finished { bytes }` variant. Tuple return so the
    // post-task `await` can pick both up without re-reading the channel.
    let stdout_task: JoinHandle<(Option<PathBuf>, Option<u64>)> = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut final_path: Option<PathBuf> = None;
        let mut last_total: Option<u64> = None;
        let mut saw_post_processing = false;
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(path_str) = line.strip_prefix(FILEPATH_MARKER) {
                final_path = Some(PathBuf::from(path_str));
                continue;
            }
            if let Some(path_str) = parse_destination_line(&line) {
                let _ = tx_stdout
                    .send(DownloadEvent::PartialFilePath {
                        path: PathBuf::from(path_str),
                    })
                    .await;
                continue;
            }
            if let Some(event) = parse_progress_line(&line) {
                if let DownloadEvent::Progress { total_bytes, .. } = &event
                    && let Some(t) = total_bytes
                {
                    last_total = Some(*t);
                }
                let _ = tx_stdout.send(event).await;
                continue;
            }
            // Detect post-processing once and forward; everything else
            // is dropped silently.
            if !saw_post_processing && is_post_processing_line(&line) {
                saw_post_processing = true;
                let _ = tx_stdout.send(DownloadEvent::PostProcessing).await;
            }
        }
        (final_path, last_total)
    });

    let stderr_task: JoinHandle<Vec<u8>> = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        while let Ok(Some(line)) = reader.next_line().await {
            for byte in line.as_bytes() {
                buf.push(*byte);
            }
            buf.push(b'\n');
            if buf.len() > STDERR_TAIL_BUDGET * 2 {
                let drop_to = buf.len() - STDERR_TAIL_BUDGET;
                buf.drain(..drop_to);
            }
        }
        buf
    });

    // Race the child's exit against the cancel token. On cancel, run the
    // two-stage termination (SIGTERM → 2 s grace → SIGKILL on Unix; immediate
    // TerminateProcess on Windows) so yt-dlp gets a chance to flush its
    // `.part` file cleanly before we force-kill it. Cross-platform behavior
    // is documented in `crate::cancel`.
    let exit_status = tokio::select! {
        status = child.wait() => status?,
        () = cancel.notified() => {
            terminate_with_grace(&mut child).await;
            let _ = tx.send(DownloadEvent::Error {
                message: "cancelled".to_string(),
            }).await;
            return Err(BridgeError::Cancelled);
        }
    };

    let (final_path, last_total) = stdout_task.await.unwrap_or((None, None));
    let stderr_buf = stderr_task.await.unwrap_or_default();

    if exit_status.success() {
        let _ = tx
            .send(DownloadEvent::Finished {
                file_path: final_path,
                bytes: last_total,
            })
            .await;
        Ok(())
    } else {
        let tail = stderr_tail(&stderr_buf);
        // Always send the Error UiEvent first so the UI's existing error
        // stream stays stable regardless of which typed error variant the
        // supervisor returns.
        let _ = tx
            .send(DownloadEvent::Error {
                message: tail.clone(),
            })
            .await;
        if is_bot_check_stderr(&tail) {
            return Err(BridgeError::AuthRequired { stderr_tail: tail });
        }
        Err(BridgeError::ExitedWithError {
            code: exit_status.code(),
            stderr_tail: tail,
        })
    }
}

/// Heuristic: `yt-dlp` reports post-processing with a `[ExtractAudio]`,
/// `[Merger]`, `[FixupM4a]`, etc. prefix. We collapse all of those into a
/// single `PostProcessing` event since the UI does not need to distinguish.
fn is_post_processing_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    matches!(trimmed.as_bytes().first(), Some(b'['))
        && (trimmed.contains("ExtractAudio")
            || trimmed.contains("Merger")
            || trimmed.contains("Fixup")
            || trimmed.contains("Metadata")
            || trimmed.contains("EmbedSubtitle")
            || trimmed.contains("VideoConvertor"))
}

fn stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_BUDGET);
    String::from_utf8_lossy(&stderr[start..]).to_string()
}
