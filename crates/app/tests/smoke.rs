//! Smoke test: spawn the compiled `app` binary with `YT_DLP_UI_SMOKE=1` and
//! redirect app-data to a tempdir. Assert exit 0 and a `smoke_ok` log line
//! with `loaded_queue_count`.
//!
//! Skipped on Windows; the fake-yt-dlp shim relies on POSIX exec semantics.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::Command;

/// Strips ANSI escape sequences from text. The dev `tracing_subscriber::fmt`
/// layer emits color codes by default; tests need plain text to assert on.
fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
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

/// Writes a no-op fake `yt-dlp` binary into `dir` and returns its path. The
/// smoke path short-circuits before invoking yt-dlp, but `paths::bundled_yt_dlp_path`
/// still scans `$PATH` for it on dev builds and must find something.
fn write_fake_yt_dlp(dir: &std::path::Path) -> PathBuf {
    let bin = dir.join("yt-dlp");
    fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write fake yt-dlp");
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    bin
}

#[test]
fn smoke_run_exits_zero_and_logs_smoke_ok() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Stage a fake yt-dlp on PATH.
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let _fake = write_fake_yt_dlp(&bin_dir);
    let path_var = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // Stage a per-OS data dir override. On macOS we override HOME; on Linux,
    // XDG_DATA_HOME suffices.
    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let mut cmd = Command::cargo_bin("app").expect("cargo bin app");
    cmd.env("YT_DLP_UI_SMOKE", "1")
        .env("PATH", &path_var)
        .env("RUST_LOG", "info");

    // The app calls `paths::default_download_dir()` at startup, which goes
    // through `directories::UserDirs`. UserDirs on Linux reads
    // `$XDG_CONFIG_HOME/user-dirs.dirs` (NOT the env var of the same name);
    // fresh GHA runners have no such file. Stage one. macOS uses CFNetwork
    // and returns ~/Downloads regardless of physical existence, so
    // HOME=tmp/data is sufficient there.
    if cfg!(target_os = "macos") {
        // Slight tweak: `directories` builds Application Support relative to
        // HOME, so HOME=tmp/data routes the SQLite db into tmp/data/Library/
        // Application Support/yt-dlp-ui/db.sqlite.
        cmd.env("HOME", &data_dir);
    } else {
        let downloads = tmp.path().join("Downloads");
        fs::create_dir_all(&downloads).expect("mkdir Downloads");
        let xdg_config = tmp.path().join(".config");
        fs::create_dir_all(&xdg_config).expect("mkdir .config");
        let user_dirs_file = xdg_config.join("user-dirs.dirs");
        fs::write(
            &user_dirs_file,
            format!("XDG_DOWNLOAD_DIR=\"{}\"\n", downloads.display()),
        )
        .expect("write user-dirs.dirs");
        cmd.env("XDG_DATA_HOME", &data_dir);
        cmd.env("HOME", tmp.path());
        cmd.env("USERPROFILE", tmp.path());
        cmd.env("XDG_CONFIG_HOME", &xdg_config);
        cmd.env("XDG_DOWNLOAD_DIR", &downloads);
    }

    let output = cmd.output().expect("run app");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "app must exit 0 in smoke mode\nstdout: {stdout}\nstderr: {stderr}"
    );

    let combined = strip_ansi(&format!("{stdout}\n{stderr}"));
    assert!(
        combined.contains("smoke_ok"),
        "log must contain `smoke_ok`. Combined output: {combined}"
    );
    assert!(
        combined.contains("loaded_queue_count"),
        "log must contain `loaded_queue_count`. Combined output: {combined}"
    );

    // db.sqlite must exist somewhere under the data dir.
    let mut found = false;
    for entry in walkdir(&data_dir) {
        if entry.file_name().and_then(|n| n.to_str()) == Some("db.sqlite") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "db.sqlite must be created somewhere under {}",
        data_dir.display()
    );
}

/// Tiny recursive walker — avoids pulling `walkdir` into dev-deps for one test.
fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(read) = fs::read_dir(&p) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}
