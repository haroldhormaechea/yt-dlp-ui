//! Tests for [`super::BotCheckCoordinator`].
//!
//! Drives the coordinator's state machine directly — these tests do not spawn
//! `yt-dlp` or any UI. The `Db`, when present, uses an on-disk `SQLite` file in a
//! tempdir so the persisted-choice path can be verified end-to-end.

use tempfile::TempDir;
use tokio::sync::oneshot;

use super::{BotCheckCoordinator, CoordinatorOutcome, RetryDecision, default_browser_for_open};
use crate::browsers::Browser;
use crate::db::{Db, settings};

fn fresh_db() -> (TempDir, Db) {
    let tmp = TempDir::new().expect("tempdir");
    let db = Db::open(&tmp.path().join("db.sqlite")).expect("open db");
    (tmp, db)
}

#[tokio::test]
async fn first_report_returns_open_dialog_subsequent_returns_append() {
    let coord = BotCheckCoordinator::new();
    let (tx1, _rx1) = oneshot::channel::<RetryDecision>();
    let (tx2, _rx2) = oneshot::channel::<RetryDecision>();

    let outcome1 = coord.report_auth_required(1, tx1).await;
    assert_eq!(
        outcome1,
        CoordinatorOutcome::OpenDialog,
        "first report opens the dialog"
    );

    let outcome2 = coord.report_auth_required(2, tx2).await;
    assert_eq!(
        outcome2,
        CoordinatorOutcome::Append,
        "second report appends to the open dialog"
    );
}

#[tokio::test]
async fn user_picked_drains_all_oneshots_and_does_not_persist_when_remember_false() {
    let (_tmp, db) = fresh_db();
    let coord = BotCheckCoordinator::new();
    let (tx1, rx1) = oneshot::channel::<RetryDecision>();
    let (tx2, rx2) = oneshot::channel::<RetryDecision>();

    coord.report_auth_required(1, tx1).await;
    coord.report_auth_required(2, tx2).await;

    let ids = coord
        .user_picked(Browser::Chrome, false, &db)
        .await
        .expect("user_picked");
    assert_eq!(ids.len(), 2, "both rows must be drained");
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));

    match rx1.await {
        Ok(RetryDecision::PickedBrowser(arg)) => assert_eq!(arg, "chrome"),
        other => panic!("rx1 expected PickedBrowser(chrome), got {other:?}"),
    }
    match rx2.await {
        Ok(RetryDecision::PickedBrowser(arg)) => assert_eq!(arg, "chrome"),
        other => panic!("rx2 expected PickedBrowser(chrome), got {other:?}"),
    }

    let persisted = db
        .with_conn(settings::get_cookies_browser)
        .expect("read cookies_browser");
    assert!(
        persisted.is_none(),
        "remember=false must NOT persist the choice (got {persisted:?})"
    );
}

#[tokio::test]
async fn user_picked_persists_when_remember_true() {
    let (_tmp, db) = fresh_db();
    let coord = BotCheckCoordinator::new();
    let (tx1, rx1) = oneshot::channel::<RetryDecision>();
    let (tx2, rx2) = oneshot::channel::<RetryDecision>();

    coord.report_auth_required(1, tx1).await;
    coord.report_auth_required(2, tx2).await;

    coord
        .user_picked(Browser::Firefox, true, &db)
        .await
        .expect("user_picked");

    // Both oneshots receive PickedBrowser(firefox).
    match rx1.await {
        Ok(RetryDecision::PickedBrowser(arg)) => assert_eq!(arg, "firefox"),
        other => panic!("rx1 expected PickedBrowser(firefox), got {other:?}"),
    }
    match rx2.await {
        Ok(RetryDecision::PickedBrowser(arg)) => assert_eq!(arg, "firefox"),
        other => panic!("rx2 expected PickedBrowser(firefox), got {other:?}"),
    }

    let persisted = db
        .with_conn(settings::get_cookies_browser)
        .expect("read cookies_browser");
    assert_eq!(
        persisted,
        Some(Browser::Firefox),
        "remember=true must persist the choice"
    );
}

#[tokio::test]
async fn user_cancelled_drains_all_oneshots_with_cancelled() {
    let coord = BotCheckCoordinator::new();
    let (tx1, rx1) = oneshot::channel::<RetryDecision>();
    let (tx2, rx2) = oneshot::channel::<RetryDecision>();

    coord.report_auth_required(1, tx1).await;
    coord.report_auth_required(2, tx2).await;

    let ids = coord.user_cancelled().await;
    assert_eq!(ids.len(), 2);

    match rx1.await {
        Ok(RetryDecision::Cancelled) => {}
        other => panic!("rx1 expected Cancelled, got {other:?}"),
    }
    match rx2.await {
        Ok(RetryDecision::Cancelled) => {}
        other => panic!("rx2 expected Cancelled, got {other:?}"),
    }
}

#[tokio::test]
async fn withdraw_removes_only_that_rows_oneshot() {
    // Cancel-during-bot-check race: row 1's task is cancelled while the dialog
    // is still open. After `withdraw(1)`, a later `user_picked` notifies row 2
    // only, and row 1's oneshot tx is dropped → its supervisor sees `Err` on
    // recv (which the supervisor then routes through its own cancel branch).
    let (_tmp, db) = fresh_db();
    let coord = BotCheckCoordinator::new();
    let (tx1, rx1) = oneshot::channel::<RetryDecision>();
    let (tx2, rx2) = oneshot::channel::<RetryDecision>();

    coord.report_auth_required(1, tx1).await;
    coord.report_auth_required(2, tx2).await;

    coord.withdraw(1).await;

    let ids = coord
        .user_picked(Browser::Edge, false, &db)
        .await
        .expect("user_picked");
    assert_eq!(ids, vec![2], "only row 2 must be notified");

    // Row 1's tx was dropped by withdraw → recv returns Err.
    assert!(
        rx1.await.is_err(),
        "row 1 oneshot must be dropped after withdraw"
    );
    match rx2.await {
        Ok(RetryDecision::PickedBrowser(arg)) => assert_eq!(arg, "edge"),
        other => panic!("rx2 expected PickedBrowser(edge), got {other:?}"),
    }
}

#[tokio::test]
async fn dialog_open_resets_after_user_picked() {
    // After user_picked drains, the dialog flag must reset so a NEXT batch of
    // bot-check reports causes a fresh OpenDialog. Without this, only the
    // first batch ever opens a dialog over the lifetime of the app.
    let (_tmp, db) = fresh_db();
    let coord = BotCheckCoordinator::new();

    let (tx1, _rx1) = oneshot::channel::<RetryDecision>();
    coord.report_auth_required(1, tx1).await;
    coord
        .user_picked(Browser::Chrome, false, &db)
        .await
        .expect("user_picked");

    // Second batch.
    let (tx2, _rx2) = oneshot::channel::<RetryDecision>();
    let outcome = coord.report_auth_required(2, tx2).await;
    assert_eq!(
        outcome,
        CoordinatorOutcome::OpenDialog,
        "after a complete pick, the next report must open a fresh dialog"
    );
}

#[tokio::test]
async fn dialog_open_resets_after_user_cancelled() {
    let coord = BotCheckCoordinator::new();

    let (tx1, _rx1) = oneshot::channel::<RetryDecision>();
    coord.report_auth_required(1, tx1).await;
    coord.user_cancelled().await;

    let (tx2, _rx2) = oneshot::channel::<RetryDecision>();
    let outcome = coord.report_auth_required(2, tx2).await;
    assert_eq!(
        outcome,
        CoordinatorOutcome::OpenDialog,
        "after a cancel, the next report must open a fresh dialog"
    );
}

#[tokio::test]
async fn duplicate_row_id_overwrites_oneshot() {
    // Defensive: if a row reports twice in the same dialog cycle (shouldn't
    // happen by construction, but supervisors are restartable), the latest
    // oneshot wins and the earlier tx is dropped.
    let coord = BotCheckCoordinator::new();
    let (tx1, rx1) = oneshot::channel::<RetryDecision>();
    let (tx1b, rx1b) = oneshot::channel::<RetryDecision>();

    coord.report_auth_required(1, tx1).await;
    let outcome = coord.report_auth_required(1, tx1b).await;
    // Second report on the SAME row: dialog already open → Append.
    assert_eq!(outcome, CoordinatorOutcome::Append);

    let _ = coord.user_cancelled().await;
    assert!(rx1.await.is_err(), "first oneshot dropped on overwrite");
    assert!(matches!(rx1b.await, Ok(RetryDecision::Cancelled)));
}

// ---------------------------------------------------------------------------
// UC 10 additions — pending_count() accessor and default_browser_for_open().
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pending_count_tracks_inserts_drains_and_withdraws() {
    // UC 10 AC#4 affected-count copy is fed by `pending_count()`. Pin the
    // arithmetic across the four mutating call sites: each `report_auth_required`
    // adds one, `withdraw` removes one, and `user_picked` drains everything.
    let (_tmp, db) = fresh_db();
    let coord = BotCheckCoordinator::new();

    assert_eq!(coord.pending_count().await, 0, "fresh coordinator is empty");

    let (tx1, _rx1) = oneshot::channel::<RetryDecision>();
    coord.report_auth_required(1, tx1).await;
    assert_eq!(coord.pending_count().await, 1, "after first report");

    let (tx2, _rx2) = oneshot::channel::<RetryDecision>();
    coord.report_auth_required(2, tx2).await;
    assert_eq!(coord.pending_count().await, 2, "after second report");

    coord.withdraw(1).await;
    assert_eq!(
        coord.pending_count().await,
        1,
        "withdraw drops only the named row"
    );

    coord
        .user_picked(Browser::Chrome, false, &db)
        .await
        .expect("user_picked");
    assert_eq!(
        coord.pending_count().await,
        0,
        "user_picked drains everything"
    );
}

#[test]
fn default_browser_for_open_returns_none_for_empty_options() {
    // Defensive: the host filters out the zero-browsers case (UC 10 AC#13)
    // before showing the modal, but the helper itself must still return None
    // rather than panic if it is ever called with an empty options list.
    assert_eq!(default_browser_for_open(None, &[]), None);
    assert_eq!(default_browser_for_open(Some("chrome"), &[]), None);
}

#[test]
fn default_browser_for_open_first_open_returns_first_option() {
    // UC 10 AC#12 — first open of a session (last_pick is None) defaults to
    // the first detected browser in canonical order. The host passes options
    // in canonical order via `Browser::variants()`-driven filtering, so the
    // first element is the canonical-order first detected.
    assert_eq!(
        default_browser_for_open(None, &["brave", "chrome"]),
        Some("brave")
    );
    assert_eq!(default_browser_for_open(None, &["safari"]), Some("safari"));
}

#[test]
fn default_browser_for_open_returns_last_pick_when_present() {
    // UC 10 AC#12 — subsequent opens within a session pre-select the last
    // browser the user picked, regardless of the Remember checkbox state.
    assert_eq!(
        default_browser_for_open(Some("chrome"), &["brave", "chrome", "firefox"]),
        Some("chrome"),
    );
    // Last-pick at the head of the list is still honored (not replaced by
    // "first option" just because they happen to coincide).
    assert_eq!(
        default_browser_for_open(Some("brave"), &["brave", "chrome"]),
        Some("brave"),
    );
    // Last-pick at the tail of the list.
    assert_eq!(
        default_browser_for_open(Some("firefox"), &["brave", "chrome", "firefox"]),
        Some("firefox"),
    );
}

#[test]
fn default_browser_for_open_falls_back_to_first_when_last_pick_uninstalled() {
    // UC 10 AC#12 edge case — if the last-pick is no longer in the detected
    // list (browser uninstalled mid-session), fall back to the canonical-order
    // first detected rather than returning None or surfacing the stale name.
    assert_eq!(
        default_browser_for_open(Some("opera"), &["brave", "chrome"]),
        Some("brave"),
    );
    // Empty-string last_pick (the lib.rs startup-seed value) treats as
    // "no entry matched" → first option.
    assert_eq!(
        default_browser_for_open(Some(""), &["brave", "chrome"]),
        Some("brave"),
    );
}
