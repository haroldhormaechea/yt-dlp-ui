//! Persistence smoke: seed a row in the `SQLite` DB via the app's public DB
//! API, run the binary in smoke mode, assert it logs `loaded_queue_count=1`
//! and the seeded row survives.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

use app::db::Db;

/// Strips ANSI escape sequences from text. The dev `tracing_subscriber::fmt`
/// layer emits color codes by default; tests need plain text to assert on.
fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // CSI sequence: ESC [ ... <terminator in 0x40..=0x7e>
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn write_fake_yt_dlp(dir: &Path) -> PathBuf {
    let bin = dir.join("yt-dlp");
    fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write fake yt-dlp");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    bin
}

/// Returns the absolute path the app's `paths::app_data_dir()` would resolve
/// to, given an overridden `HOME` / `XDG_DATA_HOME`.
fn expected_app_data_dir(data_root: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        data_root
            .join("Library")
            .join("Application Support")
            .join("yt-dlp-ui")
    } else {
        data_root.join("yt-dlp-ui")
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn seeded_row_survives_smoke_run() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Stage fake yt-dlp on PATH.
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);
    let path_var = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // Stage a per-OS data dir.
    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let app_data_dir = expected_app_data_dir(&data_dir);
    fs::create_dir_all(&app_data_dir).unwrap();

    // Pre-seed db.sqlite with one queue row using the app's public DB API.
    // `with_conn` exposes the locked `rusqlite::Connection`; we drive a raw
    // INSERT directly so we don't need rusqlite in dev-deps for this test.
    let db_path = app::db_path_for(&app_data_dir);
    let dest_str = tmp.path().to_string_lossy().to_string();
    {
        let db = Db::open(&db_path).expect("open db");
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO queue_items (url, title, title_status, status, format_pref, dest_dir, created_at)
                 VALUES (?1, ?2, 'ok', 'queued', ?3, ?4, CURRENT_TIMESTAMP)",
                [
                    "https://example.com/seeded",
                    "Seeded title",
                    "\"BestHeuristic\"",
                    &dest_str,
                ],
            ).map_err(app::db::DbError::from)?;
            Ok(())
        })
        .expect("seed insert");
    }

    let mut cmd = Command::cargo_bin("app").expect("cargo bin app");
    cmd.env("YT_DLP_UI_SMOKE", "1")
        .env("PATH", &path_var)
        .env("RUST_LOG", "info");
    // Smoke spawns the app binary which calls `paths::default_download_dir()`
    // at startup. UserDirs on Linux returns None when XDG_DOWNLOAD_DIR is
    // unset AND ~/Downloads doesn't exist (the GHA Ubuntu runner shape).
    // Stage a Downloads subdir under tmp and point both HOME and
    // XDG_DOWNLOAD_DIR at it. macOS keeps the existing HOME=data_dir
    // (UserDirs there returns ~/Downloads regardless of existence).
    if cfg!(target_os = "macos") {
        cmd.env("HOME", &data_dir);
    } else {
        let downloads = tmp.path().join("Downloads");
        fs::create_dir_all(&downloads).expect("mkdir Downloads");
        cmd.env("XDG_DATA_HOME", &data_dir);
        cmd.env("HOME", tmp.path());
        cmd.env("USERPROFILE", tmp.path());
        cmd.env("XDG_DOWNLOAD_DIR", &downloads);
    }

    let output = cmd.output().expect("run app");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "app must exit 0\nstdout: {stdout}\nstderr: {stderr}"
    );

    let combined = strip_ansi(&format!("{stdout}\n{stderr}"));
    assert!(
        combined.contains("loaded_queue_count=1") || combined.contains("loaded_queue_count: 1"),
        "log must report loaded_queue_count=1. Combined output: {combined}"
    );

    // Verify the row is still in the DB after the smoke run.
    let db = Db::open(&db_path).expect("reopen db");
    let count: i64 = db
        .with_conn(|c| {
            let n: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM queue_items WHERE url = ?1",
                    ["https://example.com/seeded"],
                    |r| r.get(0),
                )
                .map_err(app::db::DbError::from)?;
            Ok(n)
        })
        .expect("count query");
    assert_eq!(count, 1, "seeded row must still exist after smoke run");

    // UC 02 — schema is at v3 after a smoke run, and every cumulative
    // migration's columns are present.
    let max_version: i64 = db
        .with_conn(|c| {
            let n: i64 = c
                .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
                .map_err(app::db::DbError::from)?;
            Ok(n)
        })
        .expect("schema_version query");
    assert_eq!(
        max_version, 3,
        "schema_version must be at v3 after UC 02 migration"
    );

    let new_columns: Vec<String> = db
        .with_conn(|c| {
            let mut stmt = c
                .prepare("PRAGMA table_info(queue_items)")
                .map_err(app::db::DbError::from)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(app::db::DbError::from)?;
            let mut names = Vec::new();
            for r in rows {
                names.push(r.map_err(app::db::DbError::from)?);
            }
            Ok(names)
        })
        .expect("table_info query");
    for required in [
        "thumbnail_path",
        "size_bytes",
        "downloaded_bytes",
        "partial_file_path",
    ] {
        assert!(
            new_columns.iter().any(|c| c == required),
            "queue_items must have column {required} after smoke run (got: {new_columns:?})"
        );
    }
}

/// UC 16 AC#3 — the user's chosen download destination must persist across
/// app restarts. We seed the `settings` KV table with a custom `dest_dir`,
/// run the smoke binary, and confirm the value is still present in the DB
/// afterwards.
#[test]
fn dest_dir_setting_persists_across_smoke_run() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Stage fake yt-dlp on PATH.
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);
    let path_var = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // Stage a per-OS data dir.
    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let app_data_dir = expected_app_data_dir(&data_dir);
    fs::create_dir_all(&app_data_dir).unwrap();

    // Pre-seed `settings.dest_dir` to a custom path the user supposedly
    // picked in the Settings panel during the previous session.
    let custom_dest = tmp.path().join("user-picked-destination");
    fs::create_dir_all(&custom_dest).expect("mkdir custom_dest");

    let db_path = app::db_path_for(&app_data_dir);
    {
        let db = Db::open(&db_path).expect("open db");
        db.with_conn(|c| app::db::settings::set_dest_dir(c, &custom_dest))
            .expect("seed dest_dir setting");
    }

    let mut cmd = Command::cargo_bin("app").expect("cargo bin app");
    cmd.env("YT_DLP_UI_SMOKE", "1")
        .env("PATH", &path_var)
        .env("RUST_LOG", "info");
    // Smoke spawns the app binary which calls `paths::default_download_dir()`
    // at startup. UserDirs on Linux returns None when XDG_DOWNLOAD_DIR is
    // unset AND ~/Downloads doesn't exist (the GHA Ubuntu runner shape).
    // Stage a Downloads subdir under tmp and point both HOME and
    // XDG_DOWNLOAD_DIR at it. macOS keeps the existing HOME=data_dir
    // (UserDirs there returns ~/Downloads regardless of existence).
    if cfg!(target_os = "macos") {
        cmd.env("HOME", &data_dir);
    } else {
        let downloads = tmp.path().join("Downloads");
        fs::create_dir_all(&downloads).expect("mkdir Downloads");
        cmd.env("XDG_DATA_HOME", &data_dir);
        cmd.env("HOME", tmp.path());
        cmd.env("USERPROFILE", tmp.path());
        cmd.env("XDG_DOWNLOAD_DIR", &downloads);
    }

    let output = cmd.output().expect("run app");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "app must exit 0\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Reopen and verify the seeded `dest_dir` value survived. The
    // smoke-mode short-circuit returns BEFORE the UI re-read at
    // `lib.rs:198-203`, but it does NOT touch the settings table; so the
    // surviving value must equal what we wrote.
    let db = Db::open(&db_path).expect("reopen db");
    let got = db
        .with_conn(|c| {
            // Use a sentinel default so we can detect "key absent" as a
            // distinct failure from "key present but wrong value".
            app::db::settings::get_dest_dir(c, std::path::Path::new("/sentinel/never-this"))
        })
        .expect("get_dest_dir");

    assert_ne!(
        got,
        std::path::PathBuf::from("/sentinel/never-this"),
        "dest_dir setting was lost across smoke run (key absent)"
    );
    assert_eq!(
        got, custom_dest,
        "dest_dir setting value drifted across smoke run (AC#3)"
    );
}
