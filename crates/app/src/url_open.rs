//! Per-OS dispatcher for opening a URL in the user's default browser.
//!
//! UC 09 uses this from the settings panel's "Read the vendor's privacy
//! policy" link. The URL is always passed as a positional argument; never
//! shelled out via `sh -c`, so there is no shell-injection surface even if
//! a future vendor URL contains odd characters.

/// Opens `url` in the user's default browser.
///
/// # Errors
///
/// Returns the [`std::io::Error`] from spawning the per-OS launcher
/// (`open`, `xdg-open`, or `cmd /C start`).
pub fn open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "url_open: unsupported OS",
        ))
    }
}
