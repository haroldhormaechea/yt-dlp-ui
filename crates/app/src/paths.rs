//! Per-OS app-data and bundled-binary path resolution.
//!
//! Locations come from the `directories` crate (XDG / Apple / Windows
//! conventions). The bundled `yt-dlp` binary path is per-OS and resolved
//! relative to the running executable; the dev fallback scans `$PATH`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use directories::{ProjectDirs, UserDirs};
use thiserror::Error;

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_tests;

const APP_NAME: &str = "yt-dlp-ui";

/// Errors emitted when resolving filesystem paths.
#[derive(Debug, Error)]
pub enum PathError {
    /// `directories` could not resolve the per-OS data dir for the current
    /// user (typically a misconfigured `$HOME` on Linux or sandboxed shells
    /// without home access).
    #[error("could not resolve project data directory")]
    NoProjectDirs,

    /// `directories` could not resolve the per-OS user dirs (e.g. Downloads).
    #[error("could not resolve user directories")]
    NoUserDirs,

    /// The bundled `yt-dlp` binary was not found at any expected location.
    /// In dev builds (debug assertions on), this only fires after a `$PATH`
    /// scan also turned up nothing.
    #[error("bundled yt-dlp binary missing (expected at: {expected})")]
    BundledMissing { expected: String },
}

/// Returns the per-OS app-data directory for `yt-dlp-ui`.
///
/// Concretely (per the `directories` crate's per-OS conventions):
/// - **Linux:** `~/.local/share/yt-dlp-ui/`
/// - **macOS:** `~/Library/Application Support/yt-dlp-ui/`
/// - **Windows:** `%LOCALAPPDATA%\yt-dlp-ui\data\` (the trailing `\data` is
///   the Windows convention enforced by `ProjectDirs::data_local_dir`).
///
/// # Errors
///
/// Returns [`PathError::NoProjectDirs`] when `directories` cannot resolve the
/// per-OS data dir (typically a missing `$HOME` on Linux).
pub fn app_data_dir() -> Result<PathBuf, PathError> {
    let dirs = ProjectDirs::from("", "", APP_NAME).ok_or(PathError::NoProjectDirs)?;
    Ok(dirs.data_local_dir().to_path_buf())
}

/// Returns the default download destination directory, with `yt-dlp-ui`
/// appended.
///
/// Concretely:
/// - **Linux:** `~/Downloads/yt-dlp-ui/` (or `$XDG_DOWNLOAD_DIR/yt-dlp-ui/`)
/// - **macOS:** `~/Downloads/yt-dlp-ui/`
/// - **Windows:** `%USERPROFILE%\Downloads\yt-dlp-ui\`
///
/// # Errors
///
/// Returns [`PathError::NoUserDirs`] when `directories` cannot resolve the
/// per-OS user dirs (no Downloads folder available).
pub fn default_download_dir() -> Result<PathBuf, PathError> {
    let user = UserDirs::new().ok_or(PathError::NoUserDirs)?;
    let downloads = user.download_dir().ok_or(PathError::NoUserDirs)?;
    Ok(downloads.join(APP_NAME))
}

/// Picks the destination root for downloads.
///
/// Returns `downloads` when present; otherwise falls back to `<app_data>/downloads`.
/// Propagates the `app_data` `PathError` if both are unavailable. Never falls
/// back to `cwd` — UC 16 AC#1.
pub(crate) fn pick_dest_root(
    downloads: Result<PathBuf, PathError>,
    app_data: Result<PathBuf, PathError>,
) -> Result<PathBuf, PathError> {
    if let Ok(p) = downloads {
        Ok(p)
    } else {
        let base = app_data?;
        log_app_data_fallback_once();
        Ok(base.join("downloads"))
    }
}

fn log_app_data_fallback_once() {
    static WARNED: OnceLock<()> = OnceLock::new();
    if WARNED.set(()).is_ok() {
        tracing::warn!(
            "default Downloads directory unavailable; falling back to <app_data>/downloads"
        );
    }
}

/// Resolves the per-OS default download destination, falling back to
/// `<app_data>/downloads` when the user's Downloads directory is unavailable.
///
/// Never falls back to the current working directory — UC 16 AC#1. Returns
/// `Result` so callers can refuse to enqueue a row when no destination can be
/// resolved at all (rare; both helpers would have to fail).
///
/// # Errors
///
/// Returns the underlying [`PathError`] when neither the user Downloads dir
/// nor the per-OS app-data dir can be resolved.
pub fn default_download_dir_or_app_data() -> Result<PathBuf, PathError> {
    pick_dest_root(default_download_dir(), app_data_dir())
}

/// Resolves the path to the bundled `yt-dlp` binary, with a dev fallback.
///
/// In release builds: the binary is expected at the per-OS bundled location
/// (next to the running executable). Missing → error.
///
/// In dev builds (`debug_assertions` enabled): if the bundled binary is
/// absent, we log a `WARN` and scan `$PATH` for `yt-dlp` / `yt-dlp.exe`.
/// This lets `cargo run` work on a developer machine without a full install.
///
/// Per-OS bundled path table (per `PROJECT_BRIEF.md` § Bundled-binary path):
///
/// | OS | Binary location |
/// |---|---|
/// | Linux | `<install_prefix>/yt-dlp` (next to `app`) |
/// | macOS | `yt-dlp-ui.app/Contents/Resources/yt-dlp` |
/// | Windows | `<install_prefix>\yt-dlp.exe` (next to `app.exe`) |
///
/// # Errors
///
/// Returns [`PathError::BundledMissing`] if the binary is not found at the
/// expected location and (in dev) is also not on `$PATH`.
pub fn bundled_yt_dlp_path() -> Result<PathBuf, PathError> {
    let expected = expected_bundled_path();
    if expected.is_file() {
        return Ok(expected);
    }

    if cfg!(debug_assertions) {
        tracing::warn!(
            "bundled yt-dlp not found at {}; scanning $PATH (dev fallback)",
            expected.display()
        );
        if let Some(path) = scan_path_for_yt_dlp() {
            return Ok(path);
        }
    }

    Err(PathError::BundledMissing {
        expected: expected.display().to_string(),
    })
}

fn expected_bundled_path() -> PathBuf {
    let exe_dir = current_exe_dir();
    expected_bundled_path_from(&exe_dir, "yt-dlp")
}

/// Resolves the bundled `<bin_name>` path relative to a given exe directory.
///
/// Public-within-crate so `paths_test.rs` can stage tempdir layouts and
/// exercise the per-OS branches without going through `current_exe()` (which
/// is a syscall and not mockable).
///
/// macOS: prefers `<exe_dir>/../Resources/<bin_name>` when that file exists
/// (the `.app/Contents/MacOS/...` → `Resources/` layout); falls back to
/// `<exe_dir>/<bin_name>` (the cargo dev layout, no `.app` wrapping).
///
/// Windows: probes `<exe_dir>/<bin_name>.exe` first; in debug builds with
/// the `.exe` absent, also probes `<bin_name>.cmd` (UC 03 dev wrapper) and
/// the canonical `<bin_name>` (no extension; Smoke 1 outcome of UC 06 — fetch
/// scripts now produce single-name binaries on all OSes including Windows).
///
/// Linux/other: `<exe_dir>/<bin_name>`.
pub(crate) fn expected_bundled_path_from(exe_dir: &Path, bin_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(contents) = exe_dir.parent() {
            let resources = contents.join("Resources").join(bin_name);
            if resources.is_file() {
                return resources;
            }
        }
        exe_dir.join(bin_name)
    }

    #[cfg(target_os = "windows")]
    {
        let exe_path = exe_dir.join(format!("{bin_name}.exe"));
        if exe_path.is_file() {
            return exe_path;
        }
        if cfg!(debug_assertions) {
            let cmd_path = exe_dir.join(format!("{bin_name}.cmd"));
            if cmd_path.is_file() {
                return cmd_path;
            }
        }
        // Canonical-name fallback (Smoke 1 outcome of UC 06): cargo-dist's
        // `include` is a single global list with no per-target pruning, so
        // fetch scripts produce a single `yt-dlp` filename on every OS,
        // including Windows. Probe that as the last resort. If neither
        // `<bin>.exe`, `<bin>.cmd`, nor `<bin>` exists, return the original
        // `.exe` path so the missing-file error message reads naturally.
        let canonical = exe_dir.join(bin_name);
        if canonical.is_file() {
            return canonical;
        }
        exe_path
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        exe_dir.join(bin_name)
    }
}

/// Walks `$PATH` for `yt-dlp` / `yt-dlp.exe`. No `which` dep on purpose; the
/// implementation is small and avoids a transitive dependency for a dev-only
/// fallback path.
fn scan_path_for_yt_dlp() -> Option<PathBuf> {
    let bin = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    scan_path_for(bin)
}

fn scan_path_for(bin: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn current_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn expected_bundled_deno_path() -> PathBuf {
    let exe_dir = current_exe_dir();
    expected_bundled_deno_path_from(&exe_dir)
}

/// Resolves the path to the bundled `ffmpeg` binary.
///
/// Mirrors [`bundled_yt_dlp_path`] in shape — `Result<PathBuf, PathError>`
/// rather than `Option`, because the type discipline forces every caller
/// (the `app` crate's `run` and the download manager's spawn-time gate
/// in particular) to handle [`PathError::BundledMissing`] explicitly.
/// In dev builds the resolver falls back to a `$PATH` scan so `cargo run`
/// works without a full install.
///
/// Per-OS bundled path table (mirrors yt-dlp; see `PROJECT_BRIEF.md`
/// § Architecture § Bundled-binary path):
///
/// | OS | Binary location |
/// |---|---|
/// | Linux | `<install_prefix>/ffmpeg` (next to `app`) |
/// | macOS | `yt-dlp-ui.app/Contents/Resources/ffmpeg` |
/// | Windows | `<install_prefix>\ffmpeg.exe` (next to `app.exe`) |
///
/// # Errors
///
/// Returns [`PathError::BundledMissing`] if the binary is not found at the
/// expected location and (in dev) is also not on `$PATH`.
pub fn bundled_ffmpeg_path() -> Result<PathBuf, PathError> {
    let expected = expected_bundled_ffmpeg_path();
    if expected.is_file() {
        return Ok(expected);
    }

    if cfg!(debug_assertions) {
        tracing::warn!(
            "bundled ffmpeg not found at {}; scanning $PATH (dev fallback)",
            expected.display()
        );
        if let Some(path) = scan_path_for_ffmpeg() {
            return Ok(path);
        }
    }

    Err(PathError::BundledMissing {
        expected: expected.display().to_string(),
    })
}

fn expected_bundled_ffmpeg_path() -> PathBuf {
    let exe_dir = current_exe_dir();
    expected_bundled_ffmpeg_path_from(&exe_dir)
}

/// Resolves the bundled `ffmpeg` path relative to a given exe directory.
///
/// Public-within-crate so `paths_test.rs` can stage tempdir layouts and
/// exercise the per-OS branches without going through `current_exe()`. The
/// shape matches [`expected_bundled_path_from`] specialized to `ffmpeg`,
/// minus the Windows `.cmd` dev-wrapper branch (no in-repo dev wrapper
/// exists for ffmpeg).
pub(crate) fn expected_bundled_ffmpeg_path_from(exe_dir: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(contents) = exe_dir.parent() {
            let resources = contents.join("Resources").join("ffmpeg");
            if resources.is_file() {
                return resources;
            }
        }
        exe_dir.join("ffmpeg")
    }

    #[cfg(target_os = "windows")]
    {
        let exe_path = exe_dir.join("ffmpeg.exe");
        if exe_path.is_file() {
            return exe_path;
        }
        let canonical = exe_dir.join("ffmpeg");
        if canonical.is_file() {
            return canonical;
        }
        exe_path
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        exe_dir.join("ffmpeg")
    }
}

fn scan_path_for_ffmpeg() -> Option<PathBuf> {
    let bin = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    scan_path_for(bin)
}

/// Resolves the path to the bundled `ffprobe` binary.
///
/// Mirrors [`bundled_ffmpeg_path`] in shape — `Result<PathBuf, PathError>`
/// rather than `Option`, so the type discipline forces every caller (the
/// `app` crate's `run` in particular) to handle [`PathError::BundledMissing`]
/// explicitly. In dev builds the resolver falls back to a `$PATH` scan so
/// `cargo run` works without a full install.
///
/// UC 28: yt-dlp's audio-only post-processing path (`ExtractAudio`) needs
/// ffprobe in addition to ffmpeg. The bridge passes `--ffmpeg-location
/// <parent_dir>` and yt-dlp discovers both binaries from that directory;
/// staging ffprobe at the canonical bundled path is what makes that
/// discovery succeed. The co-location invariant is checked at startup in
/// `lib.rs::run`.
///
/// Per-OS bundled path table (mirrors ffmpeg; see `PROJECT_BRIEF.md`
/// § Architecture § Bundled-binary path):
///
/// | OS | Binary location |
/// |---|---|
/// | Linux | `<install_prefix>/ffprobe` (next to `app`) |
/// | macOS | `yt-dlp-ui.app/Contents/Resources/ffprobe` |
/// | Windows | `<install_prefix>\ffprobe.exe` (next to `app.exe`) |
///
/// # Errors
///
/// Returns [`PathError::BundledMissing`] if the binary is not found at the
/// expected location and (in dev) is also not on `$PATH`.
pub fn bundled_ffprobe_path() -> Result<PathBuf, PathError> {
    let expected = expected_bundled_ffprobe_path();
    if expected.is_file() {
        return Ok(expected);
    }

    if cfg!(debug_assertions) {
        tracing::warn!(
            "bundled ffprobe not found at {}; scanning $PATH (dev fallback)",
            expected.display()
        );
        if let Some(path) = scan_path_for_ffprobe() {
            return Ok(path);
        }
    }

    Err(PathError::BundledMissing {
        expected: expected.display().to_string(),
    })
}

fn expected_bundled_ffprobe_path() -> PathBuf {
    let exe_dir = current_exe_dir();
    expected_bundled_ffprobe_path_from(&exe_dir)
}

/// Resolves the bundled `ffprobe` path relative to a given exe directory.
///
/// Public-within-crate so `paths_test.rs` can stage tempdir layouts and
/// exercise the per-OS branches without going through `current_exe()`. The
/// shape matches [`expected_bundled_ffmpeg_path_from`] specialized to
/// `ffprobe`.
pub(crate) fn expected_bundled_ffprobe_path_from(exe_dir: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(contents) = exe_dir.parent() {
            let resources = contents.join("Resources").join("ffprobe");
            if resources.is_file() {
                return resources;
            }
        }
        exe_dir.join("ffprobe")
    }

    #[cfg(target_os = "windows")]
    {
        let exe_path = exe_dir.join("ffprobe.exe");
        if exe_path.is_file() {
            return exe_path;
        }
        let canonical = exe_dir.join("ffprobe");
        if canonical.is_file() {
            return canonical;
        }
        exe_path
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        exe_dir.join("ffprobe")
    }
}

fn scan_path_for_ffprobe() -> Option<PathBuf> {
    let bin = if cfg!(target_os = "windows") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    scan_path_for(bin)
}

/// On macOS, removes the `com.apple.quarantine` extended attribute from each
/// bundled binary located next to (or under `Resources/` from) the running
/// executable. Idempotent: if the attribute is not present, `xattr -d` exits
/// non-zero and we silently ignore.
///
/// Background: when a user double-clicks `yt-dlp-ui.app` from a `.dmg` we
/// produced unsigned (Posture 3 — see `THREATS.md` § T9), Gatekeeper writes
/// `com.apple.quarantine` on every file inside the bundle. The main `app`
/// binary loses the attribute as soon as the user dismisses the Gatekeeper
/// prompt, but auxiliary binaries we exec via `Command::new` (yt-dlp,
/// ffmpeg, deno) keep theirs and then trip Gatekeeper *again* at first
/// subprocess spawn. We strip the attribute up front so the user is asked
/// once, not three times.
///
/// On non-macOS targets this is a no-op so callers can call unconditionally.
pub fn strip_macos_quarantine_if_needed() {
    #[cfg(target_os = "macos")]
    {
        let exe_dir = current_exe_dir();
        let resources_dir = exe_dir.parent().map(|p| p.join("Resources"));

        let candidates = [
            exe_dir.join("yt-dlp"),
            exe_dir.join("ffmpeg"),
            exe_dir.join("ffprobe"),
            exe_dir.join("deno"),
            resources_dir
                .as_ref()
                .map(|d| d.join("yt-dlp"))
                .unwrap_or_default(),
            resources_dir
                .as_ref()
                .map(|d| d.join("ffmpeg"))
                .unwrap_or_default(),
            resources_dir
                .as_ref()
                .map(|d| d.join("ffprobe"))
                .unwrap_or_default(),
            resources_dir
                .as_ref()
                .map(|d| d.join("deno"))
                .unwrap_or_default(),
        ];

        for path in candidates.iter().filter(|p| p.is_file()) {
            // -d removes; if absent, xattr exits non-zero — silently OK.
            let _ = std::process::Command::new("xattr")
                .arg("-d")
                .arg("com.apple.quarantine")
                .arg(path)
                .output();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // No-op outside macOS.
    }
}

/// Resolves the bundled `deno` path relative to a given exe directory.
///
/// Mirrors [`expected_bundled_path_from`] but always probes the `deno` binary
/// name and skips the Windows `.cmd` dev-wrapper branch (deno has no in-repo
/// dev wrapper).
pub(crate) fn expected_bundled_deno_path_from(exe_dir: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(contents) = exe_dir.parent() {
            let resources = contents.join("Resources").join("deno");
            if resources.is_file() {
                return resources;
            }
        }
        exe_dir.join("deno")
    }

    #[cfg(target_os = "windows")]
    {
        let exe_path = exe_dir.join("deno.exe");
        if exe_path.is_file() {
            return exe_path;
        }
        let canonical = exe_dir.join("deno");
        if canonical.is_file() {
            return canonical;
        }
        exe_path
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        exe_dir.join("deno")
    }
}

/// Returns the bundled deno path when present.
///
/// Mirrors [`bundled_yt_dlp_path`] but never errors — deno is optional in dev
/// (per UC 05 § Deno path), so callers branch on `Option`.
#[must_use]
pub fn bundled_deno_path() -> Option<PathBuf> {
    let expected = expected_bundled_deno_path();
    if expected.is_file() {
        Some(expected)
    } else {
        None
    }
}

/// Resolves the deno binary path for yt-dlp's `--js-runtimes deno:<path>`.
///
/// Priority: bundled (next to the running executable, per OS) > `$PATH`
/// scan > `None`. Logs a `WARN` when neither is found so the resulting
/// startup banner has a corresponding log line for diagnosis.
#[must_use]
pub fn resolved_deno_path() -> Option<PathBuf> {
    if let Some(p) = bundled_deno_path() {
        return Some(p);
    }
    let bin = if cfg!(target_os = "windows") {
        "deno.exe"
    } else {
        "deno"
    };
    if let Some(p) = scan_path_for(bin) {
        return Some(p);
    }
    tracing::warn!("deno not found at bundled path or on $PATH; YouTube extraction may degrade");
    None
}

/// Resolves the bundled `ad-window` helper-executable path.
///
/// Forward-compat: future `app → ad-window` spawn code uses this helper so
/// it doesn't have to re-derive UC 06's installer layouts. macOS resolves
/// to `Contents/MacOS/ad-window` (helper executables live alongside the
/// main binary per Apple convention; **not** `Contents/Resources/`).
///
/// Returns `None` when the helper is not present (e.g. dev builds where
/// `cargo build -p ad-window` hasn't run, or installer breakage).
#[must_use]
pub fn bundled_ad_window_path() -> Option<PathBuf> {
    let expected = expected_bundled_ad_window_path();
    if expected.is_file() {
        Some(expected)
    } else {
        None
    }
}

fn expected_bundled_ad_window_path() -> PathBuf {
    let exe_dir = current_exe_dir();
    expected_bundled_ad_window_path_from(&exe_dir)
}

/// Resolves the bundled `ad-window` path relative to a given exe directory.
///
/// Helper executables on macOS live in `Contents/MacOS/`, **not** in
/// `Contents/Resources/` (Apple convention; Resources/ is for non-executable
/// support files). Linux and Windows use the same next-to-binary layout as
/// the main app; on Windows we keep the canonical-name fallback for symmetry
/// with `expected_bundled_path_from` (Smoke 1 outcome of UC 06).
pub(crate) fn expected_bundled_ad_window_path_from(exe_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let exe_path = exe_dir.join("ad-window.exe");
        if exe_path.is_file() {
            return exe_path;
        }
        let canonical = exe_dir.join("ad-window");
        if canonical.is_file() {
            return canonical;
        }
        exe_path
    }

    #[cfg(not(target_os = "windows"))]
    {
        exe_dir.join("ad-window")
    }
}
