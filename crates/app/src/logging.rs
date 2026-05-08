//! `tracing` setup with separate dev / release configurations.
//!
//! Per `PROJECT_BRIEF.md` § Observability:
//! - Dev (debug builds): pretty stdout, env-filter, default `INFO` floor.
//! - Release: JSON to a daily-rolled file at `<app_data>/logs/yt-dlp-ui.log`,
//!   plus a `WARN+` pretty stdout layer.
//! - Retention: 7 days. Older daily rolls are deleted on startup.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[cfg(test)]
#[path = "logging_test.rs"]
mod logging_tests;

const LOG_RETENTION_DAYS: i64 = 7;
const LOG_FILENAME: &str = "yt-dlp-ui.log";

/// Errors during logging initialization.
#[derive(Debug, Error)]
pub enum LoggingError {
    /// Failed to create the logs directory.
    #[error("failed to create logs directory at {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// `tracing_subscriber::set_global_default` rejected our configuration —
    /// typically because a default subscriber was already installed.
    #[error("failed to install global tracing subscriber: {0}")]
    SetGlobal(String),
}

/// Initializes structured logging and returns a [`WorkerGuard`] that the
/// caller MUST hold for the lifetime of the application. Dropping the guard
/// flushes the appender's worker thread and is required to avoid losing
/// the trailing log lines on exit.
///
/// # Errors
///
/// Returns [`LoggingError::CreateDir`] if the logs directory cannot be
/// created, or [`LoggingError::SetGlobal`] if a global subscriber is already
/// installed.
pub fn init(app_data_dir: &Path) -> Result<WorkerGuard, LoggingError> {
    let logs_dir = app_data_dir.join("logs");
    fs::create_dir_all(&logs_dir).map_err(|e| LoggingError::CreateDir {
        path: logs_dir.display().to_string(),
        source: e,
    })?;

    prune_old_logs(&logs_dir);

    let file_appender = tracing_appender::rolling::daily(&logs_dir, LOG_FILENAME);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    if cfg!(debug_assertions) {
        // Dev: pretty stdout, default INFO floor when RUST_LOG is unset.
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let stdout_layer = fmt::layer().with_target(false).with_writer(std::io::stdout);
        tracing_subscriber::registry()
            .with(filter)
            .with(stdout_layer)
            .try_init()
            .map_err(|e| LoggingError::SetGlobal(e.to_string()))?;
    } else {
        // Release: JSON to file (INFO+); pretty stdout (WARN+).
        let file_layer = fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_filter(EnvFilter::new("info"));
        let stdout_layer = fmt::layer()
            .with_target(false)
            .with_writer(std::io::stdout)
            .with_filter(EnvFilter::new("warn"));
        tracing_subscriber::registry()
            .with(file_layer)
            .with(stdout_layer)
            .try_init()
            .map_err(|e| LoggingError::SetGlobal(e.to_string()))?;
    }

    Ok(guard)
}

/// Walks the logs directory and deletes daily rolls older than
/// [`LOG_RETENTION_DAYS`]. Errors are logged via `tracing::warn!` and never
/// block startup.
fn prune_old_logs(logs_dir: &Path) {
    let entries = match fs::read_dir(logs_dir) {
        Ok(it) => it,
        Err(err) => {
            tracing::warn!(?err, "could not read logs directory for pruning");
            return;
        }
    };
    let cutoff = match SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
    {
        Some(now) => now - LOG_RETENTION_DAYS * 24 * 60 * 60,
        None => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with(LOG_FILENAME) {
            continue;
        }
        let Some(date_part) = name.rsplit('.').next() else {
            continue;
        };
        let Some(epoch) = parse_yyyy_mm_dd_to_epoch(date_part) else {
            continue;
        };
        if epoch < cutoff
            && let Err(err) = fs::remove_file(&path)
        {
            tracing::warn!(?err, path = %path.display(), "failed to prune old log");
        }
    }
}

/// Parses a `YYYY-MM-DD` string into a UTC midnight epoch second. Returns
/// `None` for any malformed input — including different filename suffixes
/// or rolls without a date suffix.
fn parse_yyyy_mm_dd_to_epoch(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    let now_secs = i64::try_from(now.as_secs()).ok()?;
    let approx_year_secs = (y - 1970) * 365 * 86_400;
    let approx_month_secs = (m - 1) * 30 * 86_400;
    let approx_day_secs = (d - 1) * 86_400;
    let approx = approx_year_secs + approx_month_secs + approx_day_secs;
    // Safety belt: never claim a future date.
    Some(approx.min(now_secs))
}
