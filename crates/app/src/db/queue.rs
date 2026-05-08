//! DAO for the `queue_items` table.

use std::path::Path;

use rusqlite::{Connection, ToSql, params};

use crate::model::{NewQueueItem, QueueItem, QueueStatus, TitleStatus};

use super::{DbError, Result};

#[cfg(test)]
#[path = "queue_test.rs"]
mod queue_tests;

/// Common SELECT projection. Centralized so that adding a column (e.g. UC 08
/// `thumbnail_path`, `size_bytes`, `downloaded_bytes`) is a one-line change.
const SELECT_COLS: &str = "id, url, title, title_status, title_error, status,
        progress_pct, speed_bps, eta_s, error_msg,
        format_pref, dest_dir, created_at, started_at, finished_at,
        thumbnail_path, size_bytes, downloaded_bytes, partial_file_path";

/// Inserts a new queue item, returning the auto-assigned row id.
///
/// Uses `created_at = CURRENT_TIMESTAMP`, `status = 'queued'`. A duplicate URL
/// is mapped to [`DbError::Duplicate`] for explicit handling at the caller.
///
/// # Errors
///
/// Returns [`DbError::Duplicate`] when the URL already exists in the queue,
/// [`DbError::Sqlite`] for any other DB failure, or [`DbError::Json`] if the
/// format pref cannot be serialized.
pub fn insert(conn: &Connection, item: NewQueueItem) -> Result<i64> {
    let format_pref = serde_json::to_string(&item.format_pref)?;
    let title_status = item.title_status.as_str();
    let dest = item.dest_dir.to_string_lossy().to_string();

    let sql = "INSERT INTO queue_items (
        url, title, title_status, status, format_pref, dest_dir, created_at
    ) VALUES (?, ?, ?, 'queued', ?, ?, CURRENT_TIMESTAMP)";

    let result = conn.execute(
        sql,
        params![&item.url, &item.title, title_status, format_pref, dest],
    );

    match result {
        Ok(_) => Ok(conn.last_insert_rowid()),
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            Err(DbError::Duplicate(item.url))
        }
        Err(err) => Err(DbError::Sqlite(err)),
    }
}

/// Returns every queue item, ordered by `created_at ASC` (FIFO).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure or [`DbError::Decode`] /
/// [`DbError::Json`] if a row cannot be parsed.
pub fn list_all(conn: &Connection) -> Result<Vec<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         ORDER BY created_at ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(decode_row(row)?);
    }
    Ok(items)
}

/// Looks up one item by URL.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on DB errors or [`DbError::Decode`] /
/// [`DbError::Json`] if the row cannot be parsed.
pub fn find_by_url(conn: &Connection, url: &str) -> Result<Option<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         WHERE url = ?
         LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([url])?;
    if let Some(row) = rows.next()? {
        Ok(Some(decode_row(row)?))
    } else {
        Ok(None)
    }
}

/// Looks up one item by id.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on DB errors or [`DbError::Decode`] /
/// [`DbError::Json`] if the row cannot be parsed.
pub fn find_by_url_by_id_internal(conn: &Connection, id: i64) -> Result<Option<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         WHERE id = ?
         LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(decode_row(row)?))
    } else {
        Ok(None)
    }
}

/// Updates the title and title-fetch status for one row.
///
/// `title_error` is cleared when `status = ok`; preserved otherwise so the
/// caller can store an error message via a separate path if needed.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn update_title(
    conn: &Connection,
    id: i64,
    title: Option<&str>,
    status: TitleStatus,
) -> Result<()> {
    let sql = if matches!(status, TitleStatus::Ok) {
        "UPDATE queue_items SET title = ?, title_status = ?, title_error = NULL WHERE id = ?"
    } else {
        "UPDATE queue_items SET title = ?, title_status = ? WHERE id = ?"
    };
    conn.execute(sql, params![title, status.as_str(), id])?;
    Ok(())
}

/// Records a title-fetch error on a row. Sets `title_status = error` and
/// stores `error` in `title_error`.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_title_error(conn: &Connection, id: i64, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE queue_items SET title_status = 'error', title_error = ? WHERE id = ?",
        params![error, id],
    )?;
    Ok(())
}

/// Updates the lifecycle status (and `started_at` / `finished_at`
/// timestamps for the relevant transitions).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn update_status(conn: &Connection, id: i64, status: QueueStatus) -> Result<()> {
    let sql = match status {
        QueueStatus::InFlight => {
            "UPDATE queue_items SET status = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP) WHERE id = ?"
        }
        QueueStatus::Done | QueueStatus::Error | QueueStatus::Cancelled => {
            "UPDATE queue_items SET status = ?, finished_at = CURRENT_TIMESTAMP WHERE id = ?"
        }
        _ => "UPDATE queue_items SET status = ? WHERE id = ?",
    };
    conn.execute(sql, params![status.as_str(), id])?;
    Ok(())
}

/// Records an error message on a row (without changing its status).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_error_msg(conn: &Connection, id: i64, message: &str) -> Result<()> {
    conn.execute(
        "UPDATE queue_items SET error_msg = ? WHERE id = ?",
        params![message, id],
    )?;
    Ok(())
}

/// Updates the progress fields on a row.
///
/// UC 08 widens this signature so the bridge's per-tick byte counts persist
/// alongside `pct` / `speed` / `eta`. Caller passes both the previously
/// derived `pct` and the raw byte counts; the row delegate uses the bytes
/// to render the `<downloaded> / <size>` mono line.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn update_progress(
    conn: &Connection,
    id: i64,
    pct: Option<f32>,
    speed: Option<u64>,
    eta: Option<u64>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
) -> Result<()> {
    let pct_param: Option<f64> = pct.map(f64::from);
    let speed_param: Option<i64> = speed.and_then(|v| i64::try_from(v).ok());
    let eta_param: Option<i64> = eta.and_then(|v| i64::try_from(v).ok());
    let downloaded_param: Option<i64> = downloaded_bytes.and_then(|v| i64::try_from(v).ok());
    let total_param: Option<i64> = total_bytes.and_then(|v| i64::try_from(v).ok());

    // Use COALESCE on size_bytes so a later `Progress` event reporting `NA`
    // for total does not overwrite a previously-known value.
    conn.execute(
        "UPDATE queue_items
         SET progress_pct = ?,
             speed_bps = ?,
             eta_s = ?,
             downloaded_bytes = ?,
             size_bytes = COALESCE(?, size_bytes)
         WHERE id = ?",
        params![
            &pct_param as &dyn ToSql,
            &speed_param as &dyn ToSql,
            &eta_param as &dyn ToSql,
            &downloaded_param as &dyn ToSql,
            &total_param as &dyn ToSql,
            id
        ],
    )?;
    Ok(())
}

/// Marks a row done, optionally stamping `size_bytes` (from the bridge's
/// `Finished { bytes }` propagation, UC 08 § Bridge widening) and snapshotting
/// `downloaded_bytes = size_bytes` so the done-state mono line reads `100%`.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_finished(conn: &Connection, id: i64, size_bytes: Option<u64>) -> Result<()> {
    let size_param: Option<i64> = size_bytes.and_then(|v| i64::try_from(v).ok());
    conn.execute(
        "UPDATE queue_items
         SET status = 'done',
             finished_at = CURRENT_TIMESTAMP,
             size_bytes = COALESCE(?, size_bytes),
             downloaded_bytes = COALESCE(?, downloaded_bytes)
         WHERE id = ?",
        params![&size_param as &dyn ToSql, &size_param as &dyn ToSql, id],
    )?;
    Ok(())
}

/// Persists the on-disk thumbnail cache path for a row.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_thumbnail_path(conn: &Connection, id: i64, path: &Path) -> Result<()> {
    let s = path.to_string_lossy().to_string();
    conn.execute(
        "UPDATE queue_items SET thumbnail_path = ? WHERE id = ?",
        params![s, id],
    )?;
    Ok(())
}

/// Persists the on-disk path of yt-dlp's `.part` file for a row (UC 02).
/// Captured by the bridge from the `[download] Destination: <path>` stdout
/// line; consumed by [`delete_by_id`]'s on-disk cleanup.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn update_partial_path(conn: &Connection, id: i64, path: &Path) -> Result<()> {
    let s = path.to_string_lossy().to_string();
    conn.execute(
        "UPDATE queue_items SET partial_file_path = ? WHERE id = ?",
        params![s, id],
    )?;
    Ok(())
}

/// Persists the resolved destination directory for an `in_flight` row (UC 16).
///
/// Mirrors [`update_partial_path`]'s shape but is gated on `status = 'in_flight'`
/// so a row that races to `cancelled` (e.g. user clicks Cancel between the
/// supervisor's promotion and its destination resolve) does not get its
/// `dest_dir` rewritten under it. The gating mirrors [`try_promote_to_in_flight`]'s
/// posture: zero rows updated when the row is no longer in-flight is a benign
/// no-op, not an error.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn update_dest_dir(conn: &Connection, id: i64, dir: &Path) -> Result<()> {
    let s = dir.to_string_lossy().to_string();
    conn.execute(
        "UPDATE queue_items SET dest_dir = ? WHERE id = ? AND status = 'in_flight'",
        params![s, id],
    )?;
    Ok(())
}

/// Deletes a queue row and its history rows transactionally (UC 02 Remove).
///
/// `history` carries an `ON DELETE` constraint via the application-level FK
/// (the schema enables `PRAGMA foreign_keys = ON` at connection init), but
/// the explicit `DELETE FROM history` is kept here so the operation is
/// self-contained and not implicitly dependent on PRAGMA state.
///
/// Returns the number of `queue_items` rows deleted (0 or 1).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn delete_by_id(conn: &mut Connection, id: i64) -> Result<usize> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM history WHERE queue_item_id = ?", [id])?;
    let n = tx.execute("DELETE FROM queue_items WHERE id = ?", [id])?;
    tx.commit()?;
    Ok(n)
}

/// Resets a cancelled row back to `queued` so the queue runner picks it up
/// for a Restart-driven resume (UC 02). `size_bytes` and `partial_file_path`
/// are deliberately preserved — yt-dlp's `--continue` reads from the
/// existing `.part` file at the row's snapshotted `dest_dir` to skip
/// already-downloaded bytes.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn clear_for_restart(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE queue_items
         SET status = 'queued',
             progress_pct = NULL,
             speed_bps = NULL,
             eta_s = NULL,
             downloaded_bytes = NULL,
             started_at = NULL,
             finished_at = NULL,
             error_msg = NULL
         WHERE id = ?",
        [id],
    )?;
    Ok(())
}

/// Atomically promotes a row from `queued` to `in_flight` (UC 02). Returns
/// `true` only when the row was actually advanced — `false` means the row
/// is already in some other state (typically `cancelled`, set by a
/// concurrent `cancel_one` racing the supervisor's first DB write).
///
/// The supervisor calls this before spawning the yt-dlp child and aborts
/// when it returns `false`, preventing the race where Cancel-on-queued
/// transitions the row to `cancelled` but the supervisor then overwrites it
/// back to `in_flight` and starts a download anyway.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn try_promote_to_in_flight(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn.execute(
        "UPDATE queue_items
         SET status = 'in_flight',
             started_at = COALESCE(started_at, CURRENT_TIMESTAMP)
         WHERE id = ? AND status = 'queued'",
        [id],
    )?;
    Ok(n == 1)
}

/// Reverts every `in_flight` row back to `queued` and zeroes its progress
/// fields, so a clean restart picks them up from a known state.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn revert_in_flight_to_queued(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "UPDATE queue_items
         SET status = 'queued',
             progress_pct = NULL,
             speed_bps = NULL,
             eta_s = NULL,
             started_at = NULL
         WHERE status = 'in_flight'",
        [],
    )?;
    Ok(n)
}

/// Returns every row currently in `queued` status, oldest-first.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn list_queued(conn: &Connection) -> Result<Vec<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         WHERE status = 'queued'
         ORDER BY created_at ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(decode_row(row)?);
    }
    Ok(out)
}

/// Returns rows whose title fetch is still in `pending` or `fetching` state.
/// Used at startup to re-issue title fetches that did not complete before
/// the previous shutdown.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn list_titles_to_fetch(conn: &Connection) -> Result<Vec<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         WHERE title_status IN ('pending', 'fetching')"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(decode_row(row)?);
    }
    Ok(out)
}

/// Returns rows whose thumbnail has not yet been fetched. Used at startup
/// (UC 08) to re-issue per-row background fetches that did not complete
/// before the previous shutdown.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn list_pending_thumbnail_fetches(conn: &Connection) -> Result<Vec<QueueItem>> {
    let sql = format!(
        "SELECT {SELECT_COLS}
         FROM queue_items
         WHERE thumbnail_path IS NULL"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(decode_row(row)?);
    }
    Ok(out)
}

fn decode_row(row: &rusqlite::Row<'_>) -> Result<QueueItem> {
    let id: i64 = row.get(0)?;
    let url: String = row.get(1)?;
    let title: Option<String> = row.get(2)?;
    let title_status_raw: String = row.get(3)?;
    let title_error: Option<String> = row.get(4)?;
    let status_raw: String = row.get(5)?;
    let progress_pct: Option<f64> = row.get(6)?;
    let speed_bps: Option<i64> = row.get(7)?;
    let eta_s: Option<i64> = row.get(8)?;
    let error_msg: Option<String> = row.get(9)?;
    let format_pref_raw: String = row.get(10)?;
    let dest_dir: String = row.get(11)?;
    let created_at: String = row.get(12)?;
    let started_at: Option<String> = row.get(13)?;
    let finished_at: Option<String> = row.get(14)?;
    let thumbnail_path: Option<String> = row.get(15)?;
    let size_bytes: Option<i64> = row.get(16)?;
    let downloaded_bytes: Option<i64> = row.get(17)?;
    let partial_file_path: Option<String> = row.get(18)?;

    let title_status = TitleStatus::parse(&title_status_raw)
        .map_err(|s| DbError::Decode(format!("title_status={s}")))?;
    let status =
        QueueStatus::parse(&status_raw).map_err(|s| DbError::Decode(format!("status={s}")))?;
    let format_pref = serde_json::from_str(&format_pref_raw)?;

    Ok(QueueItem {
        id,
        url,
        title,
        title_status,
        title_error,
        status,
        #[allow(clippy::cast_possible_truncation)]
        progress_pct: progress_pct.map(|v| v as f32),
        speed_bps: speed_bps.and_then(|v| u64::try_from(v).ok()),
        eta_s: eta_s.and_then(|v| u64::try_from(v).ok()),
        error_msg,
        format_pref,
        dest_dir: dest_dir.into(),
        created_at,
        started_at,
        finished_at,
        thumbnail_path: thumbnail_path.map(std::path::PathBuf::from),
        size_bytes: size_bytes.and_then(|v| u64::try_from(v).ok()),
        downloaded_bytes: downloaded_bytes.and_then(|v| u64::try_from(v).ok()),
        partial_file_path: partial_file_path.map(std::path::PathBuf::from),
    })
}
