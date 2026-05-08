//! `SQLite`-backed persistence layer for the queue, settings, and history.
//!
//! The DB API is synchronous — `rusqlite` does not have a native async
//! interface and the queue volume is small enough that wrapping it in
//! `tokio::task::spawn_blocking` from the async sites is the right shape.
//! All public DAOs in this module take a `&Connection` so the caller owns
//! the lock-acquisition strategy.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use thiserror::Error;

pub mod migrations;
pub mod queue;
pub mod settings;

/// Errors returned by the DB layer.
#[derive(Debug, Error)]
pub enum DbError {
    /// A duplicate-key constraint fired (UNIQUE on `queue_items.url`).
    #[error("duplicate row: {0}")]
    Duplicate(String),

    /// Generic `rusqlite` failure not classified above.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// A row could not be decoded into the typed [`crate::model`] shape.
    /// Includes the offending column or field for debuggability.
    #[error("could not decode row: {0}")]
    Decode(String),

    /// JSON (de)serialization failure for fields stored as JSON text in the
    /// `format_pref` column or the settings KV.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result alias for DB operations.
pub type Result<T> = std::result::Result<T, DbError>;

/// Application-owned handle to the `SQLite` connection.
///
/// A single connection is used app-wide (single-process, single-user desktop
/// app — multiple connections would only complicate locking). The connection
/// is wrapped in an [`Arc`] + [`Mutex`] so callers from multiple async tasks
/// can serialize access cheaply.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (or creates) a `SQLite` database at `path`, applies pragmas, and
    /// runs all pending migrations.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] if the file cannot be opened, pragmas
    /// cannot be set, or migrations fail to apply.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        migrations::run_migrations(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` with the locked `SQLite` connection.
    ///
    /// Callers are expected to keep work inside `f` short — the lock is held
    /// for the duration. Long operations should be batched into a single
    /// closure rather than acquiring the lock repeatedly.
    ///
    /// # Errors
    ///
    /// Returns whatever `f` returns. `DbError::Sqlite(...)` if the mutex was
    /// poisoned by a panic in another thread.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| DbError::Sqlite(rusqlite::Error::InvalidQuery))?;
        f(&guard)
    }

    /// Like [`Self::with_conn`] but hands the closure a `&mut Connection`.
    /// Required by callers that begin a `rusqlite` transaction
    /// (e.g. `queue::delete_by_id`).
    ///
    /// # Errors
    ///
    /// Returns whatever `f` returns. `DbError::Sqlite(...)` if the mutex was
    /// poisoned by a panic in another thread.
    pub fn with_conn_mut<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R>) -> Result<R> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| DbError::Sqlite(rusqlite::Error::InvalidQuery))?;
        f(&mut guard)
    }
}
