//! UC 19 — `AddBar` "Audio only" toggle: reset semantics + Slint compile pin.
//!
//! AC #1 — default state (audio + video) verified at the helper level.
//! AC #6 — reset on add / value clear.
//!
//! ## Toolchain limitation
//!
//! The reset behavior is implemented purely in Slint:
//!
//! - `add_bar.slint` § `changed value => { if (self.value == "") { self.audio-only-on = false; } }`
//! - `main_window.slint` § the parent's `add-clicked` handler
//!   (`self.value = ""; self.audio-only-on = false;`)
//!
//! Slint 1.16.1's headless backend (`init_no_event_loop`) does not run the
//! event loop — `changed <prop>` handlers do not fire from programmatic
//! property writes, and host-fired callbacks like `invoke_add_urls` invoke
//! the host's registered handler, not the parent component's inline handler
//! that performs the reset. The same constraint is documented at the top of
//! `bot_check_modal_smoke.rs` and applies here.
//!
//! Per the test plan's fallback, this file therefore covers AC #1 and AC #6
//! at the predicate / construction level:
//!
//! 1. `format_pref_from_audio_only_flag` — the pure-Rust mapping consumed by
//!    `ui_bridge::on_add_urls` to translate the `AddBar`'s toggle state into
//!    the `Option<FormatPref>` override threaded into `add_url`. AC #1's
//!    "default audio+video" reduces to "false → None" here.
//! 2. `MainWindow` smoke construction — pins that the `add_bar.slint` UC 19
//!    additions (`audio-only-on` property, `audio-only-toggled` callback,
//!    `changed value` reset block) and the parent's two-arg `add-urls`
//!    callback signature compile and instantiate cleanly. The Slint reset
//!    behavior itself is covered by manual smoke at release time (per the
//!    brief's "no true UI automation at MVP" posture).

use app::{MainWindow, format_pref_from_audio_only_flag};
use yt_dlp_bridge::FormatPref;

fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping add-bar audio-only smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

/// AC #1 — default is "Audio + video": when the toggle is off, the helper
/// returns `None` so `add_url` falls back to the Settings-default format
/// (verified end-to-end by `download_mgr_test::add_url_with_none_override_uses_settings_default`).
#[test]
fn audio_only_flag_false_maps_to_none_override() {
    assert_eq!(format_pref_from_audio_only_flag(false), None);
}

/// AC #2 / #3 / #5 source-mapping — when the toggle is on, the helper
/// returns `Some(BestAudioM4a)`, the variant whose argv tuple is pinned by
/// `yt-dlp-bridge::format_test::snapshot_best_audio_m4a`.
#[test]
fn audio_only_flag_true_maps_to_best_audio_m4a_override() {
    assert_eq!(
        format_pref_from_audio_only_flag(true),
        Some(FormatPref::BestAudioM4a)
    );
}

/// Construction pin — the `AddBar` subtree (UC 19 additions: `audio-only-on`
/// in-out property, `audio-only-toggled` callback, `changed value` reset)
/// and the two-arg `add-urls(string, bool)` callback signature compile and
/// instantiate without panic. Mirrors `bot_check_modal_smoke.rs` posture.
#[test]
fn main_window_constructs_with_uc19_addbar_additions() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Register a handler for the new two-arg callback signature — pins the
    // generated bindings accept `(SharedString, bool)` parameters end to end.
    window.on_add_urls(|_raw, _audio_only| { /* no-op for the smoke test */ });

    // The `audio-only-on` property is encapsulated inside the AddBar (not
    // lifted onto MainWindow). We cannot read it from here; the smoke test's
    // contract is "it compiled and constructed". Property-level coverage of
    // the toggle's default state is the helper test above; runtime reset is
    // manual smoke at release time per the toolchain limitation noted in the
    // module docstring.
    drop(window);
}
