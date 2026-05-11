//! Tests for [`crate::db::migrations::run_migrations`].

use rusqlite::Connection;

use super::run_migrations;

fn open_in_memory() -> Connection {
    Connection::open_in_memory().expect("open :memory:")
}

#[test]
fn creates_schema_version_table() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();

    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1, "schema_version table created");
}

#[test]
fn creates_queue_items_table_with_unique_url() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();

    // Verify the table exists with the columns we expect.
    let mut stmt = conn.prepare("PRAGMA table_info(queue_items)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for required in [
        "id",
        "url",
        "title",
        "title_status",
        "title_error",
        "status",
        "progress_pct",
        "speed_bps",
        "eta_s",
        "error_msg",
        "format_pref",
        "dest_dir",
        "created_at",
        "started_at",
        "finished_at",
    ] {
        assert!(
            cols.iter().any(|c| c == required),
            "queue_items missing column: {required}"
        );
    }

    // UNIQUE constraint on url: a duplicate insert must fail.
    conn.execute(
        "INSERT INTO queue_items (url, title_status, status, format_pref, dest_dir, created_at)
         VALUES ('https://example.com/a', 'pending', 'queued', '\"BestHeuristic\"', '/tmp', CURRENT_TIMESTAMP)",
        [],
    ).unwrap();
    let res = conn.execute(
        "INSERT INTO queue_items (url, title_status, status, format_pref, dest_dir, created_at)
         VALUES ('https://example.com/a', 'pending', 'queued', '\"BestHeuristic\"', '/tmp', CURRENT_TIMESTAMP)",
        [],
    );
    assert!(res.is_err(), "duplicate URL insert must violate UNIQUE");
}

#[test]
fn creates_settings_kv_table() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('foo', 'bar')",
        [],
    )
    .unwrap();
    let value: String = conn
        .query_row("SELECT value FROM settings WHERE key='foo'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(value, "bar");
}

#[test]
fn creates_history_table() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1);
}

#[test]
fn creates_status_index_on_queue_items() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='queue_items_status_idx'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1, "status index must be created");
}

#[test]
fn records_schema_version_row() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();
    let max: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        max, 4,
        "schema_version advances through every migration (UC 27 → v4)"
    );
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 4, "one row per migration applied");
}

#[test]
fn uc02_migration_adds_partial_file_path_column() {
    // UC 02 — `partial_file_path` exists after migration 3 and round-trips
    // through SQLite. The bridge captures yt-dlp's `[download] Destination`
    // line and persists the path here so Remove can clean up the .part file.
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();

    let mut stmt = conn.prepare("PRAGMA table_info(queue_items)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        cols.iter().any(|c| c == "partial_file_path"),
        "queue_items missing UC 02 column partial_file_path (got: {cols:?})"
    );

    // Round-trip insert/select.
    conn.execute(
        "INSERT INTO queue_items (url, title_status, status, format_pref, dest_dir, created_at, partial_file_path)
         VALUES ('https://example.com/uc02', 'pending', 'queued', '\"BestHeuristic\"', '/tmp',
            CURRENT_TIMESTAMP, '/tmp/Big Buck Bunny.mp4.part')",
        [],
    )
    .unwrap();
    let path: Option<String> = conn
        .query_row(
            "SELECT partial_file_path FROM queue_items WHERE url=?",
            ["https://example.com/uc02"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(path.as_deref(), Some("/tmp/Big Buck Bunny.mp4.part"));
}

#[test]
fn uc08_migration_adds_thumbnail_and_byte_columns() {
    // UC 08 — `thumbnail_path`, `size_bytes`, `downloaded_bytes` exist after
    // schema upgrade and round-trip through SQLite.
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();

    let mut stmt = conn.prepare("PRAGMA table_info(queue_items)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for required in ["thumbnail_path", "size_bytes", "downloaded_bytes"] {
        assert!(
            cols.iter().any(|c| c == required),
            "queue_items missing UC 08 column: {required} (got: {cols:?})"
        );
    }

    // Round-trip: insert a row with values for the new columns and read back.
    conn.execute(
        "INSERT INTO queue_items (url, title_status, status, format_pref, dest_dir, created_at,
            thumbnail_path, size_bytes, downloaded_bytes)
         VALUES ('https://example.com/u8', 'pending', 'queued', '\"BestHeuristic\"', '/tmp',
            CURRENT_TIMESTAMP, '/var/cache/thumb.jpg', 4096000, 2048000)",
        [],
    )
    .unwrap();
    let (thumb, size, downloaded): (Option<String>, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT thumbnail_path, size_bytes, downloaded_bytes FROM queue_items WHERE url=?",
            ["https://example.com/u8"],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(thumb.as_deref(), Some("/var/cache/thumb.jpg"));
    assert_eq!(size, Some(4_096_000));
    assert_eq!(downloaded, Some(2_048_000));
}

// -- UC 27 ----------------------------------------------------------------

#[test]
fn uc27_migration_adds_kind_start_requested_and_display_order_columns() {
    // UC 27 — the three new columns exist after schema upgrade and
    // round-trip through SQLite with their NOT NULL defaults.
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();

    let mut stmt = conn.prepare("PRAGMA table_info(queue_items)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for required in ["kind", "start_requested", "display_order"] {
        assert!(
            cols.iter().any(|c| c == required),
            "queue_items missing UC 27 column: {required} (got: {cols:?})"
        );
    }
}

#[test]
fn uc27_migration_defaults_existing_rows_to_video_zero_zero() {
    // Apply migrations 1-3 first, seed a legacy row that the 0004 schema
    // change runs against, then apply 0004 and observe the defaults
    // (`kind = 'video'`, `start_requested = 0`, `display_order = 0`).
    //
    // We simulate the "fixture at schema_version 3" by bypassing the
    // runner and applying the first three migration SQL blobs directly.
    let mut conn = open_in_memory();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
        [],
    )
    .unwrap();

    let m1 = include_str!("migrations/0001_initial.sql");
    let m2 = include_str!("migrations/0002_uc08_thumbnails_and_bytes.sql");
    let m3 = include_str!("migrations/0003_uc02_partial_file_path.sql");
    for (v, sql) in [(1, m1), (2, m2), (3, m3)] {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?, CURRENT_TIMESTAMP)",
            [v],
        )
        .unwrap();
    }

    // Seed a legacy row before 0004 lands.
    conn.execute(
        "INSERT INTO queue_items (url, title_status, status, format_pref, dest_dir, created_at)
         VALUES ('https://example.com/legacy', 'pending', 'queued', '\"BestHeuristic\"', '/tmp', CURRENT_TIMESTAMP)",
        [],
    )
    .unwrap();

    // Now run the full set; only 0004 should be new.
    run_migrations(&mut conn).unwrap();
    let max: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(max, 4, "post-upgrade schema_version = 4");

    let (kind, start_requested, display_order): (String, i64, i64) = conn
        .query_row(
            "SELECT kind, start_requested, display_order FROM queue_items
             WHERE url = 'https://example.com/legacy'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        kind, "video",
        "legacy rows default to kind = 'video' (UC 27 migration safety)"
    );
    assert_eq!(start_requested, 0, "start_requested defaults to 0");
    assert_eq!(display_order, 0, "display_order defaults to 0");
}

#[test]
fn rerun_is_idempotent() {
    let mut conn = open_in_memory();
    run_migrations(&mut conn).unwrap();
    let first: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    run_migrations(&mut conn).unwrap();
    let second: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        first, second,
        "second run must not add another schema_version row"
    );
}
