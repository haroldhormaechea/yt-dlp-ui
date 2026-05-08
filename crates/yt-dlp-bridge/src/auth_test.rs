//! Tests for [`super::is_bot_check_stderr`].
//!
//! The matcher's contract (per UC 05 AC#1, AC#2 and `auth.rs`'s module doc):
//!
//! - Recognize the canonical yt-dlp bot-check stderr by EITHER of two
//!   case-insensitive substring fragments: the `--cookies-from-browser`
//!   recommendation OR the `Sign in to confirm you're not a bot` phrase.
//! - Fall through (return `false`) on unrelated errors so the bridge surfaces
//!   them as plain [`crate::error::BridgeError::ExitedWithError`].
//! - Acceptable failure mode: a future yt-dlp message rewording silently
//!   regresses the matcher to "no match"; that's the documented contract.

use super::is_bot_check_stderr;

/// The verbatim stderr the user reported in the UC 05 chat log. This is the
/// load-bearing real-world snippet — if it ever stops matching, the entire
/// bot-check recovery flow regresses, so we pin it explicitly.
const VERBATIM_USER_STDERR: &str = "WARNING: [youtube] No supported JavaScript runtime could be found. \
You can install one of: deno (https://docs.deno.com/runtime/), \
node (https://nodejs.org/), bun (https://bun.sh/)\n\
WARNING: [youtube] No title found in player responses; falling back to title from initial data. \
Some metadata fields may be missing.\n\
ERROR: [youtube] B10ECkQXQtU: Sign in to confirm you're not a bot. \
Use --cookies-from-browser or --cookies for the authentication. \
See https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp for how to manually pass cookies. \
Also see https://github.com/yt-dlp/yt-dlp/wiki/Extractors#exporting-youtube-cookies for tips on effectively exporting YouTube cookies.";

#[test]
fn verbatim_user_stderr_matches() {
    assert!(
        is_bot_check_stderr(VERBATIM_USER_STDERR),
        "the user's reported stderr must be recognized as a bot-check"
    );
}

#[test]
fn cookies_from_browser_phrase_alone_matches() {
    let stderr = "ERROR: please use --cookies-from-browser to authenticate.";
    assert!(is_bot_check_stderr(stderr));
}

#[test]
fn sign_in_phrase_alone_matches() {
    let stderr = "ERROR: Sign in to confirm you're not a bot.";
    assert!(is_bot_check_stderr(stderr));
}

#[test]
fn region_locked_error_does_not_match() {
    // Region-locked errors do NOT recommend cookies; UC 05 § Pitfalls calls
    // this out explicitly — the matcher must NOT match.
    let stderr = "ERROR: [youtube] abc: Video unavailable. The uploader has not made this video available in your country.";
    assert!(!is_bot_check_stderr(stderr));
}

#[test]
fn http_403_error_does_not_match() {
    let stderr = "ERROR: [generic] HTTP Error 403: Forbidden";
    assert!(!is_bot_check_stderr(stderr));
}

#[test]
fn empty_stderr_does_not_match() {
    assert!(!is_bot_check_stderr(""));
}

#[test]
fn mixed_case_sign_in_phrase_matches() {
    // Case-insensitive substring match — yt-dlp could reformat capitalization
    // without rewording. Pin the case-insensitivity behavior.
    let stderr = "ERROR: Sign In To Confirm You're Not A Bot. Use cookies.";
    assert!(is_bot_check_stderr(stderr));
}

#[test]
fn multi_line_stderr_with_trigger_in_tail_matches() {
    let stderr = "WARNING: line 1\n\
                  WARNING: line 2\n\
                  WARNING: line 3\n\
                  WARNING: line 4\n\
                  ERROR: deep in the tail — Use --cookies-from-browser please.";
    assert!(is_bot_check_stderr(stderr));
}

#[test]
fn random_unrelated_text_does_not_match() {
    // Defensive: a bridge user shouldn't be able to confuse the matcher with
    // unrelated output that happens to contain English prose.
    let stderr = "Network is unreachable. Check your firewall or VPN.";
    assert!(!is_bot_check_stderr(stderr));
}

#[test]
fn cookies_flag_with_uppercase_matches() {
    // The flag itself is lowercase by yt-dlp convention but defensive case
    // folding is part of the matcher's contract.
    let stderr = "ERROR: USE --COOKIES-FROM-BROWSER.";
    assert!(is_bot_check_stderr(stderr));
}
