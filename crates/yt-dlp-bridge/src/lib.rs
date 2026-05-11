//! yt-dlp-bridge — typed wrapper around the bundled `yt-dlp` standalone binary.
//!
//! This crate is deliberately UI-free. Its responsibilities (per
//! `PROJECT_BRIEF.md` § Architecture):
//! - Spawn `yt-dlp` via `tokio::process::Command`.
//! - Parse machine-readable progress (`--newline` + `--progress-template`) into
//!   structured events: `Started`, `Progress`, `PostProcessing`, `Finished`,
//!   `Error`.
//! - Provide cancellation primitives (graceful then forced kill).
//! - Map yt-dlp exit codes and stderr lines into typed error variants.
//!
//! The crate accepts the path to the `yt-dlp` binary as a constructor argument
//! so it can be unit-tested with a fake binary.

mod auth;
mod cancel;
pub mod download;
pub mod error;
pub mod format;
pub mod metadata;
pub mod parser;

pub use download::{DownloadEvent, DownloadRequest, start};
pub use error::{BridgeError, Result};
pub use format::FormatPref;
pub use metadata::{
    EnumerationOutcome, PlaylistEntry, VideoMetadata, enumerate_playlist_cancellable,
    expand_playlist, fetch_metadata, get_thumbnail_url, get_title, get_title_cancellable,
};
pub use parser::{parse_destination_line, parse_progress_line};

/// Returns the version of this crate, as recorded in `Cargo.toml`.
///
/// Used by the `app` crate to log a "bridge handshake" line at startup so
/// version mismatches between a release-built `app` and a stale `yt-dlp-bridge`
/// in a dev tree are obvious in the logs.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
