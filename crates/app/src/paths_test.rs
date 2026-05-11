//! Tests for [`crate::paths`].
//!
//! These mostly assert the per-OS suffix returned by the `directories` crate
//! lookup, since the absolute prefix depends on the user's environment.
//!
//! The platform-specific tests at the bottom of this file probe
//! [`crate::paths::bundled_yt_dlp_path`] by manipulating files next to the
//! test binary (i.e. the cargo profile dir of the test runner). Those tests
//! serialize via a process-wide mutex because they share filesystem state.

use crate::paths;

// On macOS / Linux, `ProjectDirs::data_local_dir` returns
// `<base>/yt-dlp-ui` so the tail component IS the app name.
#[test]
#[cfg(not(target_os = "windows"))]
fn app_data_dir_ends_with_app_name() {
    let dir = paths::app_data_dir().expect("resolve app_data_dir");
    let last = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("dir has tail component");
    assert_eq!(last, "yt-dlp-ui", "app_data_dir tail must be 'yt-dlp-ui'");
}

// On Windows, `ProjectDirs::data_local_dir` returns
// `%LOCALAPPDATA%\yt-dlp-ui\data` — the tail is `data`, the app
// name is the parent component. The shape is fixed by the
// `directories` crate's Windows convention.
#[test]
#[cfg(target_os = "windows")]
fn app_data_dir_ends_with_data_under_app_name() {
    let dir = paths::app_data_dir().expect("resolve app_data_dir");
    let last = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("dir has tail component");
    assert_eq!(last, "data", "app_data_dir tail must be 'data' on Windows");
    let parent_tail = dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .expect("parent has tail component");
    assert_eq!(
        parent_tail, "yt-dlp-ui",
        "app_data_dir parent tail must be 'yt-dlp-ui' on Windows"
    );
}

#[test]
fn default_download_dir_ends_with_app_name() {
    // Fresh CI runners (notably GHA Ubuntu) may not have ~/Downloads
    // and don't set XDG_DOWNLOAD_DIR, so `directories::UserDirs` returns
    // None and `default_download_dir` errors with NoUserDirs. That's a
    // legitimate environment shape; skip the assertion rather than fail.
    let Ok(dir) = paths::default_download_dir() else {
        return;
    };
    let last = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("dir has tail component");
    assert_eq!(
        last, "yt-dlp-ui",
        "default_download_dir tail must be 'yt-dlp-ui'"
    );
}

#[test]
fn default_download_dir_parent_is_downloads_or_xdg() {
    // Same skip-on-Err posture as above — fresh CI runners may have no
    // resolvable download dir; that is expected, not a regression.
    let Ok(dir) = paths::default_download_dir() else {
        return;
    };
    // We don't bind a hard expectation on the absolute parent path because it
    // depends on the test runner's $HOME, but we do require a parent (i.e. it
    // is not a root path on its own).
    assert!(
        dir.parent().is_some(),
        "default_download_dir must have a parent"
    );
}

// -- Platform-specific bundled-path resolution --------------------------------
//
// The tests below mutate files next to the test binary (under
// `target/<profile>/deps`). They share a process-wide mutex so that tests
// running in parallel don't see partial state from each other. The mutex
// also protects against the rare interleaving where one test's cleanup runs
// while another is checking `is_file`.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn current_exe_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .expect("current_exe")
        .parent()
        .expect("exe has parent")
        .to_path_buf()
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
static BUNDLED_PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// -- UC 17: bundled_ffmpeg_path() per-OS coverage --------------------------
//
// Mirrors the bundled_deno_path test set: on each OS, `bundled_ffmpeg_path`
// returns Ok(<exe_dir>/ffmpeg[.exe]) when the file exists next to the
// running test binary, and Err(BundledMissing { .. }) when it is absent
// (modulo dev `$PATH` fallback — see `linux_bundled_ffmpeg_path_falls_back_to_path`
// for the dev branch).
//
// These tests share `BUNDLED_PROBE_LOCK` with the existing yt-dlp / deno
// probes because they all mutate the cargo profile dir. Any test that
// stages or removes a binary next to `current_exe` MUST hold the mutex.

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_ffmpeg_path_returns_ok_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_path = exe_dir.join("ffmpeg");
    let _ = std::fs::remove_file(&ffmpeg_path);
    std::fs::write(&ffmpeg_path, b"#!/bin/sh\nexit 0\n").expect("write ffmpeg");

    let resolved = paths::bundled_ffmpeg_path();
    let cleanup = std::fs::remove_file(&ffmpeg_path);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffmpeg_path,
            "bundled ffmpeg must point at the file next to the running exe"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffmpeg");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_ffmpeg_path_returns_err_when_absent_and_no_path_match() {
    // When `<exe_dir>/ffmpeg` does not exist AND `$PATH` has no `ffmpeg`,
    // the resolver returns BundledMissing. We can't reliably guarantee
    // that ffmpeg is absent from CI's $PATH (some images have it), so we
    // deliberately scope the assertion to "Err is BundledMissing OR Ok
    // points at a $PATH location". The first branch is the production
    // case for clean installs; the second is the dev fallback.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_path = exe_dir.join("ffmpeg");
    let _ = std::fs::remove_file(&ffmpeg_path);

    let resolved = paths::bundled_ffmpeg_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            assert_ne!(
                p, ffmpeg_path,
                "with no file next to the binary, Ok must come from the $PATH dev fallback, \
                 not the next-to-binary candidate"
            );
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_ffmpeg_path_returns_ok_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_path = exe_dir.join("ffmpeg");
    let _ = std::fs::remove_file(&ffmpeg_path);
    std::fs::write(&ffmpeg_path, b"#!/bin/sh\nexit 0\n").expect("write ffmpeg");

    let resolved = paths::bundled_ffmpeg_path();
    let cleanup = std::fs::remove_file(&ffmpeg_path);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffmpeg_path,
            "bundled ffmpeg must point at the file next to the running exe"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffmpeg");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_ffmpeg_path_returns_err_when_absent_and_no_path_match() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_path = exe_dir.join("ffmpeg");
    let _ = std::fs::remove_file(&ffmpeg_path);

    let resolved = paths::bundled_ffmpeg_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            assert_ne!(
                p, ffmpeg_path,
                "with no file next to the binary, Ok must come from the $PATH dev fallback"
            );
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffmpeg_path_returns_ok_when_exe_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_exe = exe_dir.join("ffmpeg.exe");
    let canonical = exe_dir.join("ffmpeg");
    // Defensive cleanup of either form.
    let _ = std::fs::remove_file(&ffmpeg_exe);
    let _ = std::fs::remove_file(&canonical);
    std::fs::write(&ffmpeg_exe, b"@echo potato\r\n").expect("write ffmpeg.exe");

    let resolved = paths::bundled_ffmpeg_path();
    let cleanup = std::fs::remove_file(&ffmpeg_exe);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffmpeg_exe,
            "Windows bundled ffmpeg must prefer ffmpeg.exe when present"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffmpeg.exe");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffmpeg_path_falls_back_to_canonical_name() {
    // Mirror the yt-dlp Smoke 1 outcome: when no `.exe` is present, the
    // resolver probes the bare `ffmpeg` filename. fetch-ffmpeg.sh /
    // fetch-ffmpeg.ps1 emit the canonical no-extension name on every OS,
    // so this branch is the actual production path on Windows.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffmpeg_exe = exe_dir.join("ffmpeg.exe");
    let canonical = exe_dir.join("ffmpeg");
    let _ = std::fs::remove_file(&ffmpeg_exe);
    let _ = std::fs::remove_file(&canonical);
    std::fs::write(&canonical, b"@echo potato\r\n").expect("write ffmpeg");

    let resolved = paths::bundled_ffmpeg_path();
    let cleanup = std::fs::remove_file(&canonical);

    match resolved {
        Ok(p) => assert_eq!(
            p, canonical,
            "Windows must fall back to canonical 'ffmpeg' (no ext) when .exe absent"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffmpeg");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffmpeg_path_returns_err_when_neither_present() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let _ = std::fs::remove_file(exe_dir.join("ffmpeg.exe"));
    let _ = std::fs::remove_file(exe_dir.join("ffmpeg"));

    let resolved = paths::bundled_ffmpeg_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            // Dev fallback only — not next to the binary.
            assert_ne!(p, exe_dir.join("ffmpeg.exe"));
            assert_ne!(p, exe_dir.join("ffmpeg"));
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

// -- UC 17: expected_bundled_ffmpeg_path_from per-OS branches --------------

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_ffmpeg() {
    // Mirrors `macos_resources_branch_yt_dlp`: on macOS, the bundled
    // ffmpeg lives at <Contents/Resources>/ffmpeg, NOT next to the main
    // binary at <Contents/MacOS>/.
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main-binary").expect("write main");
    let bundled = resources_dir.join("ffmpeg");
    std::fs::write(&bundled, b"#!/bin/sh\nexit 0\n").expect("write ffmpeg");

    let resolved = paths::expected_bundled_ffmpeg_path_from(&macos_dir);
    assert_eq!(
        resolved, bundled,
        "macOS Resources branch must resolve ffmpeg → <Contents/Resources/ffmpeg>"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_dev_fallback_ffmpeg_when_no_contents_parent() {
    // Cargo dev layout: target/<profile>/app, no `.app/Contents/MacOS`
    // wrapping. The helper must fall through to `<exe_dir>/ffmpeg`.
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("yt-dlp-ui"), b"main").expect("write main");
    let bundled = tmp.path().join("ffmpeg");

    let resolved = paths::expected_bundled_ffmpeg_path_from(tmp.path());
    assert_eq!(
        resolved, bundled,
        "macOS dev layout (no Contents/ parent) must resolve ffmpeg next-to-binary"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_skipped_when_resources_ffmpeg_absent() {
    // If `Contents/Resources/ffmpeg` is missing, the helper must NOT
    // silently return the non-existent Resources path; it falls through
    // to the next-to-binary candidate. Mirrors the yt-dlp regression test.
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main").expect("write main");

    let resolved = paths::expected_bundled_ffmpeg_path_from(&macos_dir);
    assert_eq!(
        resolved,
        macos_dir.join("ffmpeg"),
        "missing Resources/ffmpeg must fall through to <exe_dir>/ffmpeg"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_helper_prefers_exe_over_canonical_for_ffmpeg() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let exe = tmp.path().join("ffmpeg.exe");
    let canonical = tmp.path().join("ffmpeg");
    std::fs::write(&exe, b"exe").expect("write exe");
    std::fs::write(&canonical, b"canonical").expect("write canonical");

    let resolved = paths::expected_bundled_ffmpeg_path_from(tmp.path());
    assert_eq!(
        resolved, exe,
        "Windows must prefer ffmpeg.exe over canonical 'ffmpeg'"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_helper_falls_back_to_canonical_for_ffmpeg() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let canonical = tmp.path().join("ffmpeg");
    std::fs::write(&canonical, b"canonical").expect("write canonical");

    let resolved = paths::expected_bundled_ffmpeg_path_from(tmp.path());
    assert_eq!(
        resolved, canonical,
        "Windows must fall back to canonical 'ffmpeg' (no ext) when .exe absent"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_helper_returns_next_to_binary_for_ffmpeg() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Linux helper does not need the file to exist to return the candidate.
    let resolved = paths::expected_bundled_ffmpeg_path_from(tmp.path());
    assert_eq!(
        resolved,
        tmp.path().join("ffmpeg"),
        "Linux ffmpeg helper must resolve to <exe_dir>/ffmpeg"
    );
}

// -- UC 28: bundled_ffprobe_path() per-OS coverage -------------------------
//
// Mirrors the bundled_ffmpeg_path test set line-for-line: on each OS,
// `bundled_ffprobe_path` returns Ok(<exe_dir>/ffprobe[.exe]) when the file
// exists next to the running test binary, and Err(BundledMissing { .. })
// when it is absent (modulo dev `$PATH` fallback). The fetch / build / stage
// protocol for ffprobe is identical to ffmpeg's (BtbN archive extraction on
// Linux + Windows, lipo-merged build artifact on macOS) so the runtime
// resolver's per-OS shape must match exactly.
//
// These tests share `BUNDLED_PROBE_LOCK` with the yt-dlp / deno / ffmpeg
// probes because they all mutate the cargo profile dir. Any test that
// stages or removes a binary next to `current_exe` MUST hold the mutex.

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_ffprobe_path_returns_ok_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_path = exe_dir.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe_path);
    std::fs::write(&ffprobe_path, b"#!/bin/sh\nexit 0\n").expect("write ffprobe");

    let resolved = paths::bundled_ffprobe_path();
    let cleanup = std::fs::remove_file(&ffprobe_path);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffprobe_path,
            "bundled ffprobe must point at the file next to the running exe"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffprobe");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_ffprobe_path_returns_err_when_absent_and_no_path_match() {
    // When `<exe_dir>/ffprobe` does not exist AND `$PATH` has no `ffprobe`,
    // the resolver returns BundledMissing. We can't reliably guarantee
    // that ffprobe is absent from CI's $PATH (some images have it as part
    // of an ffmpeg system install), so we scope the assertion to "Err is
    // BundledMissing OR Ok points at a $PATH location". The first branch
    // is the production case for clean installs; the second is the dev
    // fallback. Mirrors the ffmpeg-equivalent regression posture.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_path = exe_dir.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe_path);

    let resolved = paths::bundled_ffprobe_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            assert_ne!(
                p, ffprobe_path,
                "with no file next to the binary, Ok must come from the $PATH dev fallback, \
                 not the next-to-binary candidate"
            );
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_ffprobe_path_returns_ok_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_path = exe_dir.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe_path);
    std::fs::write(&ffprobe_path, b"#!/bin/sh\nexit 0\n").expect("write ffprobe");

    let resolved = paths::bundled_ffprobe_path();
    let cleanup = std::fs::remove_file(&ffprobe_path);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffprobe_path,
            "bundled ffprobe must point at the file next to the running exe"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffprobe");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_ffprobe_path_returns_err_when_absent_and_no_path_match() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_path = exe_dir.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe_path);

    let resolved = paths::bundled_ffprobe_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            assert_ne!(
                p, ffprobe_path,
                "with no file next to the binary, Ok must come from the $PATH dev fallback"
            );
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffprobe_path_returns_ok_when_exe_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_exe = exe_dir.join("ffprobe.exe");
    let canonical = exe_dir.join("ffprobe");
    // Defensive cleanup of either form.
    let _ = std::fs::remove_file(&ffprobe_exe);
    let _ = std::fs::remove_file(&canonical);
    std::fs::write(&ffprobe_exe, b"@echo potato\r\n").expect("write ffprobe.exe");

    let resolved = paths::bundled_ffprobe_path();
    let cleanup = std::fs::remove_file(&ffprobe_exe);

    match resolved {
        Ok(p) => assert_eq!(
            p, ffprobe_exe,
            "Windows bundled ffprobe must prefer ffprobe.exe when present"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffprobe.exe");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffprobe_path_falls_back_to_canonical_name() {
    // Mirror the ffmpeg / yt-dlp Smoke 1 outcome: when no `.exe` is present,
    // the resolver probes the bare `ffprobe` filename. fetch-ffmpeg.sh /
    // fetch-ffmpeg.ps1 emit the canonical no-extension name on every OS for
    // ffprobe too, so this branch is the actual production path on Windows.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let ffprobe_exe = exe_dir.join("ffprobe.exe");
    let canonical = exe_dir.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe_exe);
    let _ = std::fs::remove_file(&canonical);
    std::fs::write(&canonical, b"@echo potato\r\n").expect("write ffprobe");

    let resolved = paths::bundled_ffprobe_path();
    let cleanup = std::fs::remove_file(&canonical);

    match resolved {
        Ok(p) => assert_eq!(
            p, canonical,
            "Windows must fall back to canonical 'ffprobe' (no ext) when .exe absent"
        ),
        Err(e) => panic!("expected Ok, got Err({e:?})"),
    }
    cleanup.expect("cleanup ffprobe");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_ffprobe_path_returns_err_when_neither_present() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let _ = std::fs::remove_file(exe_dir.join("ffprobe.exe"));
    let _ = std::fs::remove_file(exe_dir.join("ffprobe"));

    let resolved = paths::bundled_ffprobe_path();
    match resolved {
        Err(paths::PathError::BundledMissing { .. }) => {}
        Ok(p) => {
            // Dev fallback only — not next to the binary.
            assert_ne!(p, exe_dir.join("ffprobe.exe"));
            assert_ne!(p, exe_dir.join("ffprobe"));
        }
        Err(e) => panic!("unexpected error variant: {e:?}"),
    }
}

// -- UC 28: expected_bundled_ffprobe_path_from per-OS branches -------------

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_ffprobe() {
    // Mirrors `macos_resources_branch_ffmpeg`: ffprobe co-locates with
    // ffmpeg under <Contents/Resources>/, not next to the main binary at
    // <Contents/MacOS>/.
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main-binary").expect("write main");
    let bundled = resources_dir.join("ffprobe");
    std::fs::write(&bundled, b"#!/bin/sh\nexit 0\n").expect("write ffprobe");

    let resolved = paths::expected_bundled_ffprobe_path_from(&macos_dir);
    assert_eq!(
        resolved, bundled,
        "macOS Resources branch must resolve ffprobe → <Contents/Resources/ffprobe>"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_dev_fallback_ffprobe_when_no_contents_parent() {
    // Cargo dev layout: target/<profile>/app, no `.app/Contents/MacOS`
    // wrapping. The helper must fall through to `<exe_dir>/ffprobe`.
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("yt-dlp-ui"), b"main").expect("write main");
    let bundled = tmp.path().join("ffprobe");

    let resolved = paths::expected_bundled_ffprobe_path_from(tmp.path());
    assert_eq!(
        resolved, bundled,
        "macOS dev layout (no Contents/ parent) must resolve ffprobe next-to-binary"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_skipped_when_resources_ffprobe_absent() {
    // Co-location-invariant guard mirroring the ffmpeg-equivalent: if
    // `Contents/Resources/ffprobe` is missing, the helper must NOT silently
    // return the non-existent Resources path; it falls through to the
    // next-to-binary candidate. This is what makes the startup co-location
    // ERROR log fire on a partial bundle (ffmpeg present + ffprobe absent
    // or vice versa).
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main").expect("write main");

    let resolved = paths::expected_bundled_ffprobe_path_from(&macos_dir);
    assert_eq!(
        resolved,
        macos_dir.join("ffprobe"),
        "missing Resources/ffprobe must fall through to <exe_dir>/ffprobe"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_helper_prefers_exe_over_canonical_for_ffprobe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let exe = tmp.path().join("ffprobe.exe");
    let canonical = tmp.path().join("ffprobe");
    std::fs::write(&exe, b"exe").expect("write exe");
    std::fs::write(&canonical, b"canonical").expect("write canonical");

    let resolved = paths::expected_bundled_ffprobe_path_from(tmp.path());
    assert_eq!(
        resolved, exe,
        "Windows must prefer ffprobe.exe over canonical 'ffprobe'"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_helper_falls_back_to_canonical_for_ffprobe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let canonical = tmp.path().join("ffprobe");
    std::fs::write(&canonical, b"canonical").expect("write canonical");

    let resolved = paths::expected_bundled_ffprobe_path_from(tmp.path());
    assert_eq!(
        resolved, canonical,
        "Windows must fall back to canonical 'ffprobe' (no ext) when .exe absent"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_helper_returns_next_to_binary_for_ffprobe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Linux helper does not need the file to exist to return the candidate.
    let resolved = paths::expected_bundled_ffprobe_path_from(tmp.path());
    assert_eq!(
        resolved,
        tmp.path().join("ffprobe"),
        "Linux ffprobe helper must resolve to <exe_dir>/ffprobe"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_deno_path_returns_some_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let deno_path = exe_dir.join("deno");
    let _ = std::fs::remove_file(&deno_path);
    std::fs::write(&deno_path, b"#!/bin/sh\nexit 0\n").expect("write deno");

    let resolved = paths::bundled_deno_path();
    let cleanup = std::fs::remove_file(&deno_path);

    assert_eq!(
        resolved.as_deref(),
        Some(deno_path.as_path()),
        "bundled deno path must point at the binary next to the running exe"
    );
    cleanup.expect("cleanup deno");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_bundled_deno_path_returns_none_when_absent() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let deno_path = exe_dir.join("deno");
    let _ = std::fs::remove_file(&deno_path);

    assert!(
        paths::bundled_deno_path().is_none(),
        "no bundled deno → None"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_bundled_deno_path_returns_some_when_next_to_binary() {
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let deno_path = exe_dir.join("deno.exe");
    let _ = std::fs::remove_file(&deno_path);
    std::fs::write(&deno_path, b"@echo potato\r\n").expect("write deno.exe");

    let resolved = paths::bundled_deno_path();
    let cleanup = std::fs::remove_file(&deno_path);

    assert_eq!(
        resolved.as_deref(),
        Some(deno_path.as_path()),
        "bundled deno path must point at deno.exe next to the running exe"
    );
    cleanup.expect("cleanup deno.exe");
}

#[cfg(target_os = "windows")]
#[test]
fn windows_debug_falls_back_to_cmd_when_exe_missing() {
    // Regression guard for AC #6: in dev (debug_assertions on), if
    // `yt-dlp.exe` is absent next to the binary, `bundled_yt_dlp_path` should
    // return the `yt-dlp.cmd` wrapper that build.rs places there.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let exe_path = exe_dir.join("yt-dlp.exe");
    let cmd_path = exe_dir.join("yt-dlp.cmd");

    // Defensive: if a previous test left files behind, clean them up.
    let _ = std::fs::remove_file(&exe_path);
    let _ = std::fs::remove_file(&cmd_path);

    std::fs::write(&cmd_path, b"@echo potato\r\n").expect("write cmd");

    let resolved = paths::bundled_yt_dlp_path().expect("resolve bundled");
    let cleanup_cmd = std::fs::remove_file(&cmd_path);

    assert_eq!(
        resolved, cmd_path,
        "debug fallback must return the .cmd wrapper"
    );
    cleanup_cmd.expect("cleanup cmd");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_deno_path_returns_some_when_next_to_binary() {
    // bundled_deno_path is `Some(_)` when a `deno` file exists next to the
    // running binary. UC 05 AC#14 — bundled is the highest-priority slot.
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let deno_path = exe_dir.join("deno");
    let _ = std::fs::remove_file(&deno_path);
    std::fs::write(&deno_path, b"#!/bin/sh\nexit 0\n").expect("write deno");

    let resolved = paths::bundled_deno_path();
    let cleanup = std::fs::remove_file(&deno_path);

    assert_eq!(
        resolved.as_deref(),
        Some(deno_path.as_path()),
        "bundled deno path must point at the binary next to the running exe"
    );
    cleanup.expect("cleanup deno");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_bundled_deno_path_returns_none_when_absent() {
    // Inverse of the above — when no deno file exists next to the binary,
    // bundled_deno_path returns None (falls back to PATH at the call-site).
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let deno_path = exe_dir.join("deno");
    let _ = std::fs::remove_file(&deno_path);

    let resolved = paths::bundled_deno_path();
    assert!(
        resolved.is_none(),
        "bundled_deno_path must be None when no file exists next to the binary"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_next_to_binary_fallback_when_no_resources_parent() {
    // Regression guard: when running out of `target/<profile>` (no `.app`
    // bundle wrapping the binary), `bundled_yt_dlp_path` must fall through
    // to the next-to-binary location. This is the cargo dev layout, and it
    // must keep working post-UC-03 (UC 03 added a Windows-only branch but
    // must not regress macOS).
    let _guard = BUNDLED_PROBE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let exe_dir = current_exe_dir();
    let next_to_binary = exe_dir.join("yt-dlp");

    let _ = std::fs::remove_file(&next_to_binary);
    std::fs::write(&next_to_binary, b"#!/bin/sh\nexit 0\n").expect("write yt-dlp");

    let resolved = paths::bundled_yt_dlp_path().expect("resolve bundled");
    let cleanup = std::fs::remove_file(&next_to_binary);

    assert_eq!(
        resolved, next_to_binary,
        "macos dev layout must resolve to next-to-binary"
    );
    cleanup.expect("cleanup yt-dlp");
}

// -- UC 06 § 2.6 — `_from(exe_dir, ...)` helper coverage ---------------------
//
// These tests exercise the per-OS branches of the bundled-path resolvers
// directly, by staging a tempdir layout and calling the `_from` helpers. They
// do NOT touch `current_exe()` or the test runner's directory, so they can
// run in parallel with each other and with the BUNDLED_PROBE_LOCK-guarded
// tests above.
//
// Coverage targets per the UC 06 proposal:
//   1. macOS Resources branch (yt-dlp, deno).
//   2. macOS dev-fallback (no `Contents/` parent → next-to-binary).
//   3. macOS ad-window branch (`Contents/MacOS/ad-window`, NOT Resources/ —
//      Apple convention puts helper executables next to the main binary).
//   4. Linux + Windows next-to-binary for `bundled_ad_window_path` helper.
//   5. Windows canonical-name fallback (Smoke 1 outcome: fetch scripts emit
//      single-name `yt-dlp` on every OS, so the Windows branch must probe
//      `<bin>` after `<bin>.exe` and (debug-only) `<bin>.cmd`).

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_yt_dlp() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main-binary").expect("write main");
    let bundled = resources_dir.join("yt-dlp");
    std::fs::write(&bundled, b"#!/bin/sh\nexit 0\n").expect("write yt-dlp");

    let resolved = paths::expected_bundled_path_from(&macos_dir, "yt-dlp");
    assert_eq!(
        resolved, bundled,
        "macOS Resources branch must resolve <Contents/MacOS> + yt-dlp → <Contents/Resources/yt-dlp>"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_deno() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main-binary").expect("write main");
    let bundled = resources_dir.join("deno");
    std::fs::write(&bundled, b"#!/bin/sh\nexit 0\n").expect("write deno");

    let resolved = paths::expected_bundled_deno_path_from(&macos_dir);
    assert_eq!(
        resolved, bundled,
        "macOS Resources branch must resolve deno → <Contents/Resources/deno>"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_ad_window_branch_resolves_to_macos_dir_not_resources() {
    // ad-window is a helper EXECUTABLE — Apple convention is
    // `Contents/MacOS/`, NOT `Contents/Resources/`. The latter is for
    // non-executable support files. This test guards against a future
    // refactor accidentally routing ad-window through the Resources branch.
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main").expect("write main");
    let helper = macos_dir.join("ad-window");
    std::fs::write(&helper, b"helper").expect("write ad-window");
    // Stage a decoy in Resources/ to confirm the resolver does NOT pick it up.
    std::fs::write(resources_dir.join("ad-window"), b"decoy").expect("write decoy");

    let resolved = paths::expected_bundled_ad_window_path_from(&macos_dir);
    assert_eq!(
        resolved, helper,
        "ad-window must resolve to Contents/MacOS/ad-window, not Contents/Resources/ad-window"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_dev_fallback_yt_dlp_when_no_contents_parent() {
    // Cargo dev layout: the binary lives at `target/<profile>/app`, with no
    // `.app/Contents/MacOS/` wrapping. `expected_bundled_path_from` must
    // fall through to `<exe_dir>/yt-dlp`.
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("yt-dlp-ui"), b"main").expect("write main");
    let bundled = tmp.path().join("yt-dlp");

    let resolved = paths::expected_bundled_path_from(tmp.path(), "yt-dlp");
    assert_eq!(
        resolved, bundled,
        "macOS dev layout (no Contents/ parent) must resolve to next-to-binary"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_dev_fallback_deno_when_no_contents_parent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("yt-dlp-ui"), b"main").expect("write main");
    let bundled = tmp.path().join("deno");

    let resolved = paths::expected_bundled_deno_path_from(tmp.path());
    assert_eq!(
        resolved, bundled,
        "macOS dev layout (no Contents/ parent) must resolve deno to next-to-binary"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_resources_branch_skipped_when_resources_file_absent() {
    // If `Contents/Resources/yt-dlp` is missing (broken install), the helper
    // must NOT silently return the non-existent Resources path; it falls
    // through to the next-to-binary candidate. This shapes the error message
    // when bundled_yt_dlp_path() can't find the binary at all.
    let tmp = tempfile::tempdir().expect("tempdir");
    let macos_dir = tmp.path().join("Contents").join("MacOS");
    let resources_dir = tmp.path().join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir).expect("mkdir MacOS");
    std::fs::create_dir_all(&resources_dir).expect("mkdir Resources");
    std::fs::write(macos_dir.join("yt-dlp-ui"), b"main").expect("write main");

    let resolved = paths::expected_bundled_path_from(&macos_dir, "yt-dlp");
    assert_eq!(
        resolved,
        macos_dir.join("yt-dlp"),
        "missing Resources/yt-dlp must fall through to <exe_dir>/yt-dlp"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_ad_window_path_helper_resolves_next_to_binary() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let helper = tmp.path().join("ad-window");
    std::fs::write(&helper, b"helper").expect("write ad-window");

    let resolved = paths::expected_bundled_ad_window_path_from(tmp.path());
    assert_eq!(
        resolved, helper,
        "Linux ad-window must resolve to <exe_dir>/ad-window"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_ad_window_path_helper_returns_path_even_when_absent() {
    // The `_from` helper does not check existence; it returns the candidate
    // path unconditionally on Linux. The public `bundled_ad_window_path`
    // wrapper is the one that maps absence to `None`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let resolved = paths::expected_bundled_ad_window_path_from(tmp.path());
    assert_eq!(
        resolved,
        tmp.path().join("ad-window"),
        "Linux helper returns the canonical next-to-binary path without is_file probe"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_ad_window_path_helper_prefers_exe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let exe = tmp.path().join("ad-window.exe");
    std::fs::write(&exe, b"ad-window").expect("write ad-window.exe");

    let resolved = paths::expected_bundled_ad_window_path_from(tmp.path());
    assert_eq!(
        resolved, exe,
        "Windows ad-window must prefer ad-window.exe when present"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_ad_window_path_helper_falls_back_to_canonical_name() {
    // Symmetric with `expected_bundled_path_from`'s Smoke 1 outcome: when no
    // `.exe` is present, probe the bare `ad-window` filename. Guards against
    // a future packaging path that drops the `.exe` suffix on Windows.
    let tmp = tempfile::tempdir().expect("tempdir");
    let canonical = tmp.path().join("ad-window");
    std::fs::write(&canonical, b"ad-window").expect("write ad-window");

    let resolved = paths::expected_bundled_ad_window_path_from(tmp.path());
    assert_eq!(
        resolved, canonical,
        "Windows ad-window must fall back to canonical name when .exe absent"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_ad_window_path_helper_returns_exe_path_when_neither_exists() {
    // When neither `ad-window.exe` nor `ad-window` exists, the helper still
    // returns the `.exe` candidate so the missing-file error surfaces with
    // the canonical Windows name. The `bundled_ad_window_path` wrapper
    // converts that to `None` via its `is_file()` probe.
    let tmp = tempfile::tempdir().expect("tempdir");
    let resolved = paths::expected_bundled_ad_window_path_from(tmp.path());
    assert_eq!(
        resolved,
        tmp.path().join("ad-window.exe"),
        "Windows fallback returns .exe candidate when neither file exists"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_yt_dlp_canonical_name_fallback_when_no_extension() {
    // Smoke 1 outcome of UC 06: cargo-dist's `include` is a single global
    // list with no per-target pruning, so fetch scripts produce a single
    // `yt-dlp` filename on every OS — including Windows. The Windows branch
    // of `expected_bundled_path_from` must probe that bare name as a
    // last-resort fallback after `.exe` and the (debug-only) `.cmd`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let canonical = tmp.path().join("yt-dlp");
    std::fs::write(&canonical, b"yt-dlp").expect("write yt-dlp");

    let resolved = paths::expected_bundled_path_from(tmp.path(), "yt-dlp");
    assert_eq!(
        resolved, canonical,
        "Windows must fall back to canonical 'yt-dlp' (no ext) when .exe / .cmd absent"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_yt_dlp_prefers_exe_over_canonical_when_both_present() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let exe = tmp.path().join("yt-dlp.exe");
    let canonical = tmp.path().join("yt-dlp");
    std::fs::write(&exe, b"exe").expect("write exe");
    std::fs::write(&canonical, b"canonical").expect("write canonical");

    let resolved = paths::expected_bundled_path_from(tmp.path(), "yt-dlp");
    assert_eq!(
        resolved, exe,
        "Windows must prefer yt-dlp.exe over canonical 'yt-dlp'"
    );
}

// -- UC 16: pick_dest_root branches ----------------------------------------

#[test]
fn pick_dest_root_prefers_downloads_when_available() {
    // UC 16 — happy path: when the user's Downloads dir resolves, we use
    // it as-is. The app-data fallback is NOT consulted.
    let downloads = std::path::PathBuf::from("/users/potato/Downloads/yt-dlp-ui");
    let app_data = std::path::PathBuf::from("/users/potato/.local/share/yt-dlp-ui");
    let got = paths::pick_dest_root(Ok(downloads.clone()), Ok(app_data))
        .expect("pick_dest_root must succeed");
    assert_eq!(
        got, downloads,
        "downloads-available branch must return the downloads path verbatim"
    );
}

#[test]
fn pick_dest_root_falls_back_to_app_data_when_downloads_missing() {
    // UC 16 — fallback path: when `directories::UserDirs` cannot resolve
    // a Downloads folder, we land in `<app_data>/downloads` rather than
    // failing or (forbidden) using `cwd`.
    let app_data = std::path::PathBuf::from("/users/potato/.local/share/yt-dlp-ui");
    let got = paths::pick_dest_root(Err(paths::PathError::NoUserDirs), Ok(app_data.clone()))
        .expect("pick_dest_root must succeed via fallback");
    assert_eq!(
        got,
        app_data.join("downloads"),
        "fallback must be `<app_data>/downloads`"
    );
}

#[test]
fn pick_dest_root_propagates_error_when_both_unavailable() {
    // UC 16 AC#1 — the third branch: when neither Downloads nor app-data
    // can be resolved, propagate `PathError::NoProjectDirs`. The caller
    // refuses to enqueue a row in this rare case; never fall back to cwd.
    let got = paths::pick_dest_root(
        Err(paths::PathError::NoUserDirs),
        Err(paths::PathError::NoProjectDirs),
    );
    assert!(
        matches!(got, Err(paths::PathError::NoProjectDirs)),
        "must propagate the app_data error when both helpers fail; got {got:?}"
    );
}

#[test]
fn pick_dest_root_uses_downloads_even_when_app_data_unavailable() {
    // Defensive: once Downloads resolves, the app-data error is irrelevant.
    // Guards against a future refactor that accidentally `?`-propagates
    // app_data before checking downloads.
    let downloads = std::path::PathBuf::from("/users/potato/Downloads/yt-dlp-ui");
    let got = paths::pick_dest_root(Ok(downloads.clone()), Err(paths::PathError::NoProjectDirs))
        .expect("downloads-available must short-circuit before reading app_data");
    assert_eq!(got, downloads);
}
