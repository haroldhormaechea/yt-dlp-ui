//! `YouTube` bot-check stderr matcher.
//!
//! The matcher is heuristic, pinned to yt-dlp's *current* stderr phrasing, and
//! NOT a stable upstream contract. Two phrase fragments are recognized:
//!
//! - `--cookies-from-browser` — yt-dlp's own actionable suggestion. This is the
//!   primary signal: region-locked, age-gated, and other access errors do NOT
//!   recommend cookies, so a substring hit here has effectively zero false
//!   positive risk for unrelated failures.
//! - `sign in to confirm you're not a bot` — the canonical user-facing phrase.
//!   Corroborates the primary signal and catches drift if yt-dlp ever stops
//!   emitting the cookies recommendation but keeps the bot phrase.
//!
//! Both checks are case-insensitive substring matches on the stderr tail.
//!
//! If yt-dlp reworks its messaging and neither phrase matches, the bridge
//! falls through to [`crate::error::BridgeError::ExitedWithError`] — the
//! pre-UC-05 generic-error experience, which is the documented acceptable
//! failure mode for this matcher (per UC 05 § Pitfalls — "Bot-check error
//! pattern in stderr is not a stable yt-dlp contract").

/// Returns `true` when `stderr_tail` looks like yt-dlp's `YouTube` bot-check error.
pub(crate) fn is_bot_check_stderr(stderr_tail: &str) -> bool {
    let lc = stderr_tail.to_lowercase();
    lc.contains("--cookies-from-browser") || lc.contains("sign in to confirm you're not a bot")
}

#[cfg(test)]
#[path = "auth_test.rs"]
mod auth_tests;
