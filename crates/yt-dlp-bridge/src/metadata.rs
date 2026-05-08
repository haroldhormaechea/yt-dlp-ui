//! Title fetch and playlist expansion via `yt-dlp` subprocess invocations.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde::de::{self, Deserializer};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;
use tokio::time::timeout;

use crate::auth::is_bot_check_stderr;
use crate::cancel::terminate_with_grace;
use crate::error::{BridgeError, Result};

#[cfg(test)]
#[path = "metadata_test.rs"]
mod metadata_tests;

/// Maximum bytes of stderr retained for diagnostic purposes when a child
/// exits with a non-zero status.
const STDERR_TAIL_BUDGET: usize = 4096;

/// One entry in a playlist as returned by `yt-dlp --flat-playlist --dump-json`.
///
/// Only the fields used by the UI are deserialized; everything else in the
/// upstream JSON line is ignored to keep this struct stable across `yt-dlp`
/// versions.
#[derive(Debug, Clone)]
pub struct PlaylistEntry {
    /// The per-video URL. Sourced from `webpage_url` in `yt-dlp`'s output;
    /// falls back to `url` for extractors that do not populate `webpage_url`.
    /// Both fields present is accepted; `webpage_url` wins.
    pub url: String,

    /// The video title, if `yt-dlp` could resolve it during the flat-playlist
    /// pass. `None` is common for some extractors and means a follow-up
    /// `get_title` call will be required.
    pub title: Option<String>,

    /// Upstream thumbnail URL, if yt-dlp populated it during the
    /// flat-playlist pass (UC 08). Many extractors leave this `None` even
    /// with `--flat-playlist`; the row falls back to the gradient
    /// placeholder until a separate fetch succeeds.
    pub thumbnail: Option<String>,
}

/// Deserialization-only mirror of the subset of fields we read from
/// `yt-dlp`'s JSON output. Carries `url` and `webpage_url` independently so
/// documents containing both (the standard `YouTube` extractor shape) are
/// accepted; the canonical URL is resolved in [`PlaylistEntry`]'s manual
/// `Deserialize` impl.
///
/// `deny_unknown_fields` is intentionally NOT set: yt-dlp emits dozens of
/// unrelated fields per line, and silent ignore is required for stability
/// across upstream versions.
#[derive(Deserialize)]
struct RawPlaylistEntry {
    url: Option<String>,
    webpage_url: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
}

impl<'de> Deserialize<'de> for PlaylistEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawPlaylistEntry::deserialize(deserializer)?;
        let url = raw.webpage_url.or(raw.url).ok_or_else(|| {
            de::Error::custom("playlist entry missing both `webpage_url` and `url`")
        })?;
        Ok(PlaylistEntry {
            url,
            title: raw.title,
            thumbnail: raw.thumbnail,
        })
    }
}

/// Fetches the title of a single video URL.
///
/// Spawns `yt-dlp --skip-download --print %(title)s --no-playlist <url>` and
/// returns stdout (trimmed).
///
/// # Errors
///
/// Returns:
/// - [`BridgeError::Spawn`] if the child process cannot be created.
/// - [`BridgeError::ExitedWithError`] if the child exits with a non-zero
///   status, the timeout expires, or stdout was empty.
/// - [`BridgeError::Io`] for unexpected I/O failures while reading the child's
///   pipes.
pub async fn get_title(
    yt_dlp_path: &Path,
    url: &str,
    timeout_dur: Duration,
    cookies_browser: Option<&str>,
    js_runtime_path: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<String> {
    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("--skip-download")
        .arg("--print")
        .arg("%(title)s")
        .arg("--no-playlist");
    if let Some(browser) = cookies_browser {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    if let Some(deno) = js_runtime_path {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(ffmpeg) = ffmpeg_path
        && let Some(parent) = ffmpeg.parent()
    {
        cmd.arg("--ffmpeg-location").arg(parent);
    }
    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(BridgeError::Spawn)?;

    let output_fut = child.wait_with_output();
    let output = match timeout(timeout_dur, output_fut).await {
        Ok(res) => res?,
        Err(_) => {
            return Err(BridgeError::ExitedWithError {
                code: None,
                stderr_tail: format!("yt-dlp --print title timed out after {timeout_dur:?}"),
            });
        }
    };

    if !output.status.success() {
        let tail = stderr_tail(&output.stderr);
        if is_bot_check_stderr(&tail) {
            return Err(BridgeError::AuthRequired { stderr_tail: tail });
        }
        return Err(BridgeError::ExitedWithError {
            code: output.status.code(),
            stderr_tail: tail,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err(BridgeError::ExitedWithError {
            code: output.status.code(),
            stderr_tail: "yt-dlp --print title produced empty stdout".to_string(),
        });
    }
    Ok(stdout)
}

/// Cancellable variant of [`get_title`] (UC 02).
///
/// Spawns the same `yt-dlp --skip-download --print %(title)s --no-playlist`
/// subprocess as [`get_title`], but:
/// - pipes stdout/stderr manually instead of via `wait_with_output`, so the
///   child can be terminated without leaking pipes;
/// - races the child's exit against the supplied `cancel` notify and the
///   timeout, allowing `cancel_one` on a `title_status = fetching` row to
///   tear the subprocess down immediately;
/// - on cancel, runs [`terminate_with_grace`] (Unix: SIGTERM → 2 s grace →
///   SIGKILL; Windows: immediate `TerminateProcess`) and returns
///   [`BridgeError::Cancelled`].
///
/// Argument shape mirrors [`get_title`] so the manager can swap between them
/// without rewriting cookie / deno wiring.
///
/// **Thumbnail-fetch is NOT cancellable** — UC 02's acceptance criteria
/// scope cancellation to title fetches only. A future use case can extend
/// the same pattern to [`get_thumbnail_url`] if needed.
///
/// # Errors
///
/// - [`BridgeError::Spawn`] if the child cannot be created.
/// - [`BridgeError::ExitedWithError`] for non-zero exit, timeout, or empty
///   stdout (matching [`get_title`]'s contract).
/// - [`BridgeError::AuthRequired`] if stderr matches the `YouTube` bot-check
///   pattern.
/// - [`BridgeError::Cancelled`] when `cancel` was notified.
/// - [`BridgeError::Io`] for unexpected pipe failures.
pub async fn get_title_cancellable(
    yt_dlp_path: &Path,
    url: &str,
    timeout_dur: Duration,
    cookies_browser: Option<&str>,
    js_runtime_path: Option<&Path>,
    ffmpeg_path: Option<&Path>,
    cancel: Arc<Notify>,
) -> Result<String> {
    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("--skip-download")
        .arg("--print")
        .arg("%(title)s")
        .arg("--no-playlist");
    if let Some(browser) = cookies_browser {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    if let Some(deno) = js_runtime_path {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(ffmpeg) = ffmpeg_path
        && let Some(parent) = ffmpeg.parent()
    {
        cmd.arg("--ffmpeg-location").arg(parent);
    }
    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(BridgeError::Spawn)?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| BridgeError::Parse("child stdout missing".to_string()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| BridgeError::Parse("child stderr missing".to_string()))?;

    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(256);
        let _ = stdout.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(1024);
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let status = tokio::select! {
        res = child.wait() => res?,
        () = tokio::time::sleep(timeout_dur) => {
            terminate_with_grace(&mut child).await;
            return Err(BridgeError::ExitedWithError {
                code: None,
                stderr_tail: format!("yt-dlp --print title timed out after {timeout_dur:?}"),
            });
        }
        () = cancel.notified() => {
            terminate_with_grace(&mut child).await;
            return Err(BridgeError::Cancelled);
        }
    };

    let stdout_buf = stdout_task.await.unwrap_or_default();
    let stderr_buf = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let tail = stderr_tail(&stderr_buf);
        if is_bot_check_stderr(&tail) {
            return Err(BridgeError::AuthRequired { stderr_tail: tail });
        }
        return Err(BridgeError::ExitedWithError {
            code: status.code(),
            stderr_tail: tail,
        });
    }

    let title = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    if title.is_empty() {
        return Err(BridgeError::ExitedWithError {
            code: status.code(),
            stderr_tail: "yt-dlp --print title produced empty stdout".to_string(),
        });
    }
    Ok(title)
}

/// Fetches the upstream thumbnail URL for a single video URL (UC 08).
///
/// Spawns `yt-dlp --skip-download --print %(thumbnail)s --no-playlist <url>`
/// and returns stdout (trimmed). Mirrors [`get_title`] in argument shape so
/// the app crate can call both with the same cookie / deno wiring.
///
/// The returned string is suitable as input to a plain HTTPS GET — yt-dlp
/// resolves signed CDN URLs at print time. Note that some signed URLs
/// expire, which is why the app crate does not persist this URL across
/// restarts.
///
/// # Errors
///
/// Same shape as [`get_title`]: spawn failures, non-zero exit, timeout, or
/// empty stdout collapse into [`BridgeError::ExitedWithError`] /
/// [`BridgeError::AuthRequired`] / [`BridgeError::Spawn`].
pub async fn get_thumbnail_url(
    yt_dlp_path: &Path,
    url: &str,
    timeout_dur: Duration,
    cookies_browser: Option<&str>,
    js_runtime_path: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<String> {
    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("--skip-download")
        .arg("--print")
        .arg("%(thumbnail)s")
        .arg("--no-playlist");
    if let Some(browser) = cookies_browser {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    if let Some(deno) = js_runtime_path {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(ffmpeg) = ffmpeg_path
        && let Some(parent) = ffmpeg.parent()
    {
        cmd.arg("--ffmpeg-location").arg(parent);
    }
    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(BridgeError::Spawn)?;

    let output_fut = child.wait_with_output();
    let output = match timeout(timeout_dur, output_fut).await {
        Ok(res) => res?,
        Err(_) => {
            return Err(BridgeError::ExitedWithError {
                code: None,
                stderr_tail: format!("yt-dlp --print thumbnail timed out after {timeout_dur:?}"),
            });
        }
    };

    if !output.status.success() {
        let tail = stderr_tail(&output.stderr);
        if is_bot_check_stderr(&tail) {
            return Err(BridgeError::AuthRequired { stderr_tail: tail });
        }
        return Err(BridgeError::ExitedWithError {
            code: output.status.code(),
            stderr_tail: tail,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err(BridgeError::ExitedWithError {
            code: output.status.code(),
            stderr_tail: "yt-dlp --print thumbnail produced empty stdout".to_string(),
        });
    }
    Ok(stdout)
}

/// Expands a playlist URL into its constituent entries.
///
/// Spawns `yt-dlp --flat-playlist --dump-json <url>` and parses each line of
/// stdout as a [`PlaylistEntry`]. Returns an empty vec when the input behaves
/// like a single video — concretely, when only one JSON line is produced AND
/// that entry's `webpage_url` matches the input URL. Callers treat the empty
/// vec as "fall back to single-video handling and call [`get_title`] instead".
///
/// # Errors
///
/// Returns:
/// - [`BridgeError::Spawn`] if the child process cannot be created.
/// - [`BridgeError::ExitedWithError`] if the child exits with a non-zero status.
/// - [`BridgeError::Json`] if a JSON line cannot be deserialized into a
///   [`PlaylistEntry`].
/// - [`BridgeError::Io`] for unexpected I/O failures while reading the child's
///   pipes.
pub async fn expand_playlist(
    yt_dlp_path: &Path,
    url: &str,
    cookies_browser: Option<&str>,
    js_runtime_path: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<Vec<PlaylistEntry>> {
    let mut cmd = Command::new(yt_dlp_path);
    cmd.arg("--flat-playlist").arg("--dump-json");
    if let Some(browser) = cookies_browser {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    if let Some(deno) = js_runtime_path {
        cmd.arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    if let Some(ffmpeg) = ffmpeg_path
        && let Some(parent) = ffmpeg.parent()
    {
        cmd.arg("--ffmpeg-location").arg(parent);
    }
    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(BridgeError::Spawn)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BridgeError::Parse("child stdout missing".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BridgeError::Parse("child stderr missing".to_string()))?;

    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let stdout_task = tokio::spawn(async move {
        let mut entries: Vec<PlaylistEntry> = Vec::new();
        let mut lines = stdout_reader.lines();
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let entry: PlaylistEntry = serde_json::from_str(&line)?;
            entries.push(entry);
        }
        Ok::<Vec<PlaylistEntry>, BridgeError>(entries)
    });

    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(1024);
        let mut lines = stderr_reader.lines();
        while let Some(line) = lines.next_line().await.unwrap_or(None) {
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
        }
        buf
    });

    let status = child.wait().await?;
    let entries_res = stdout_task
        .await
        .map_err(|e| BridgeError::Parse(format!("stdout task join error: {e}")))?;
    let stderr_buf = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let tail = stderr_tail(&stderr_buf);
        if is_bot_check_stderr(&tail) {
            return Err(BridgeError::AuthRequired { stderr_tail: tail });
        }
        return Err(BridgeError::ExitedWithError {
            code: status.code(),
            stderr_tail: tail,
        });
    }

    let entries = entries_res?;
    if entries.len() == 1 && entries[0].url == url {
        return Ok(Vec::new());
    }
    Ok(entries)
}

/// Truncates the stderr buffer to the trailing [`STDERR_TAIL_BUDGET`] bytes
/// and returns it as a UTF-8 lossy string.
fn stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_BUDGET);
    String::from_utf8_lossy(&stderr[start..]).to_string()
}
