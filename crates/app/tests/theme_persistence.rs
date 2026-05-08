//! AC#7 — theme persistence across an app restart.
//!
//! Seeds a theme value via the public DB API, runs the binary in
//! `YT_DLP_UI_SMOKE` mode pointed at the same data dir, then reopens the DB
//! and asserts the stored theme survived the round-trip. Mirrors the
//! `persistence.rs` pattern (`HOME` / `XDG_DATA_HOME` override + fake yt-dlp
//! on `PATH`).

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

use app::db::Db;
use app::db::settings::{self, ExplicitTheme, ThemePref};

fn write_fake_yt_dlp(dir: &Path) -> PathBuf {
    let bin = dir.join("yt-dlp");
    fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write fake yt-dlp");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    bin
}

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

/// Runs the smoke binary against `data_dir` (with a fake yt-dlp on PATH) and
/// asserts a clean exit. Used to drive the "close + reopen" leg of the
/// persistence round-trip — the smoke binary opens the DB on startup, runs
/// migrations if needed, and exits 0, which is exactly the lifecycle this
/// test wants to verify the theme value survives.
fn run_smoke(data_dir: &Path, bin_dir: &Path) {
    let path_var = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut cmd = Command::cargo_bin("app").expect("cargo bin app");
    cmd.env("YT_DLP_UI_SMOKE", "1")
        .env("PATH", &path_var)
        .env("RUST_LOG", "info");
    if cfg!(target_os = "macos") {
        cmd.env("HOME", data_dir);
    } else {
        cmd.env("XDG_DATA_HOME", data_dir);
    }

    let output = cmd.output().expect("run app");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "smoke run must exit 0\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// AC#7 — set theme to Dark, restart via smoke run, expect Dark to persist.
#[test]
fn theme_dark_survives_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);

    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let app_data_dir = expected_app_data_dir(&data_dir);
    fs::create_dir_all(&app_data_dir).unwrap();

    let db_path = app::db_path_for(&app_data_dir);

    // First launch: open the DB (running migrations) and persist Dark.
    {
        let db = Db::open(&db_path).expect("open db");
        db.with_conn(|c| settings::set_theme(c, ExplicitTheme::Dark))
            .expect("persist dark theme");
    }

    // Restart: smoke run reopens the DB and exits 0.
    run_smoke(&data_dir, &bin_dir);

    // Second launch: reopen, read theme back, expect Dark.
    {
        let db = Db::open(&db_path).expect("reopen db");
        let pref = db.with_conn(settings::get_theme).expect("read theme");
        assert_eq!(
            pref,
            ThemePref::Dark,
            "Dark theme must persist across an app restart (AC#7)"
        );
    }
}

/// AC#7 — set theme to Light, restart via smoke run, expect Light to persist.
#[test]
fn theme_light_survives_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);

    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let app_data_dir = expected_app_data_dir(&data_dir);
    fs::create_dir_all(&app_data_dir).unwrap();

    let db_path = app::db_path_for(&app_data_dir);

    {
        let db = Db::open(&db_path).expect("open db");
        db.with_conn(|c| settings::set_theme(c, ExplicitTheme::Light))
            .expect("persist light theme");
    }

    run_smoke(&data_dir, &bin_dir);

    {
        let db = Db::open(&db_path).expect("reopen db");
        let pref = db.with_conn(settings::get_theme).expect("read theme");
        assert_eq!(
            pref,
            ThemePref::Light,
            "Light theme must persist across an app restart (AC#7)"
        );
    }
}

/// AC#5 — first-launch default is `System` when nothing has been written.
/// Drives the same restart pattern but skips the seeding step, so the smoke
/// run is effectively the "first launch" and the post-run read must report
/// System (the default for an absent key).
#[test]
fn theme_default_after_fresh_install_is_system() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);

    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let app_data_dir = expected_app_data_dir(&data_dir);
    fs::create_dir_all(&app_data_dir).unwrap();

    // First launch: smoke run creates db.sqlite and runs migrations. No theme
    // set yet — this models a brand-new install.
    run_smoke(&data_dir, &bin_dir);

    let db_path = app::db_path_for(&app_data_dir);
    let db = Db::open(&db_path).expect("reopen db");
    let pref = db.with_conn(settings::get_theme).expect("read theme");
    assert_eq!(
        pref,
        ThemePref::System,
        "first-launch default theme is System (AC#5)"
    );
}
