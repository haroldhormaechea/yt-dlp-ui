//! Top-level error type for the `app` crate's [`crate::run`] entry point.

use thiserror::Error;

use crate::db::DbError;
use crate::logging::LoggingError;
use crate::paths::PathError;

/// Anything `run()` can fail with at startup. Once the event loop is
/// running, errors are kept inside their respective subsystems and
/// surfaced via the UI / logs rather than bubbling up here.
#[derive(Debug, Error)]
pub enum AppError {
    /// Path resolution failed (no app-data dir, no Downloads dir, no bundled
    /// `yt-dlp`).
    #[error(transparent)]
    Paths(#[from] PathError),

    /// Database initialization or migration failed.
    #[error(transparent)]
    Db(#[from] DbError),

    /// Logging subsystem failed to initialize.
    #[error(transparent)]
    Logging(#[from] LoggingError),

    /// Filesystem I/O failed (e.g. creating the app-data directory).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The Slint platform layer returned an error during window setup or run.
    #[error("ui error: {0}")]
    Ui(String),

    /// Tokio runtime construction failed.
    #[error("tokio runtime error: {0}")]
    Runtime(String),
}
