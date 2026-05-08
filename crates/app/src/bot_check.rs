//! Multi-row bot-check dialog coordination.
//!
//! When yt-dlp returns [`yt_dlp_bridge::BridgeError::AuthRequired`] for a row,
//! the supervisor calls [`BotCheckCoordinator::report_auth_required`] and
//! awaits a [`RetryDecision`] over a oneshot channel. The coordinator
//! deduplicates concurrent reports across rows: only the first report opens
//! the dialog; subsequent reports register their oneshot and wait. When the
//! user picks (or cancels), the coordinator drains every registered oneshot
//! and broadcasts the same decision to all of them — so a "pick chrome" with
//! "remember" applied at the moment N rows were waiting retries all of them
//! atomically with one cookies pick.
//!
//! Threading: callers from the tokio supervisor (`download_mgr`) and from the
//! Slint event loop bridge (`ui_bridge`) coexist on this single `Mutex`, but
//! the methods are short and never `await` while holding the mutex — they
//! drain oneshots into a local Vec first and `send` afterward.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};

use crate::browsers::Browser;
use crate::db::{Db, DbError, settings};

#[cfg(test)]
#[path = "bot_check_test.rs"]
mod bot_check_tests;

/// Coordinator outcome for a single `report_auth_required` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorOutcome {
    /// This is the first row to hit the bot-check while no dialog is open;
    /// the caller should cause the dialog to be shown.
    OpenDialog,
    /// A dialog is already open from an earlier row; the caller should keep
    /// quiet — the user's pending pick will resolve this row too.
    Append,
}

/// Decision delivered back to a per-row supervisor over the oneshot.
#[derive(Debug)]
pub enum RetryDecision {
    /// Retry the row with `--cookies-from-browser <browser>`. The string is
    /// already the yt-dlp arg form (e.g. `"chrome"`).
    PickedBrowser(String),
    /// User cancelled the dialog; the row should error out.
    Cancelled,
}

struct BotCheckState {
    pending: HashMap<i64, oneshot::Sender<RetryDecision>>,
    dialog_open: bool,
}

/// Cheap-to-clone handle to the shared coordinator state.
#[derive(Clone)]
pub struct BotCheckCoordinator {
    inner: Arc<Mutex<BotCheckState>>,
}

impl Default for BotCheckCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl BotCheckCoordinator {
    /// Builds a fresh coordinator with no rows pending and no dialog open.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BotCheckState {
                pending: HashMap::new(),
                dialog_open: false,
            })),
        }
    }

    /// Registers `row_id` as waiting on a user decision and returns whether
    /// the caller should cause a new dialog to open.
    pub async fn report_auth_required(
        &self,
        row_id: i64,
        retry_tx: oneshot::Sender<RetryDecision>,
    ) -> CoordinatorOutcome {
        let mut state = self.inner.lock().await;
        state.pending.insert(row_id, retry_tx);
        if state.dialog_open {
            CoordinatorOutcome::Append
        } else {
            state.dialog_open = true;
            CoordinatorOutcome::OpenDialog
        }
    }

    /// Persists the choice (when `remember` is true), then broadcasts
    /// [`RetryDecision::PickedBrowser`] to every registered oneshot. Returns
    /// the row ids that were notified (drained from the registry).
    ///
    /// # Errors
    ///
    /// Returns the underlying [`DbError`] if persisting `cookies_browser`
    /// fails. The drained oneshots are NOT notified in that case — the
    /// caller is responsible for surfacing the failure to the user (and
    /// can call [`Self::user_cancelled`] afterward to release the rows).
    pub async fn user_picked(
        &self,
        browser: Browser,
        remember: bool,
        db: &Db,
    ) -> Result<Vec<i64>, DbError> {
        if remember {
            let db = db.clone();
            let choice = browser;
            tokio::task::spawn_blocking(move || {
                db.with_conn(|c| settings::set_cookies_browser(c, Some(choice)))
            })
            .await
            .map_err(|e| DbError::Decode(format!("join error: {e}")))??;
        }
        let arg = browser.as_yt_dlp_arg().to_string();
        let drained = self.drain_pending().await;
        let ids: Vec<i64> = drained.iter().map(|(id, _)| *id).collect();
        for (_, tx) in drained {
            let _ = tx.send(RetryDecision::PickedBrowser(arg.clone()));
        }
        Ok(ids)
    }

    /// Broadcasts [`RetryDecision::Cancelled`] to every registered oneshot.
    pub async fn user_cancelled(&self) -> Vec<i64> {
        let drained = self.drain_pending().await;
        let ids: Vec<i64> = drained.iter().map(|(id, _)| *id).collect();
        for (_, tx) in drained {
            let _ = tx.send(RetryDecision::Cancelled);
        }
        ids
    }

    /// Removes the registered oneshot for `row_id` without sending anything.
    /// Used by the supervisor when the row is cancelled while the dialog is
    /// still open, so the user's eventual pick does not race with a row that
    /// has already stopped.
    pub async fn withdraw(&self, row_id: i64) {
        let mut state = self.inner.lock().await;
        state.pending.remove(&row_id);
    }

    /// Number of rows currently waiting on a user decision. Used by the
    /// download manager to feed the modal's affected-count copy (UC 10).
    pub async fn pending_count(&self) -> usize {
        self.inner.lock().await.pending.len()
    }

    async fn drain_pending(&self) -> Vec<(i64, oneshot::Sender<RetryDecision>)> {
        let mut state = self.inner.lock().await;
        let mut out: Vec<(i64, oneshot::Sender<RetryDecision>)> =
            Vec::with_capacity(state.pending.len());
        for (id, tx) in state.pending.drain() {
            out.push((id, tx));
        }
        state.dialog_open = false;
        out
    }
}

/// Picks the modal's default-selected browser for an `open` event (UC 10).
///
/// - If `last_pick` is `Some(name)` and `name` is among `options`, return it
///   so subsequent opens within a session pre-select the user's last pick.
/// - Otherwise return the first option (canonical-order first detected).
/// - Returns `None` only when `options` is empty (the host filters that
///   case out — modal not shown when zero browsers are detected).
#[must_use]
pub fn default_browser_for_open<'a>(
    last_pick: Option<&str>,
    options: &'a [&str],
) -> Option<&'a str> {
    if let Some(name) = last_pick
        && let Some(found) = options.iter().find(|o| **o == name)
    {
        return Some(*found);
    }
    options.first().copied()
}
