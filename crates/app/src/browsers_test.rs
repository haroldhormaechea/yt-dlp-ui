//! Tests for [`crate::browsers`].
//!
//! Per-OS detection is cfg-gated; the test matrix is gated to match. Each test
//! constructs a temp dir laid out like a minimal install of the relevant
//! browser and asserts [`super::detect_installed`] reports the expected variant.

use tempfile::TempDir;

use super::{Browser, detect_installed};

#[test]
fn browser_yt_dlp_arg_strings_are_canonical() {
    // Pin every variant's yt-dlp arg string. yt-dlp's `--cookies-from-browser`
    // contract is the load-bearing fact for AC#8, AC#10, AC#13.
    assert_eq!(Browser::Brave.as_yt_dlp_arg(), "brave");
    assert_eq!(Browser::Chrome.as_yt_dlp_arg(), "chrome");
    assert_eq!(Browser::Chromium.as_yt_dlp_arg(), "chromium");
    assert_eq!(Browser::Edge.as_yt_dlp_arg(), "edge");
    assert_eq!(Browser::Firefox.as_yt_dlp_arg(), "firefox");
    assert_eq!(Browser::Opera.as_yt_dlp_arg(), "opera");
    assert_eq!(Browser::Safari.as_yt_dlp_arg(), "safari");
    assert_eq!(Browser::Vivaldi.as_yt_dlp_arg(), "vivaldi");
}

// -- display_name <-> from_display_name round-trip (UC 09) -------------------

#[test]
fn display_name_round_trips_for_every_variant() {
    // UC 09 split lower-case yt-dlp args from title-cased UI labels. The
    // round-trip must hold for all eight canonical browsers so settings reads
    // and bot-check labels survive the rename.
    for &variant in Browser::variants() {
        let name = variant.display_name();
        let parsed = Browser::from_display_name(name)
            .unwrap_or_else(|| panic!("display_name {name:?} must round-trip"));
        assert_eq!(
            parsed, variant,
            "{variant:?} must round-trip via display_name"
        );
    }
}

#[test]
fn from_display_name_is_case_insensitive() {
    // Pre-UC-09 stored values were lower-case (the yt-dlp-arg form). Reading
    // them back through `from_display_name` after the rename must still work
    // so existing `cookies_browser` rows are forward-compatible.
    assert_eq!(Browser::from_display_name("brave"), Some(Browser::Brave));
    assert_eq!(Browser::from_display_name("BRAVE"), Some(Browser::Brave));
    assert_eq!(Browser::from_display_name("Brave"), Some(Browser::Brave));
    assert_eq!(
        Browser::from_display_name("FireFox"),
        Some(Browser::Firefox)
    );
    assert_eq!(Browser::from_display_name("safari"), Some(Browser::Safari));
}

#[test]
fn from_display_name_rejects_unknown_strings() {
    assert_eq!(Browser::from_display_name(""), None);
    assert_eq!(Browser::from_display_name("Netscape"), None);
    assert_eq!(Browser::from_display_name("None"), None);
    assert_eq!(Browser::from_display_name("brave-browser"), None);
}

#[test]
fn display_name_strings_are_canonical() {
    // Pin the exact title-cased form so the dropdown copy can't silently drift.
    assert_eq!(Browser::Brave.display_name(), "Brave");
    assert_eq!(Browser::Chrome.display_name(), "Chrome");
    assert_eq!(Browser::Chromium.display_name(), "Chromium");
    assert_eq!(Browser::Edge.display_name(), "Edge");
    assert_eq!(Browser::Firefox.display_name(), "Firefox");
    assert_eq!(Browser::Opera.display_name(), "Opera");
    assert_eq!(Browser::Safari.display_name(), "Safari");
    assert_eq!(Browser::Vivaldi.display_name(), "Vivaldi");
}

#[test]
fn variants_returns_all_eight_in_canonical_order() {
    let v = Browser::variants();
    assert_eq!(v.len(), 8);
    assert_eq!(v[0], Browser::Brave);
    assert_eq!(v[1], Browser::Chrome);
    assert_eq!(v[2], Browser::Chromium);
    assert_eq!(v[3], Browser::Edge);
    assert_eq!(v[4], Browser::Firefox);
    assert_eq!(v[5], Browser::Opera);
    assert_eq!(v[6], Browser::Safari);
    assert_eq!(v[7], Browser::Vivaldi);
}

// -- macOS detection ----------------------------------------------------------

#[cfg(target_os = "macos")]
#[test]
fn macos_detects_brave_in_system_applications() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("Applications").join("Brave Browser.app"))
        .expect("create Brave bundle");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Brave));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_detects_firefox_in_user_applications() {
    let tmp = TempDir::new().expect("tempdir");
    let user_apps = tmp.path().join("Users").join("test").join("Applications");
    std::fs::create_dir_all(user_apps.join("Firefox.app")).expect("create Firefox bundle");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Firefox));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_safari_always_detected() {
    // Safari ships with macOS — detection short-circuits to true unconditionally.
    let tmp = TempDir::new().expect("tempdir");
    let detected = detect_installed(Some(tmp.path()));
    assert!(
        detected.contains(&Browser::Safari),
        "Safari is part of macOS and must always be detected"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_empty_tempdir_yields_only_safari() {
    let tmp = TempDir::new().expect("tempdir");
    let detected = detect_installed(Some(tmp.path()));
    assert_eq!(
        detected,
        vec![Browser::Safari],
        "empty tempdir on macOS must yield only Safari"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_detects_multiple_browsers_simultaneously() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("Applications").join("Google Chrome.app"))
        .expect("create Chrome bundle");
    std::fs::create_dir_all(tmp.path().join("Applications").join("Firefox.app"))
        .expect("create Firefox bundle");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Chrome));
    assert!(detected.contains(&Browser::Firefox));
    assert!(detected.contains(&Browser::Safari));
}

// -- Linux detection ----------------------------------------------------------

#[cfg(target_os = "linux")]
#[test]
fn linux_detects_chrome_via_config_dir() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".config").join("google-chrome"))
        .expect("create chrome config");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Chrome));
    assert!(
        !detected.contains(&Browser::Safari),
        "Safari is macOS-only — must not surface on Linux"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_detects_firefox_via_snap() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(
        tmp.path()
            .join("snap")
            .join("firefox")
            .join("common")
            .join("firefox"),
    )
    .expect("create firefox snap dir");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Firefox));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_detects_brave_via_flatpak() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(
        tmp.path()
            .join(".var")
            .join("app")
            .join("com.brave.Browser"),
    )
    .expect("create brave flatpak dir");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Brave));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_empty_tempdir_yields_empty_vec() {
    let tmp = TempDir::new().expect("tempdir");
    let detected = detect_installed(Some(tmp.path()));
    assert!(
        detected.is_empty(),
        "empty tempdir on Linux must yield no browsers (got {detected:?})"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_detects_firefox_via_mozilla_config() {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".config").join(".mozilla").join("firefox"))
        .expect("create mozilla firefox config");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Firefox));
}

// -- Windows detection --------------------------------------------------------

#[cfg(target_os = "windows")]
#[test]
fn windows_detects_chrome_in_program_files() {
    let tmp = TempDir::new().expect("tempdir");
    let chrome_dir = tmp
        .path()
        .join("Program Files")
        .join("Google")
        .join("Chrome")
        .join("Application");
    std::fs::create_dir_all(&chrome_dir).expect("create chrome dir");

    let detected = detect_installed(Some(tmp.path()));
    assert!(detected.contains(&Browser::Chrome));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_empty_tempdir_yields_empty_vec() {
    let tmp = TempDir::new().expect("tempdir");
    let detected = detect_installed(Some(tmp.path()));
    assert!(
        detected.is_empty(),
        "empty tempdir on Windows must yield no browsers"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_safari_never_detected() {
    let tmp = TempDir::new().expect("tempdir");
    // Even with a Safari-named directory present, Windows detection short-
    // circuits to false for Safari.
    std::fs::create_dir_all(tmp.path().join("Program Files").join("Safari"))
        .expect("create Safari dir");
    let detected = detect_installed(Some(tmp.path()));
    assert!(!detected.contains(&Browser::Safari));
}
