//! yt-dlp-ui — ad-window helper.
//!
//! This is a scaffold stub. The real entry point will:
//! 1. Initialize a minimal `tracing_subscriber` writing to stderr (so `app`'s
//!    stdout pipe stays clean for the JSON IPC protocol).
//! 2. Read newline-delimited JSON commands from stdin (see `PROJECT_BRIEF.md`
//!    § Architecture § Ad-window lifecycle for the protocol).
//! 3. Build a `tao::EventLoop` and a `wry::WebView` window when a `show`
//!    command arrives; tear it down on `hide` or `shutdown`.
//! 4. Emit JSON events back to `app` over stdout (`ready`, `click`, `error`).
//!
//! Trust boundary: this process must NOT open the `SQLite` database, must NOT
//! spawn further subprocesses, must NOT read files outside its own per-process
//! `WebView` cache directory. See `PROJECT_BRIEF.md` § Architecture § Trust
//! boundaries.

use tracing::info;

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "ad-window starting");
    info!("scaffold stub — webview not implemented yet; exiting cleanly");
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
