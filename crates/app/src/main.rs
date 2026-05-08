//! `yt-dlp-ui` binary entry point.
//!
//! Per `PROJECT_BRIEF.md` § Architecture, the real entry point lives in the
//! `app` library so integration tests can exercise the same code path.

fn main() -> Result<(), app::AppError> {
    app::run()
}
