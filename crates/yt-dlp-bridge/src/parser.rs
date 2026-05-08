//! Pure parser for `yt-dlp`'s machine-readable progress lines.
//!
//! The bridge spawns `yt-dlp` with the following progress template:
//! `yt-dlp-ui-progress %(progress.downloaded_bytes)d %(progress.total_bytes)d %(progress.speed)d %(progress.eta)d`.
//!
//! Each emitted line therefore looks like:
//! `yt-dlp-ui-progress 1234567 4567890 123456 12`
//!
//! `yt-dlp` substitutes the literal string `NA` for unknown numeric fields, so
//! the parser has to tolerate that sentinel in any numeric slot.

use crate::download::DownloadEvent;

#[cfg(test)]
#[path = "parser_test.rs"]
mod parser_tests;

const PROGRESS_PREFIX: &str = "yt-dlp-ui-progress ";

/// Prefix yt-dlp emits on stdout when it has chosen the on-disk filename for
/// the active download (the `.part` file path lives at the same location with
/// a `.part` suffix). Captured by the bridge so the app can persist
/// `partial_file_path` and clean it up on Remove.
const DESTINATION_PREFIX: &str = "[download] Destination: ";

/// Strips the `[download] Destination: ` prefix from a yt-dlp stdout line and
/// returns the trailing path. Returns `None` for any line that does not match
/// the prefix.
///
/// The matched line format is stable across recent yt-dlp releases, but is
/// version-sensitive — a rewording upstream regresses the capture to `None`,
/// which leaks the `.part` file silently on Remove (documented in
/// `use-cases/02-cancel-remove-and-restart.md` § Pitfalls).
#[must_use]
pub fn parse_destination_line(line: &str) -> Option<&str> {
    line.strip_prefix(DESTINATION_PREFIX)
}

/// Outcome of parsing a single whitespace-separated progress field.
///
/// `Bad` lets the caller distinguish "the line is malformed, drop it" from
/// "the field is the `NA` sentinel" — both of which would otherwise collapse
/// into a single `None` and lose information.
enum FieldValue {
    Value(u64),
    NotAvailable,
    Bad,
}

/// Parses a single line of `yt-dlp` stdout into a [`DownloadEvent::Progress`].
///
/// Returns `None` for any line that does not match the expected progress
/// template — including blank lines, `[download]` lines, post-processing
/// status, and non-progress diagnostic output. Returning `None` is the
/// expected control-flow for the vast majority of stdout lines; the caller
/// drops them silently.
///
/// Does not allocate beyond what `Option<DownloadEvent>` requires; this is
/// hot-path code (one call per progress tick per active download).
#[must_use]
pub fn parse_progress_line(line: &str) -> Option<DownloadEvent> {
    let rest = line.strip_prefix(PROGRESS_PREFIX)?;

    let mut parts = rest.split_whitespace();
    let downloaded = parse_field(parts.next()?);
    let total = parse_field(parts.next()?);
    let speed = parse_field(parts.next()?);
    let eta = parse_field(parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    if matches!(downloaded, FieldValue::Bad)
        || matches!(total, FieldValue::Bad)
        || matches!(speed, FieldValue::Bad)
        || matches!(eta, FieldValue::Bad)
    {
        return None;
    }

    let downloaded_bytes = to_option(&downloaded);
    let total_bytes = to_option(&total);
    let pct = match (downloaded_bytes, total_bytes) {
        (Some(d), Some(t)) if t > 0 => Some(percent(d, t)),
        _ => None,
    };

    Some(DownloadEvent::Progress {
        pct,
        speed_bps: to_option(&speed),
        eta_s: to_option(&eta),
        downloaded_bytes,
        total_bytes,
    })
}

/// Parses one whitespace-separated field of the progress template.
fn parse_field(raw: &str) -> FieldValue {
    if raw == "NA" {
        return FieldValue::NotAvailable;
    }
    raw.parse::<u64>()
        .map_or(FieldValue::Bad, FieldValue::Value)
}

fn to_option(field: &FieldValue) -> Option<u64> {
    match field {
        FieldValue::Value(v) => Some(*v),
        FieldValue::NotAvailable | FieldValue::Bad => None,
    }
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn percent(downloaded: u64, total: u64) -> f32 {
    let raw = (downloaded as f64 / total as f64) * 100.0;
    raw.clamp(0.0, 100.0) as f32
}
