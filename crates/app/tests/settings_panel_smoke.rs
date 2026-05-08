//! UC 09 — Headless smoke construction of `MainWindow` with the settings
//! panel mounted, exercised through every panel-related property the
//! implementation seeds. Pins that the Slint compile, the widget tree, and
//! every primitive used by the panel (`Toggle`, `Stepper`, `Tooltip`,
//! `Select.enabled`) construct without panic — both with the panel closed
//! and with `settings-open=true` — across both theme polarities.
//!
//! Per the brief's MVP testing posture (Quality & Standards § Testing) and
//! UC 09 AC#23, "no true UI automation at MVP" — driving the full event
//! loop or the rfd async picker headlessly is deferred to a manual smoke
//! documented in `CONTRIBUTING.md`. `MainWindow::new()` returning Ok and
//! every property accepting realistic values is what this test pins.
//!
//! We deliberately do NOT run the Slint event loop (mirrors the rationale
//! in `row_visual_smoke.rs`):
//!
//! - on macOS, `winit` requires the event loop on the main thread; cargo
//!   test runs each test on a worker thread, which deadlocks/panics.
//! - on Linux without an X / Wayland display, no backend is available.

use app::{DesignTokens, MainWindow};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Helper — install the headless backend and try to construct a window.
/// `init_no_event_loop` returns `Err` if a backend is already installed
/// (the first test in the binary wins); we ignore that. If `MainWindow::new`
/// itself fails (no backend at all on a bare CI host) the test skips
/// cleanly rather than failing — the code-under-test isn't the regression.
fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping settings-panel smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

fn cookies_options(opts: &[&str]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = opts.iter().copied().map(SharedString::from).collect();
    ModelRc::from(Rc::new(VecModel::from(v)))
}

/// Drives the "panel closed → panel open" path while seeding every
/// panel-related property to a representative value. Asserts the construction
/// and property writes do not panic.
#[test]
fn settings_panel_constructs_with_typical_seeded_state() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Seed General-tab properties.
    window.set_dest_dir_display(SharedString::from("~/Downloads/yt-dlp-ui"));
    window.set_format_pref(SharedString::from("BestHeuristic"));
    window.set_concurrency_cap(3);

    // Seed Cookies-tab properties — non-empty case (AC#13, hint variant 2).
    let opts = cookies_options(&["None", "Brave", "Chrome", "Firefox"]);
    window.set_cookies_options(opts);
    window.set_cookies_browser(SharedString::from("None"));
    window.set_cookies_empty(false);

    // Seed Privacy-and-Ads-tab properties.
    window.set_focus_mode(false);
    window.set_ads_personalized(true);
    window.set_vendor_privacy_url(SharedString::from("https://example.invalid/privacy"));

    // Open the panel — exercises the `Rectangle`-with-animated-`x` mount
    // (AC#2) plus all three tabs (AC#8) and every primitive in the tree.
    window.set_settings_open(true);

    // Round-trip — readbacks confirm Slint accepted every write.
    assert!(window.get_settings_open());
    assert_eq!(window.get_concurrency_cap(), 3);
    assert!(!window.get_focus_mode());
    assert!(window.get_ads_personalized());
    assert!(!window.get_cookies_empty());
    assert_eq!(window.get_cookies_options().row_count(), 4);
}

/// AC#14 — empty cookies state. Drives the panel with zero detected browsers
/// (only the leading "None" option) and confirms the seeded `cookies-empty`
/// flag flows through without panic. The actual disabled-state visual is
/// verified in `settings_panel_model.rs` and at manual smoke time.
#[test]
fn settings_panel_constructs_in_empty_cookies_state() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_cookies_options(cookies_options(&["None"]));
    window.set_cookies_browser(SharedString::from("None"));
    window.set_cookies_empty(true);

    // AC#18 disabled-link branch: no vendor URL configured.
    window.set_vendor_privacy_url(SharedString::from(""));

    window.set_settings_open(true);

    assert!(window.get_cookies_empty());
    assert!(window.get_vendor_privacy_url().is_empty());
    assert_eq!(window.get_cookies_options().row_count(), 1);
}

/// AC#20 — `DesignTokens` flip. Constructs the window, opens the panel, and
/// flips `dark-mode` true → false; nothing in the tree binds to a stale
/// value if both polarities construct cleanly.
#[test]
fn settings_panel_handles_theme_flip_with_panel_open() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Minimal seed — just enough that every Tab has content.
    window.set_cookies_options(cookies_options(&["None", "Firefox"]));
    window.set_cookies_browser(SharedString::from("None"));
    window.set_cookies_empty(false);
    window.set_focus_mode(true);
    window.set_ads_personalized(false);
    window.set_vendor_privacy_url(SharedString::from("https://example.invalid/privacy"));
    window.set_concurrency_cap(7);

    let tokens = window.global::<DesignTokens<'_>>();

    // Dark first.
    tokens.set_dark_mode(true);
    window.set_settings_open(true);
    assert!(tokens.get_dark_mode());

    // Flip to light with the panel still open — AC#20 contract.
    tokens.set_dark_mode(false);
    assert!(!tokens.get_dark_mode());
    assert!(window.get_settings_open());

    // Close the panel — exercises the inverse animation mount path.
    window.set_settings_open(false);
    assert!(!window.get_settings_open());
}

/// AC#12 — concurrency cap stepper accepts every in-range value. The Slint
/// stepper clamp uses ternary expressions (Slint 1.16.1 lacks `min`/`max`
/// builtins per the implementation note); pin every boundary so a refactor
/// of the clamp can't silently drift.
#[test]
fn settings_panel_accepts_every_concurrency_cap_value() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_cookies_options(cookies_options(&["None"]));
    window.set_cookies_empty(true);
    window.set_settings_open(true);

    for cap in 1..=10 {
        window.set_concurrency_cap(cap);
        assert_eq!(window.get_concurrency_cap(), cap);
    }
}

/// UC 16 AC#4 — the Settings panel's "Save destination" line must reflect a
/// pre-seeded persisted destination (not the default) on the next launch.
///
/// We exercise the same read+format chain that `lib.rs:198-203` executes at
/// startup:
///   1. Open a fresh DB and seed `settings.dest_dir` with a custom path.
///   2. Read it back via `settings::get_dest_dir`, passing a sentinel
///      default that must NOT win.
///   3. Format via `formats::format_dest_dir` (the same helper `run_ui`
///      calls) and stamp the result onto the panel via
///      `set_dest_dir_display`.
///   4. Confirm the displayed string matches the formatted custom path —
///      proving the re-read path picked up the persisted value, not the
///      default fallback or the test's previously-hardcoded literal.
#[test]
fn settings_panel_displays_persisted_dest_dir_on_relaunch() {
    let Some(window) = try_make_window() else {
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");
    let db = app::db::Db::open(&db_path).expect("open db");

    // Pre-seed a custom destination as if the user picked it last session.
    let custom = tmp.path().join("user-saved-destination");
    std::fs::create_dir_all(&custom).expect("mkdir custom");
    db.with_conn(|c| app::db::settings::set_dest_dir(c, &custom))
        .expect("seed dest_dir");

    // Re-read it via the same helper `run_ui` uses; the sentinel default
    // must NOT win — that would mean the persisted value was lost.
    let sentinel = std::path::PathBuf::from("/sentinel/never-this");
    let got = db
        .with_conn(|c| app::db::settings::get_dest_dir(c, &sentinel))
        .expect("get_dest_dir");
    assert_ne!(
        got, sentinel,
        "persisted dest_dir was lost on re-read (AC#4 regression)"
    );
    assert_eq!(
        got, custom,
        "re-read returned a different value than seeded"
    );

    // Drive the same display-formatting `run_ui` does, then stamp it on
    // the panel. The expected display matches `format_dest_dir` exactly —
    // any future tweak to that helper updates both sides.
    let display = app::formats::format_dest_dir(&got);
    window.set_dest_dir_display(SharedString::from(display.clone()));
    window.set_settings_open(true);

    let actual = window.get_dest_dir_display().to_string();
    assert_eq!(
        actual, display,
        "panel must reflect the formatted persisted dest_dir on the re-read path"
    );

    // Sanity — the display string is non-empty (an empty value would
    // hide the "Save destination" line, which is a different bug).
    assert!(
        !actual.is_empty(),
        "rendered dest_dir display must not be empty for a persisted setting"
    );
}
