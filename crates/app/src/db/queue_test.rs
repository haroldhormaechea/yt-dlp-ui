//! Tests for [`crate::db::queue`].

use std::path::PathBuf;

use rusqlite::Connection;
use yt_dlp_bridge::FormatPref;

use crate::db::DbError;
use crate::db::migrations::run_migrations;
use crate::db::queue;
use crate::db::queue::InsertedOrPreexisting;
use crate::model::{NewQueueItem, PlaceholderKind, QueueStatus, TitleStatus};

fn fresh_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open :memory:");
    run_migrations(&mut conn).unwrap();
    conn
}

fn make_item(url: &str) -> NewQueueItem {
    NewQueueItem {
        url: url.to_string(),
        title: None,
        title_status: TitleStatus::Pending,
        format_pref: FormatPref::BestHeuristic,
        dest_dir: PathBuf::from("/tmp/dl"),
        kind: PlaceholderKind::Video,
        display_order: 0,
    }
}

fn make_pending(url: &str) -> NewQueueItem {
    NewQueueItem {
        url: url.to_string(),
        title: None,
        title_status: TitleStatus::Fetching,
        format_pref: FormatPref::BestHeuristic,
        dest_dir: PathBuf::from("/tmp/dl"),
        kind: PlaceholderKind::Pending,
        display_order: 0,
    }
}

#[test]
fn insert_returns_row_id_and_persists() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    assert!(id > 0);

    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .expect("inserted row must be findable");
    assert_eq!(row.url, "https://example.com/a");
    assert_eq!(row.status, QueueStatus::Queued);
    assert_eq!(row.title_status, TitleStatus::Pending);
    assert_eq!(row.format_pref, FormatPref::BestHeuristic);
    assert_eq!(row.dest_dir, PathBuf::from("/tmp/dl"));
}

#[test]
fn duplicate_url_returns_db_error_duplicate() {
    let conn = fresh_db();
    queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    let err = queue::insert(&conn, make_item("https://example.com/a")).unwrap_err();
    assert!(
        matches!(err, DbError::Duplicate(ref u) if u == "https://example.com/a"),
        "duplicate must surface as DbError::Duplicate (got {err:?})"
    );
}

#[test]
fn list_all_returns_inserted_items() {
    let conn = fresh_db();
    queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::insert(&conn, make_item("https://example.com/b")).unwrap();
    queue::insert(&conn, make_item("https://example.com/c")).unwrap();
    let all = queue::list_all(&conn).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn update_status_in_flight_sets_started_at() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::update_status(&conn, id, QueueStatus::InFlight).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::InFlight);
    assert!(row.started_at.is_some(), "started_at must be set");
    assert!(row.finished_at.is_none(), "finished_at must NOT be set yet");
}

#[test]
fn update_status_done_sets_finished_at() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::update_status(&conn, id, QueueStatus::InFlight).unwrap();
    queue::update_status(&conn, id, QueueStatus::Done).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::Done);
    assert!(row.finished_at.is_some(), "finished_at must be set on Done");
}

#[test]
fn update_status_error_and_cancelled_set_finished_at() {
    let conn = fresh_db();
    let id_e = queue::insert(&conn, make_item("https://example.com/e")).unwrap();
    queue::update_status(&conn, id_e, QueueStatus::Error).unwrap();
    let row_e = queue::find_by_url(&conn, "https://example.com/e")
        .unwrap()
        .unwrap();
    assert!(row_e.finished_at.is_some());

    let id_c = queue::insert(&conn, make_item("https://example.com/c")).unwrap();
    queue::update_status(&conn, id_c, QueueStatus::Cancelled).unwrap();
    let row_c = queue::find_by_url(&conn, "https://example.com/c")
        .unwrap()
        .unwrap();
    assert!(row_c.finished_at.is_some());
}

#[test]
fn update_progress_writes_fields() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::update_progress(
        &conn,
        id,
        Some(42.5),
        Some(2048),
        Some(60),
        Some(2_048_000),
        Some(4_096_000),
    )
    .unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert!((row.progress_pct.unwrap() - 42.5).abs() < 0.001);
    assert_eq!(row.speed_bps, Some(2048));
    assert_eq!(row.eta_s, Some(60));
    // UC 08: byte-count fields persist alongside progress.
    assert_eq!(row.downloaded_bytes, Some(2_048_000));
    assert_eq!(row.size_bytes, Some(4_096_000));
}

#[test]
fn update_title_ok_clears_title_error() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::set_title_error(&conn, id, "boom").unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.title_error.as_deref(), Some("boom"));
    assert_eq!(row.title_status, TitleStatus::Error);

    queue::update_title(&conn, id, Some("Real Title"), TitleStatus::Ok).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.title.as_deref(), Some("Real Title"));
    assert_eq!(row.title_status, TitleStatus::Ok);
    assert!(
        row.title_error.is_none(),
        "title_error must clear on Ok update"
    );
}

#[test]
fn revert_in_flight_to_queued_zeroes_progress_and_started_at() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::update_status(&conn, id, QueueStatus::InFlight).unwrap();
    queue::update_progress(
        &conn,
        id,
        Some(50.0),
        Some(1024),
        Some(30),
        Some(500),
        Some(1000),
    )
    .unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::InFlight);
    assert!(row.progress_pct.is_some());
    assert!(row.started_at.is_some());

    let n = queue::revert_in_flight_to_queued(&conn).unwrap();
    assert_eq!(n, 1, "one row reverted");

    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::Queued);
    assert!(row.progress_pct.is_none(), "progress_pct must be cleared");
    assert!(row.speed_bps.is_none(), "speed_bps must be cleared");
    assert!(row.eta_s.is_none(), "eta_s must be cleared");
    assert!(row.started_at.is_none(), "started_at must be cleared");
}

#[test]
fn list_queued_returns_only_queued() {
    let conn = fresh_db();
    let id_a = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    let _id_b = queue::insert(&conn, make_item("https://example.com/b")).unwrap();
    queue::update_status(&conn, id_a, QueueStatus::InFlight).unwrap();
    let queued = queue::list_queued(&conn).unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].url, "https://example.com/b");
}

#[test]
fn list_titles_to_fetch_returns_pending_and_fetching() {
    let conn = fresh_db();
    let id_a = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    let id_b = queue::insert(&conn, make_item("https://example.com/b")).unwrap();
    let id_c = queue::insert(&conn, make_item("https://example.com/c")).unwrap();
    queue::update_title(&conn, id_a, None, TitleStatus::Fetching).unwrap();
    queue::update_title(&conn, id_b, Some("Resolved"), TitleStatus::Ok).unwrap();
    // id_c stays pending.

    let to_fetch = queue::list_titles_to_fetch(&conn).unwrap();
    let urls: Vec<&str> = to_fetch.iter().map(|i| i.url.as_str()).collect();
    assert_eq!(to_fetch.len(), 2, "pending + fetching only");
    assert!(urls.contains(&"https://example.com/a"));
    assert!(urls.contains(&"https://example.com/c"));
    assert!(!urls.contains(&"https://example.com/b"));
    let _ = id_c;
}

// -- UC 08 ----------------------------------------------------------------

#[test]
fn set_thumbnail_path_persists_path_to_row() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    let p = std::path::PathBuf::from("/var/cache/yt-dlp-ui/thumbnails/abc.jpg");
    queue::set_thumbnail_path(&conn, id, &p).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.thumbnail_path.as_deref(), Some(p.as_path()));
}

#[test]
fn list_pending_thumbnail_fetches_returns_only_null_thumbnail_rows() {
    let conn = fresh_db();
    let _id_a = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    let id_b = queue::insert(&conn, make_item("https://example.com/b")).unwrap();
    let id_c = queue::insert(&conn, make_item("https://example.com/c")).unwrap();

    // b and c get a thumbnail path; a stays NULL.
    let p1 = std::path::PathBuf::from("/cache/b.jpg");
    let p2 = std::path::PathBuf::from("/cache/c.jpg");
    queue::set_thumbnail_path(&conn, id_b, &p1).unwrap();
    queue::set_thumbnail_path(&conn, id_c, &p2).unwrap();

    let pending = queue::list_pending_thumbnail_fetches(&conn).unwrap();
    let urls: Vec<&str> = pending.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://example.com/a"],
        "only the NULL-thumbnail row is queued for refetch"
    );
}

#[test]
fn list_pending_thumbnail_fetches_empty_when_all_cached() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::set_thumbnail_path(&conn, id, &std::path::PathBuf::from("/cache/a.jpg")).unwrap();
    let pending = queue::list_pending_thumbnail_fetches(&conn).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn set_finished_stamps_size_and_downloaded_via_coalesce() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::set_finished(&conn, id, Some(1024 * 1024)).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.size_bytes, Some(1024 * 1024));
    assert_eq!(
        row.downloaded_bytes,
        Some(1024 * 1024),
        "set_finished snapshots downloaded_bytes := size_bytes for done state"
    );
    assert_eq!(row.status, QueueStatus::Done);
}

#[test]
fn set_error_msg_does_not_change_status() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/a")).unwrap();
    queue::set_error_msg(&conn, id, "oops").unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/a")
        .unwrap()
        .unwrap();
    assert_eq!(row.error_msg.as_deref(), Some("oops"));
    assert_eq!(row.status, QueueStatus::Queued, "status untouched");
}

// -- UC 02 ----------------------------------------------------------------

#[test]
fn update_partial_path_round_trips() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/uc02")).unwrap();
    let p = std::path::PathBuf::from("/tmp/clip.mp4.part");
    queue::update_partial_path(&conn, id, &p).unwrap();

    let row = queue::find_by_url(&conn, "https://example.com/uc02")
        .unwrap()
        .unwrap();
    assert_eq!(row.partial_file_path.as_deref(), Some(p.as_path()));

    // Overwrite — last write wins.
    let q = std::path::PathBuf::from("/tmp/clip-2.mp4.part");
    queue::update_partial_path(&conn, id, &q).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/uc02")
        .unwrap()
        .unwrap();
    assert_eq!(row.partial_file_path.as_deref(), Some(q.as_path()));
}

#[test]
fn delete_by_id_removes_row_when_no_history() {
    let mut conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/del")).unwrap();
    let n = queue::delete_by_id(&mut conn, id).unwrap();
    assert_eq!(n, 1, "exactly one queue_items row deleted");
    assert!(
        queue::find_by_url(&conn, "https://example.com/del")
            .unwrap()
            .is_none(),
        "row gone"
    );
}

#[test]
fn delete_by_id_cascades_history_rows() {
    let mut conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/cascade")).unwrap();
    // Seed a history row referencing this queue item.
    conn.execute(
        "INSERT INTO history (queue_item_id, file_path, bytes, completed_at)
         VALUES (?, '/tmp/done.mp4', 1024, CURRENT_TIMESTAMP)",
        [id],
    )
    .unwrap();

    let history_before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM history WHERE queue_item_id = ?",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(history_before, 1);

    let n = queue::delete_by_id(&mut conn, id).unwrap();
    assert_eq!(n, 1);

    let history_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM history WHERE queue_item_id = ?",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        history_after, 0,
        "history rows for the deleted queue item must be gone (transactional cascade)"
    );
}

#[test]
fn delete_by_id_returns_zero_for_missing_id() {
    let mut conn = fresh_db();
    let n = queue::delete_by_id(&mut conn, 999_999).unwrap();
    assert_eq!(n, 0, "no row to delete");
}

#[test]
fn clear_for_restart_preserves_resume_scaffolding_field_by_field() {
    // UC 02 AC#13 / AC#14: Restart must zero progress fields (so the UI
    // shows a clean queued row) AND preserve `size_bytes`,
    // `partial_file_path`, `dest_dir`, `format_pref`, `url`, `title` so
    // yt-dlp's `--continue` resumes from the snapshot.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/restart")).unwrap();

    // Populate the row as if a download had cancelled mid-flight.
    queue::update_title(&conn, id, Some("Restart Title"), TitleStatus::Ok).unwrap();
    queue::update_status(&conn, id, QueueStatus::InFlight).unwrap();
    queue::update_progress(
        &conn,
        id,
        Some(75.0),
        Some(2_048),
        Some(15),
        Some(750_000),
        Some(1_000_000),
    )
    .unwrap();
    queue::update_status(&conn, id, QueueStatus::Cancelled).unwrap();
    queue::set_error_msg(&conn, id, "should be cleared").unwrap();
    let part_path = std::path::PathBuf::from("/tmp/Restart Title.mp4.part");
    queue::update_partial_path(&conn, id, &part_path).unwrap();

    let before = queue::find_by_url(&conn, "https://example.com/restart")
        .unwrap()
        .unwrap();
    assert_eq!(before.status, QueueStatus::Cancelled);
    assert!(before.started_at.is_some());
    assert!(before.finished_at.is_some());
    assert!(before.progress_pct.is_some());
    assert!(before.error_msg.is_some());
    assert_eq!(before.size_bytes, Some(1_000_000));
    assert_eq!(
        before.partial_file_path.as_deref(),
        Some(part_path.as_path())
    );

    queue::clear_for_restart(&conn, id).unwrap();

    let after = queue::find_by_url(&conn, "https://example.com/restart")
        .unwrap()
        .unwrap();
    // Cleared fields.
    assert_eq!(after.status, QueueStatus::Queued, "status flips to queued");
    assert!(after.started_at.is_none(), "started_at cleared");
    assert!(after.finished_at.is_none(), "finished_at cleared");
    assert!(after.progress_pct.is_none(), "progress_pct cleared");
    assert!(after.eta_s.is_none(), "eta_s cleared");
    assert!(after.speed_bps.is_none(), "speed_bps cleared");
    assert!(after.downloaded_bytes.is_none(), "downloaded_bytes cleared");
    assert!(after.error_msg.is_none(), "error_msg cleared");

    // Preserved-for-resume fields.
    assert_eq!(
        after.size_bytes,
        Some(1_000_000),
        "size_bytes preserved so the queue UI still shows the file size"
    );
    assert_eq!(
        after.partial_file_path.as_deref(),
        Some(part_path.as_path()),
        "partial_file_path preserved so --continue can resume"
    );
    assert_eq!(after.dest_dir, before.dest_dir, "dest_dir preserved");
    assert_eq!(
        after.format_pref, before.format_pref,
        "format_pref preserved"
    );
    assert_eq!(after.url, before.url, "url preserved");
    assert_eq!(after.title, before.title, "title preserved");
}

#[test]
fn try_promote_to_in_flight_advances_queued_row() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/promote")).unwrap();
    let promoted = queue::try_promote_to_in_flight(&conn, id).unwrap();
    assert!(promoted, "queued row must be promotable");

    let row = queue::find_by_url(&conn, "https://example.com/promote")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::InFlight);
    assert!(row.started_at.is_some(), "started_at stamped on promotion");
}

#[test]
fn try_promote_to_in_flight_refuses_cancelled_row() {
    // UC 02 challenger flag B: when `cancel_one` flips the row to
    // `cancelled` between the runner's read and the supervisor's first
    // write, `try_promote_to_in_flight` MUST return false so the
    // supervisor aborts before spawning yt-dlp.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/race")).unwrap();
    queue::update_status(&conn, id, QueueStatus::Cancelled).unwrap();

    let promoted = queue::try_promote_to_in_flight(&conn, id).unwrap();
    assert!(
        !promoted,
        "cancelled row must NOT be promoted (returns false)"
    );

    let row = queue::find_by_url(&conn, "https://example.com/race")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.status,
        QueueStatus::Cancelled,
        "row must remain cancelled (not overwritten to in_flight)"
    );
    assert!(
        row.started_at.is_none(),
        "started_at must NOT be stamped on a refused promotion"
    );
}

#[test]
fn try_promote_to_in_flight_refuses_done_or_error_row() {
    // Belt-and-braces — only `queued` rows are promotable; everything else
    // bails out cleanly. Mirrors the SQL `WHERE status = 'queued'` guard.
    let conn = fresh_db();
    let id_done = queue::insert(&conn, make_item("https://example.com/done")).unwrap();
    queue::update_status(&conn, id_done, QueueStatus::Done).unwrap();
    assert!(!queue::try_promote_to_in_flight(&conn, id_done).unwrap());

    let id_err = queue::insert(&conn, make_item("https://example.com/err")).unwrap();
    queue::update_status(&conn, id_err, QueueStatus::Error).unwrap();
    assert!(!queue::try_promote_to_in_flight(&conn, id_err).unwrap());
}

// -- UC 16: update_dest_dir gating ----------------------------------------

#[test]
fn update_dest_dir_writes_when_in_flight() {
    // UC 16 — `update_dest_dir` is the supervisor's spawn-time persist hook.
    // A row that has been promoted to in_flight gets its `dest_dir` rewritten
    // to the resolved path. This is the happy-path leg of the WHERE-clause
    // guard.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/in-flight-write")).unwrap();
    queue::update_status(&conn, id, QueueStatus::InFlight).unwrap();

    let resolved = PathBuf::from("/var/tmp/yt-dlp-ui/resolved");
    queue::update_dest_dir(&conn, id, &resolved).unwrap();

    let row = queue::find_by_url(&conn, "https://example.com/in-flight-write")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.dest_dir, resolved,
        "in_flight row's dest_dir must be rewritten to the resolved path"
    );
}

#[test]
fn update_dest_dir_no_op_on_cancelling_row() {
    // UC 16 — the WHERE-clause guard `status = 'in_flight'` makes
    // `update_dest_dir` a no-op when the row has raced to `cancelling`
    // (e.g. user clicks Cancel between the supervisor's promotion and its
    // dest resolve). The cancelled / cancelling row keeps whatever
    // `dest_dir` was on it before.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_item("https://example.com/raced-cancel")).unwrap();
    queue::update_status(&conn, id, QueueStatus::Cancelling).unwrap();

    // Sanity: capture the row's dest_dir before the no-op call.
    let before = queue::find_by_url(&conn, "https://example.com/raced-cancel")
        .unwrap()
        .unwrap()
        .dest_dir;

    let resolved = PathBuf::from("/var/tmp/yt-dlp-ui/should-not-be-written");
    queue::update_dest_dir(&conn, id, &resolved).unwrap();

    let row = queue::find_by_url(&conn, "https://example.com/raced-cancel")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.dest_dir, before,
        "cancelling row's dest_dir must remain its pre-call value (WHERE-clause guard refused the write)"
    );
    assert_ne!(
        row.dest_dir, resolved,
        "the would-be write must NOT have landed (defensive)"
    );
}

#[test]
fn update_dest_dir_no_op_on_other_terminal_states() {
    // UC 16 — the same WHERE-clause guard refuses writes on done /
    // cancelled / error / queued rows. Belt-and-braces against future
    // refactors that might call `update_dest_dir` from outside the
    // supervisor's spawn-time path.
    let conn = fresh_db();
    let resolved = PathBuf::from("/var/tmp/yt-dlp-ui/post");

    for status in [
        QueueStatus::Queued,
        QueueStatus::Done,
        QueueStatus::Cancelled,
        QueueStatus::Error,
    ] {
        let url = format!("https://example.com/guard-{}", status.as_str());
        let id = queue::insert(&conn, make_item(&url)).unwrap();
        if !matches!(status, QueueStatus::Queued) {
            queue::update_status(&conn, id, status).unwrap();
        }
        let before = queue::find_by_url(&conn, &url).unwrap().unwrap().dest_dir;

        queue::update_dest_dir(&conn, id, &resolved).unwrap();

        let after = queue::find_by_url(&conn, &url).unwrap().unwrap().dest_dir;
        assert_eq!(
            after, before,
            "row in status {status:?} must NOT have its dest_dir rewritten by `update_dest_dir`"
        );
    }
}

// -- UC 27: placeholder rows ---------------------------------------------

#[test]
fn insert_round_trips_kind_start_requested_and_display_order() {
    // The three new columns ride through INSERT → SELECT untouched.
    let conn = fresh_db();
    let mut item = make_pending("https://example.com/uc27-rt");
    item.display_order = 4_096;
    let id = queue::insert(&conn, item).unwrap();
    assert!(id > 0);

    let row = queue::find_by_url(&conn, "https://example.com/uc27-rt")
        .unwrap()
        .expect("row exists");
    assert_eq!(row.kind, PlaceholderKind::Pending);
    assert!(
        !row.start_requested,
        "freshly-inserted rows default to start_requested = false"
    );
    assert_eq!(row.display_order, 4_096);
}

#[test]
fn insert_defaults_existing_callers_to_video_kind_and_video_gate() {
    // A `Video`-kind row inserted via `make_item` lands on the auto-promote
    // path of `list_queued` / `try_promote_to_in_flight`.
    let conn = fresh_db();
    let _id = queue::insert(&conn, make_item("https://example.com/v1")).unwrap();
    let queued = queue::list_queued(&conn).unwrap();
    assert_eq!(queued.len(), 1, "video row appears in list_queued");

    let promoted = queue::try_promote_to_in_flight(&conn, queued[0].id).unwrap();
    assert!(promoted, "video kind must be promotable");
}

#[test]
fn list_queued_excludes_pending_kind() {
    // UC 27: pending placeholders never appear in list_queued — they need
    // enumeration to finish before they become auto-promotable.
    let conn = fresh_db();
    queue::insert(&conn, make_pending("https://example.com/ph")).unwrap();
    queue::insert(&conn, make_item("https://example.com/v")).unwrap();
    let queued = queue::list_queued(&conn).unwrap();
    let urls: Vec<&str> = queued.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(urls, vec!["https://example.com/v"]);
}

#[test]
fn try_promote_to_in_flight_refuses_pending_kind() {
    // UC 27: SQL `WHERE kind = 'video'` gate. A pending row is never
    // promoted by the auto-runner, even when status = 'queued'.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_pending("https://example.com/ph")).unwrap();
    let promoted = queue::try_promote_to_in_flight(&conn, id).unwrap();
    assert!(!promoted, "pending kind must NOT be promoted");

    let row = queue::find_by_url(&conn, "https://example.com/ph")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.status,
        QueueStatus::Queued,
        "status untouched on refused promotion"
    );
    assert_eq!(row.kind, PlaceholderKind::Pending);
}

#[test]
fn promote_placeholder_to_video_flips_kind_and_clears_start_requested() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_pending("https://example.com/promote")).unwrap();
    queue::set_start_requested(&conn, id, true).unwrap();

    queue::promote_placeholder_to_video(&conn, id).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/promote")
        .unwrap()
        .unwrap();
    assert_eq!(row.kind, PlaceholderKind::Video);
    assert!(
        !row.start_requested,
        "start_requested must reset on promotion"
    );
}

#[test]
fn set_start_requested_round_trips_bool() {
    let conn = fresh_db();
    let id = queue::insert(&conn, make_pending("https://example.com/sr")).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/sr")
        .unwrap()
        .unwrap();
    assert!(!row.start_requested);

    queue::set_start_requested(&conn, id, true).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/sr")
        .unwrap()
        .unwrap();
    assert!(row.start_requested);

    queue::set_start_requested(&conn, id, false).unwrap();
    let row = queue::find_by_url(&conn, "https://example.com/sr")
        .unwrap()
        .unwrap();
    assert!(!row.start_requested);
}

#[test]
fn clear_for_restart_placeholder_resets_fields_but_keeps_kind_pending() {
    // UC 27: Restart of a placeholder zeroes progress / error fields but
    // keeps `kind = 'pending'` so the manager can re-spawn enumeration.
    let conn = fresh_db();
    let id = queue::insert(&conn, make_pending("https://example.com/restart-ph")).unwrap();
    queue::update_status(&conn, id, QueueStatus::Cancelled).unwrap();
    queue::set_error_msg(&conn, id, "before-restart").unwrap();
    queue::set_title_error(&conn, id, "title-before").unwrap();

    queue::clear_for_restart_placeholder(&conn, id).unwrap();

    let row = queue::find_by_url(&conn, "https://example.com/restart-ph")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, QueueStatus::Queued);
    assert_eq!(row.kind, PlaceholderKind::Pending, "kind stays pending");
    assert_eq!(row.title_status, TitleStatus::Pending);
    assert!(row.title_error.is_none(), "title_error cleared");
    assert!(row.error_msg.is_none(), "error_msg cleared");
    assert!(row.started_at.is_none());
    assert!(row.finished_at.is_none());
}

#[test]
fn list_pending_enumerations_returns_only_pending_kind() {
    // UC 27: startup recovery seeds enumeration re-issue from this view.
    let conn = fresh_db();
    let id_ph_a = queue::insert(&conn, make_pending("https://example.com/ph-a")).unwrap();
    let _id_ph_b = queue::insert(&conn, make_pending("https://example.com/ph-b")).unwrap();
    let _id_v = queue::insert(&conn, make_item("https://example.com/video")).unwrap();

    let pending = queue::list_pending_enumerations(&conn).unwrap();
    let urls: Vec<&str> = pending.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(pending.len(), 2, "exactly two pending placeholders");
    assert!(urls.contains(&"https://example.com/ph-a"));
    assert!(urls.contains(&"https://example.com/ph-b"));
    assert!(!urls.contains(&"https://example.com/video"));
    let _ = id_ph_a;
}

#[test]
fn max_display_order_returns_zero_on_empty_table() {
    let conn = fresh_db();
    let v = queue::max_display_order(&conn).unwrap();
    assert_eq!(v, 0);
}

#[test]
fn max_display_order_picks_up_highest_value() {
    let conn = fresh_db();
    for (i, url) in [
        "https://example.com/a",
        "https://example.com/b",
        "https://example.com/c",
    ]
    .iter()
    .enumerate()
    {
        let mut item = make_item(url);
        item.display_order = (i as i64 + 1) * 1_048_576;
        queue::insert(&conn, item).unwrap();
    }
    let v = queue::max_display_order(&conn).unwrap();
    assert_eq!(v, 3 * 1_048_576);
}

#[test]
fn replace_placeholder_with_children_happy_path() {
    // UC 27: placeholder row is deleted, N children inserted with
    // display_order slots strictly inside the placeholder's old range.
    let mut conn = fresh_db();
    let mut placeholder = make_pending("https://example.com/playlist");
    placeholder.display_order = 1_048_576; // one stride
    let ph_id = queue::insert(&conn, placeholder).unwrap();

    let children = vec![
        NewQueueItem {
            url: "https://example.com/c1".to_string(),
            title: Some("c1".to_string()),
            title_status: TitleStatus::Ok,
            format_pref: FormatPref::BestHeuristic,
            dest_dir: PathBuf::from("/tmp/dl"),
            kind: PlaceholderKind::Video,
            display_order: 0,
        },
        NewQueueItem {
            url: "https://example.com/c2".to_string(),
            title: Some("c2".to_string()),
            title_status: TitleStatus::Ok,
            format_pref: FormatPref::BestHeuristic,
            dest_dir: PathBuf::from("/tmp/dl"),
            kind: PlaceholderKind::Video,
            display_order: 0,
        },
    ];

    let out =
        queue::replace_placeholder_with_children(&mut conn, ph_id, 1_048_576, &children).unwrap();
    assert_eq!(out.len(), 2);
    for (_, _, tag) in &out {
        assert!(matches!(tag, InsertedOrPreexisting::Inserted));
    }

    // Placeholder is gone.
    assert!(
        queue::find_by_url(&conn, "https://example.com/playlist")
            .unwrap()
            .is_none(),
        "placeholder row deleted"
    );
    // Children are present, each with kind = Video and display_order strictly
    // inside (placeholder_display_order, placeholder_display_order + stride).
    let c1 = queue::find_by_url(&conn, "https://example.com/c1")
        .unwrap()
        .unwrap();
    let c2 = queue::find_by_url(&conn, "https://example.com/c2")
        .unwrap()
        .unwrap();
    assert_eq!(c1.kind, PlaceholderKind::Video);
    assert_eq!(c2.kind, PlaceholderKind::Video);
    assert!(
        c1.display_order > 1_048_576 && c1.display_order < 2 * 1_048_576,
        "c1 display_order ({}) inside placeholder slot",
        c1.display_order
    );
    assert!(
        c2.display_order > c1.display_order,
        "children preserve playlist order via increasing display_order"
    );
}

#[test]
fn replace_placeholder_with_children_collides_via_insert_or_ignore() {
    // UC 27: a playlist entry whose URL is already in the queue returns
    // the pre-existing row id with `Preexisting` tag; freshly-inserted
    // entries get `Inserted`. No transaction abort.
    let mut conn = fresh_db();

    // Seed an existing video row that will collide with the playlist.
    let preexisting_id = queue::insert(&conn, make_item("https://example.com/collide")).unwrap();

    // Placeholder for the playlist add.
    let mut placeholder = make_pending("https://example.com/playlist");
    placeholder.display_order = 1_048_576;
    let ph_id = queue::insert(&conn, placeholder).unwrap();

    let children = vec![
        NewQueueItem {
            url: "https://example.com/collide".to_string(),
            title: Some("dup".to_string()),
            title_status: TitleStatus::Ok,
            format_pref: FormatPref::BestHeuristic,
            dest_dir: PathBuf::from("/tmp/dl"),
            kind: PlaceholderKind::Video,
            display_order: 0,
        },
        NewQueueItem {
            url: "https://example.com/fresh".to_string(),
            title: Some("fresh".to_string()),
            title_status: TitleStatus::Ok,
            format_pref: FormatPref::BestHeuristic,
            dest_dir: PathBuf::from("/tmp/dl"),
            kind: PlaceholderKind::Video,
            display_order: 0,
        },
    ];

    let out =
        queue::replace_placeholder_with_children(&mut conn, ph_id, 1_048_576, &children).unwrap();
    assert_eq!(out.len(), 2);

    // Tag + id of the colliding entry == pre-existing row.
    let (idx0, id0, tag0) = out[0];
    assert_eq!(idx0, 0);
    assert_eq!(
        id0, preexisting_id,
        "collision must return the pre-existing winner's row id"
    );
    assert!(
        matches!(tag0, InsertedOrPreexisting::Preexisting),
        "tag must be Preexisting on collision"
    );

    let (idx1, _id1, tag1) = out[1];
    assert_eq!(idx1, 1);
    assert!(
        matches!(tag1, InsertedOrPreexisting::Inserted),
        "tag must be Inserted for the non-colliding entry"
    );

    // Both URLs exist in the queue; the placeholder is gone.
    assert!(
        queue::find_by_url(&conn, "https://example.com/playlist")
            .unwrap()
            .is_none(),
        "placeholder removed"
    );
    assert!(
        queue::find_by_url(&conn, "https://example.com/collide")
            .unwrap()
            .is_some(),
        "pre-existing collide row still present"
    );
    assert!(
        queue::find_by_url(&conn, "https://example.com/fresh")
            .unwrap()
            .is_some(),
        "fresh child inserted"
    );
}

#[test]
fn replace_placeholder_with_children_empty_input_removes_placeholder() {
    // Edge case: enumeration returned no entries. The placeholder is still
    // deleted; no children land.
    let mut conn = fresh_db();
    let ph_id = queue::insert(&conn, make_pending("https://example.com/playlist")).unwrap();
    let out = queue::replace_placeholder_with_children(&mut conn, ph_id, 1_048_576, &[]).unwrap();
    assert!(out.is_empty());
    assert!(
        queue::find_by_url(&conn, "https://example.com/playlist")
            .unwrap()
            .is_none()
    );
}
