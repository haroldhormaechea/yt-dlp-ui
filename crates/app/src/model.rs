//! Domain types for the queue, settings, and UI rows.
//!
//! These types are pure data — no I/O, no async, no DB handles. The DB
//! layer (`crate::db`) maps rows to/from these structs; the UI layer
//! (`crate::ui_bridge`) projects them to Slint-friendly shapes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use yt_dlp_bridge::FormatPref;

#[cfg(test)]
#[path = "model_test.rs"]
mod model_tests;

/// Lifecycle state of a queue item.
///
/// Kept in sync with the SQL CHECK string column in the `queue_items` table:
/// `'queued' | 'in_flight' | 'paused' | 'cancelling' | 'cancelled' | 'done' | 'error'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Queued,
    InFlight,
    Paused,
    /// UC 02 transient: Cancel was requested on an `in_flight` row but the
    /// bridge has not yet confirmed the subprocess is dead. The UI shows
    /// the row's Cancel/Remove/Restart buttons disabled while in this
    /// state to prevent double-cancel races.
    Cancelling,
    Cancelled,
    Done,
    Error,
}

impl QueueStatus {
    /// Returns the snake-case string used in the `SQLite` `status` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InFlight => "in_flight",
            Self::Paused => "paused",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Done => "done",
            Self::Error => "error",
        }
    }

    /// Parses a snake-case status string into a [`QueueStatus`]. Unknown
    /// values are reported as an error to the caller.
    ///
    /// # Errors
    ///
    /// Returns the offending string if it does not match a known variant.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "queued" => Ok(Self::Queued),
            "in_flight" => Ok(Self::InFlight),
            "paused" => Ok(Self::Paused),
            "cancelling" => Ok(Self::Cancelling),
            "cancelled" => Ok(Self::Cancelled),
            "done" => Ok(Self::Done),
            "error" => Ok(Self::Error),
            other => Err(other.to_string()),
        }
    }
}

/// Lifecycle state of a per-row title fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleStatus {
    /// No fetch has been attempted yet.
    Pending,
    /// A title fetch is currently in flight.
    Fetching,
    /// The title was fetched successfully.
    Ok,
    /// The title fetch failed; the row carries the error message.
    Error,
}

impl TitleStatus {
    /// Returns the snake-case string used in the `SQLite` `title_status` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fetching => "fetching",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }

    /// Parses a snake-case status string into a [`TitleStatus`].
    ///
    /// # Errors
    ///
    /// Returns the offending string if it does not match a known variant.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "pending" => Ok(Self::Pending),
            "fetching" => Ok(Self::Fetching),
            "ok" => Ok(Self::Ok),
            "error" => Ok(Self::Error),
            other => Err(other.to_string()),
        }
    }
}

/// UC 27 discriminator. `Video` is the historical row kind — fully-known
/// (or known-enough) video row. `Pending` is an optimistic placeholder
/// inserted before enumeration resolves; the queue runner refuses to
/// auto-promote it to `in_flight` even if `status = 'queued'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceholderKind {
    Video,
    Pending,
}

impl PlaceholderKind {
    /// Returns the snake-case string used in the `SQLite` `kind` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Pending => "pending",
        }
    }

    /// Parses a snake-case kind string into a [`PlaceholderKind`].
    ///
    /// # Errors
    ///
    /// Returns the offending string if it does not match a known variant.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "video" => Ok(Self::Video),
            "pending" => Ok(Self::Pending),
            other => Err(other.to_string()),
        }
    }
}

/// Settings snapshot taken at queue add-time. Stored on each row so that
/// later changes to the global Settings do not retroactively affect items
/// already in the queue (per UC 01 § Pitfalls — Edge case).
#[derive(Debug, Clone)]
pub struct NewQueueItem {
    pub url: String,
    pub title: Option<String>,
    pub title_status: TitleStatus,
    pub format_pref: FormatPref,
    pub dest_dir: PathBuf,
    /// UC 27. Defaults to `Video` for existing call sites; `add_url` sets
    /// `Pending` for optimistic placeholder inserts.
    pub kind: PlaceholderKind,
    /// UC 27. Per-process monotonically-increasing sort key, allocated by
    /// the download manager from an `AtomicU64` seeded at startup.
    pub display_order: i64,
}

/// One row of the persistent queue.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub title_status: TitleStatus,
    pub title_error: Option<String>,
    pub status: QueueStatus,
    pub progress_pct: Option<f32>,
    pub speed_bps: Option<u64>,
    pub eta_s: Option<u64>,
    pub error_msg: Option<String>,
    pub format_pref: FormatPref,
    pub dest_dir: PathBuf,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// Path to the cached thumbnail (UC 08). `None` until the per-row
    /// background fetcher succeeds.
    pub thumbnail_path: Option<PathBuf>,
    /// Total file size in bytes once known (set by the bridge `Progress`
    /// or `Finished` event; UC 08).
    pub size_bytes: Option<u64>,
    /// Bytes downloaded so far (set by the bridge `Progress` event; UC 08).
    pub downloaded_bytes: Option<u64>,
    /// On-disk path of yt-dlp's `.part` file for the active download
    /// (UC 02). Captured by the bridge from the `[download] Destination:`
    /// stdout line; consumed on Remove to delete the partial file from
    /// disk. `None` until the bridge has emitted a `PartialFilePath` event.
    pub partial_file_path: Option<PathBuf>,
    /// UC 27 row discriminator. `Pending` rows are skeleton placeholders
    /// awaiting enumeration; they are never auto-promoted by the queue
    /// runner.
    pub kind: PlaceholderKind,
    /// UC 27. Latched "user clicked Start while the placeholder was still
    /// resolving" intent. Reset to false on promote / replace.
    pub start_requested: bool,
    /// UC 27 sort key. See [`NewQueueItem::display_order`].
    pub display_order: i64,
}

/// Global app settings (read from the `settings` KV table).
#[derive(Debug, Clone)]
pub struct Settings {
    pub concurrency_cap: u32,
    pub format_pref: FormatPref,
    pub dest_dir: PathBuf,
}

/// Slint-friendly projection of a [`QueueItem`]. Fields are scalar string /
/// numeric types so the Slint model can consume them directly.
#[derive(Debug, Clone)]
pub struct UiQueueRow {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub title_status: TitleStatus,
    pub title_error: Option<String>,
    pub status: QueueStatus,
    /// 0..=100; equal to 0 when no progress is known yet.
    pub progress_pct: f32,
    pub speed_bps: Option<u64>,
    pub eta_s: Option<u64>,
    pub error_msg: Option<String>,
    /// Snapshotted destination directory; the Slint row uses this to render
    /// the "saved to <path>" mono line on done rows.
    pub dest_dir: PathBuf,
    /// Final byte size if known. UC 08 widens the bridge `Progress` event to
    /// carry `total_bytes`, which `download_mgr` snapshots into the DB and
    /// projects here.
    pub size_bytes: Option<u64>,
    /// Bytes downloaded so far (last `Progress` event). Persists at terminal
    /// status so the cancelled-row mono line still reads correctly.
    pub downloaded_bytes: Option<u64>,
    /// Cached thumbnail path on disk, set by the per-row background
    /// fetcher. `None` until the fetcher succeeds — the row renders the
    /// gradient placeholder until then.
    pub thumbnail_path: Option<PathBuf>,
    /// UC 27 row discriminator projected into the UI layer so the Slint
    /// row template can branch on placeholder vs. video.
    pub kind: PlaceholderKind,
    /// UC 27. Latched "Start clicked on pending row" intent — disables the
    /// Download button and swaps it for "Starting…".
    pub start_requested: bool,
    /// UC 27 sort key, mirrored into the UI so the queue's order matches
    /// the DB's `ORDER BY display_order`.
    pub display_order: i64,
    /// UC 27. Unix epoch ms when the placeholder row was inserted (creation
    /// time for `Pending` rows; for `Video` rows it's the row's creation
    /// time too). Used by the 5-second "Still fetching info…" affordance
    /// in `queue_row.slint` against a Slint global `current-now-ms`.
    pub created_at_unix_ms: i64,
}

/// Splits a multi-line URL paste into trimmed, non-empty entries.
///
/// Used by the Add button to emulate an N-add operation when the user pastes
/// several URLs at once. Whitespace-only lines are dropped; surrounding
/// whitespace is trimmed but inner characters are preserved.
#[must_use]
pub fn split_pasted_urls(raw: &str) -> Vec<String> {
    raw.split('\n')
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}
