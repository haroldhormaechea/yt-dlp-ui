//! Per-OS browser detection for the `YouTube` bot-check cookies dialog.
//!
//! Detection is filesystem-based on Linux/macOS and `%PROGRAMFILES%`-based on
//! Windows; no `winreg` / `which` dependencies are pulled in. Per-OS coverage
//! mirrors the cfg-branch shape of `crate::paths`.
//!
//! The detection is best-effort. False positives (e.g. a browser binary
//! present but no working cookie DB) are surfaced as plain yt-dlp errors via
//! the standard error path; this module is not in the business of validating
//! the cookie store.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "browsers_test.rs"]
mod browsers_tests;

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().to_path_buf())
}

/// Browsers recognized by yt-dlp's `--cookies-from-browser` flag (canonical
/// set as of yt-dlp 2026.x). The variant order is the canonical UI order
/// returned by [`Browser::variants`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Browser {
    Brave,
    Chrome,
    Chromium,
    Edge,
    Firefox,
    Opera,
    Safari,
    Vivaldi,
}

impl Browser {
    /// The exact string yt-dlp expects after `--cookies-from-browser`.
    #[must_use]
    pub fn as_yt_dlp_arg(&self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
            Self::Opera => "opera",
            Self::Safari => "safari",
            Self::Vivaldi => "vivaldi",
        }
    }

    /// Human-readable, title-cased name shown in user-facing UI (settings
    /// dropdowns, bot-check popup). UC 09 introduced the split between this
    /// and [`Self::as_yt_dlp_arg`] so the dropdown reads "Brave / Chrome / …"
    /// instead of yt-dlp's lowercase argument tokens.
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Brave => "Brave",
            Self::Chrome => "Chrome",
            Self::Chromium => "Chromium",
            Self::Edge => "Edge",
            Self::Firefox => "Firefox",
            Self::Opera => "Opera",
            Self::Safari => "Safari",
            Self::Vivaldi => "Vivaldi",
        }
    }

    /// Inverse of [`Self::display_name`]. Case-insensitive so legacy
    /// lowercase strings (the pre-UC-09 format) still round-trip — that
    /// keeps existing `cookies_browser` DB rows readable across the rename.
    #[must_use]
    pub fn from_display_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "brave" => Some(Self::Brave),
            "chrome" => Some(Self::Chrome),
            "chromium" => Some(Self::Chromium),
            "edge" => Some(Self::Edge),
            "firefox" => Some(Self::Firefox),
            "opera" => Some(Self::Opera),
            "safari" => Some(Self::Safari),
            "vivaldi" => Some(Self::Vivaldi),
            _ => None,
        }
    }

    /// All eight browsers in canonical UI order.
    #[must_use]
    pub fn variants() -> &'static [Browser] {
        &[
            Self::Brave,
            Self::Chrome,
            Self::Chromium,
            Self::Edge,
            Self::Firefox,
            Self::Opera,
            Self::Safari,
            Self::Vivaldi,
        ]
    }
}

/// Returns the list of detected browsers on the current host.
///
/// `root` overrides the filesystem root used for probes; production callers
/// pass `None` (real `/Applications`, `$HOME`, `%PROGRAMFILES%`) and tests
/// pass a `tempdir` whose layout mirrors a real install.
#[must_use]
pub fn detect_installed(root: Option<&Path>) -> Vec<Browser> {
    Browser::variants()
        .iter()
        .copied()
        .filter(|b| is_installed(*b, root))
        .collect()
}

#[cfg(target_os = "macos")]
fn is_installed(browser: Browser, root: Option<&Path>) -> bool {
    // Safari is part of macOS — always available on this host.
    if matches!(browser, Browser::Safari) {
        return true;
    }
    let app_name = macos_app_bundle_name(browser);
    let system_apps = root.map_or_else(
        || PathBuf::from("/Applications"),
        |r| r.join("Applications"),
    );
    if system_apps.join(app_name).exists() {
        return true;
    }
    // Tests pass a `root` whose `Users/<*>/Applications` dirs simulate user
    // installs. Production passes `None` and resolves the real home dir.
    let user_apps_dirs: Vec<PathBuf> = if let Some(r) = root {
        let users = r.join("Users");
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&users) {
            for entry in rd.flatten() {
                out.push(entry.path().join("Applications"));
            }
        }
        out
    } else {
        home_dir()
            .map(|h| vec![h.join("Applications")])
            .unwrap_or_default()
    };
    for user in user_apps_dirs {
        if user.join(app_name).exists() {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn macos_app_bundle_name(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => "Brave Browser.app",
        Browser::Chrome => "Google Chrome.app",
        Browser::Chromium => "Chromium.app",
        Browser::Edge => "Microsoft Edge.app",
        Browser::Firefox => "Firefox.app",
        Browser::Opera => "Opera.app",
        Browser::Safari => "Safari.app",
        Browser::Vivaldi => "Vivaldi.app",
    }
}

#[cfg(target_os = "linux")]
fn is_installed(browser: Browser, root: Option<&Path>) -> bool {
    // Safari does not exist on Linux.
    if matches!(browser, Browser::Safari) {
        return false;
    }
    let home = root
        .map(Path::to_path_buf)
        .or_else(home_dir)
        .unwrap_or_else(|| PathBuf::from("/"));

    if path_has_binary(linux_binary_name(browser), root) {
        return true;
    }
    for cfg in linux_config_dirs(browser) {
        if home.join(".config").join(cfg).exists() {
            return true;
        }
    }
    if let Some(snap) = linux_snap_name(browser)
        && home
            .join("snap")
            .join(snap)
            .join("common")
            .join(snap)
            .exists()
    {
        return true;
    }
    if let Some(flatpak) = linux_flatpak_id(browser)
        && home.join(".var").join("app").join(flatpak).exists()
    {
        return true;
    }
    false
}

#[cfg(target_os = "linux")]
fn linux_binary_name(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => "brave-browser",
        Browser::Chrome => "google-chrome",
        Browser::Chromium => "chromium",
        Browser::Edge => "microsoft-edge",
        Browser::Firefox => "firefox",
        Browser::Opera => "opera",
        Browser::Safari => "",
        Browser::Vivaldi => "vivaldi",
    }
}

#[cfg(target_os = "linux")]
fn linux_config_dirs(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["BraveSoftware/Brave-Browser"],
        Browser::Chrome => &["google-chrome"],
        Browser::Chromium => &["chromium"],
        Browser::Edge => &["microsoft-edge"],
        Browser::Firefox => &[".mozilla/firefox", "mozilla/firefox"],
        Browser::Opera => &["opera"],
        Browser::Safari => &[],
        Browser::Vivaldi => &["vivaldi"],
    }
}

#[cfg(target_os = "linux")]
fn linux_snap_name(browser: Browser) -> Option<&'static str> {
    match browser {
        Browser::Brave => Some("brave"),
        Browser::Chromium => Some("chromium"),
        Browser::Firefox => Some("firefox"),
        Browser::Opera => Some("opera"),
        Browser::Vivaldi => Some("vivaldi"),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn linux_flatpak_id(browser: Browser) -> Option<&'static str> {
    match browser {
        Browser::Brave => Some("com.brave.Browser"),
        Browser::Chrome => Some("com.google.Chrome"),
        Browser::Chromium => Some("org.chromium.Chromium"),
        Browser::Edge => Some("com.microsoft.Edge"),
        Browser::Firefox => Some("org.mozilla.firefox"),
        Browser::Opera => Some("com.opera.Opera"),
        Browser::Vivaldi => Some("com.vivaldi.Vivaldi"),
        // Safari ships only on macOS; no Linux flatpak.
        Browser::Safari => None,
    }
}

#[cfg(target_os = "linux")]
fn path_has_binary(bin: &str, root: Option<&Path>) -> bool {
    if bin.is_empty() {
        return false;
    }
    // Tests pass a `root`; PATH-scan on the host is meaningless under that
    // sandbox. Production passes `None` and uses the real PATH.
    if root.is_some() {
        return false;
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.join(bin).is_file() {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn is_installed(browser: Browser, root: Option<&Path>) -> bool {
    if matches!(browser, Browser::Safari) {
        return false;
    }
    let candidates = windows_install_subdirs(browser);
    let roots = windows_roots(root);
    for r in roots {
        for sub in candidates {
            if r.join(sub).exists() {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn windows_roots(root: Option<&Path>) -> Vec<PathBuf> {
    if let Some(r) = root {
        return vec![
            r.join("Program Files"),
            r.join("Program Files (x86)"),
            r.join("AppData").join("Local"),
        ];
    }
    let mut out = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
        if let Some(v) = std::env::var_os(var) {
            out.push(PathBuf::from(v));
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn windows_install_subdirs(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["BraveSoftware\\Brave-Browser\\Application"],
        Browser::Chrome => &["Google\\Chrome\\Application"],
        Browser::Chromium => &["Chromium\\Application"],
        Browser::Edge => &["Microsoft\\Edge\\Application"],
        Browser::Firefox => &["Mozilla Firefox", "Firefox"],
        Browser::Opera => &["Opera"],
        Browser::Safari => &[],
        Browser::Vivaldi => &["Vivaldi\\Application"],
    }
}
