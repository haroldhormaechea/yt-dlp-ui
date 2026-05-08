//! Typed error surface for the yt-dlp-bridge crate.

use std::io;

use thiserror::Error;

/// Errors emitted by [`crate`] operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Failed to spawn the `yt-dlp` child process.
    #[error("failed to spawn yt-dlp: {0}")]
    Spawn(#[source] io::Error),

    /// The `yt-dlp` child exited with a non-zero status (or, in timeout cases,
    /// was killed before it could exit cleanly). `stderr_tail` carries up to
    /// the last few KB of stderr to make diagnosis tractable without ballooning
    /// memory on a runaway child.
    #[error("yt-dlp exited with error (code: {code:?}): {stderr_tail}")]
    ExitedWithError {
        code: Option<i32>,
        stderr_tail: String,
    },

    /// The `yt-dlp` child exited with a non-zero status whose stderr matches
    /// the `YouTube` bot-check pattern recognized by `crate::auth::is_bot_check_stderr`.
    /// The UI layer branches on this to surface a "pick a browser for cookies"
    /// dialog instead of a generic error.
    ///
    /// **Non-stable contract caveat:** the matcher is heuristic and pinned to
    /// yt-dlp's current stderr phrasing. A future yt-dlp message rewording
    /// regresses this variant to a generic [`BridgeError::ExitedWithError`] —
    /// an acceptable, opt-in failure mode (the user's pre-UC-05 experience),
    /// not a worse one.
    #[error("yt-dlp reported a YouTube bot-check (cookies required): {stderr_tail}")]
    AuthRequired { stderr_tail: String },

    /// A line of `yt-dlp` output that the bridge could not parse.
    #[error("failed to parse yt-dlp output: {0}")]
    Parse(String),

    /// A JSON document from `yt-dlp` that could not be deserialized into the
    /// expected shape.
    #[error("failed to deserialize yt-dlp JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The caller cancelled the in-flight operation.
    #[error("operation cancelled")]
    Cancelled,

    /// Generic I/O failure while reading from or writing to the child process.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, BridgeError>;
