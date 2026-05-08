//! UC 10 — Headless smoke construction of `MainWindow` with the bot-check
//! modal mounted, exercised through every modal-related property the
//! implementation seeds. Pins that the Slint compile, the widget tree, and
//! every primitive used by the modal (`CheckBox`, the layered `BrowserRow`,
//! the gradient glyph stack, the `states […]` fallback for property reset)
//! construct without panic — both with the modal closed and with
//! `bot-check-open=true` — across both theme polarities and the
//! affected-count copy variants (1 vs N) called out by AC#4.
//!
//! Per the brief's MVP testing posture (Quality & Standards § Testing) and
//! mirroring the rationale in `settings_panel_smoke.rs`, "no true UI
//! automation at MVP" — driving the full event loop or asserting on the
//! state-bound `picked` / `remember` post-transition values from outside
//! the modal is not exposed by Slint's testing backend in 1.16.1. The
//! developer's `states […]` reset fallback is therefore validated at the
//! property-write level (open → close → reopen does not panic and the
//! host-supplied `bot-check-default-pick` is honored on each reopen),
//! plus a manual smoke entry in `CONTRIBUTING.md`.
//!
//! We deliberately do NOT run the Slint event loop (mirrors the rationale
//! in `row_visual_smoke.rs` / `settings_panel_smoke.rs`):
//!
//! - on macOS, `winit` requires the event loop on the main thread; cargo
//!   test runs each test on a worker thread, which deadlocks/panics.
//! - on Linux without an X / Wayland display, no backend is available.

use app::{DesignTokens, MainWindow};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Helper — install the headless backend and try to construct a window.
/// Mirrors `settings_panel_smoke.rs::try_make_window`. Returns `None` if no
/// backend is available so the test skips cleanly on bare CI hosts rather
/// than failing — the code-under-test isn't the regression in that case.
fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping bot-check-modal smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

fn options_model(opts: &[&str]) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = opts.iter().copied().map(SharedString::from).collect();
    ModelRc::from(Rc::new(VecModel::from(v)))
}

/// Seed every per-browser `bot-check-has-*` flag from a slice of yt-dlp arg
/// names — mirrors the dispatcher logic in `ui_bridge::ShowBotCheckDialog`.
fn set_has_flags(window: &MainWindow, available: &[&str]) {
    window.set_bot_check_has_brave(available.contains(&"brave"));
    window.set_bot_check_has_chrome(available.contains(&"chrome"));
    window.set_bot_check_has_chromium(available.contains(&"chromium"));
    window.set_bot_check_has_edge(available.contains(&"edge"));
    window.set_bot_check_has_firefox(available.contains(&"firefox"));
    window.set_bot_check_has_opera(available.contains(&"opera"));
    window.set_bot_check_has_safari(available.contains(&"safari"));
    window.set_bot_check_has_vivaldi(available.contains(&"vivaldi"));
}

/// AC#1 + AC#5-7 + AC#9 — drives the modal open with a typical multi-row
/// affected-count and a multi-browser picker. Pins that every property write
/// the dispatcher performs (options model, eight `has-*` flags, default-pick,
/// affected-count, open) is accepted by the generated bindings and that the
/// modal subtree (`BrowserRow`, glyph stack, `CheckBox`, primary button) compiles
/// and constructs.
#[test]
fn bot_check_modal_constructs_with_typical_seeded_state() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Three detected browsers in canonical order — matches the host's
    // canonical filtering (Brave > Chrome > Firefox).
    let opts = options_model(&["brave", "chrome", "firefox"]);
    window.set_bot_check_options(opts);
    set_has_flags(&window, &["brave", "chrome", "firefox"]);

    // AC#4 — multi-row affected-count copy variant.
    window.set_bot_check_affected_count(5);
    // AC#12 — first open of a session: default-pick is the first detected.
    window.set_bot_check_default_pick(SharedString::from("brave"));
    window.set_bot_check_last_pick(SharedString::from(""));

    window.set_bot_check_open(true);

    // Round-trip — readbacks confirm Slint accepted every write.
    assert!(window.get_bot_check_open());
    assert_eq!(window.get_bot_check_affected_count(), 5);
    assert_eq!(window.get_bot_check_default_pick(), "brave");
    assert!(window.get_bot_check_has_brave());
    assert!(window.get_bot_check_has_chrome());
    assert!(window.get_bot_check_has_firefox());
    assert!(!window.get_bot_check_has_safari());
    assert_eq!(window.get_bot_check_options().row_count(), 3);
}

/// AC#4 — singular-vs-plural copy. The Slint `if` branch on `affected-count`
/// drives the trailing "This applies to <N> queued items." fragment; both
/// branches must construct cleanly, including the count = 1 case where the
/// trailing fragment is suppressed by the layout. Drive both polarities back-
/// to-back through one open modal so any condition-bound subtree mounts.
#[test]
fn bot_check_modal_handles_affected_count_singular_and_plural() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_bot_check_options(options_model(&["chrome"]));
    set_has_flags(&window, &["chrome"]);
    window.set_bot_check_default_pick(SharedString::from("chrome"));
    window.set_bot_check_open(true);

    // Singular — count = 1.
    window.set_bot_check_affected_count(1);
    assert_eq!(window.get_bot_check_affected_count(), 1);

    // Plural — count = 5 (the "queued items" fragment renders).
    window.set_bot_check_affected_count(5);
    assert_eq!(window.get_bot_check_affected_count(), 5);

    // Larger N — three-digit count must still survive the i32 binding.
    window.set_bot_check_affected_count(123);
    assert_eq!(window.get_bot_check_affected_count(), 123);
}

/// AC#13 zero-browsers defensive case — the host filters this branch out
/// before raising `bot-check-open`, but the modal must not panic if it is
/// ever opened with an empty options list. Pins that the empty `for browser
/// in bot-check-options` body collapses cleanly and the open-state property
/// write is accepted.
#[test]
fn bot_check_modal_constructs_with_empty_options() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_bot_check_options(options_model(&[]));
    set_has_flags(&window, &[]);
    window.set_bot_check_affected_count(1);
    window.set_bot_check_default_pick(SharedString::from(""));
    window.set_bot_check_open(true);

    assert!(window.get_bot_check_open());
    assert_eq!(window.get_bot_check_options().row_count(), 0);
}

/// AC#14 — `DesignTokens` flip. Constructs the window, opens the modal,
/// and flips `dark-mode` true → false; nothing in the modal subtree binds
/// to a stale value if both polarities construct cleanly. Mirrors the
/// settings-panel theme-flip smoke, but for the modal subtree (gradient
/// glyphs, accent-soft selected-row background, divider lines, primary
/// button).
#[test]
fn bot_check_modal_handles_theme_flip_with_modal_open() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_bot_check_options(options_model(&["brave", "firefox"]));
    set_has_flags(&window, &["brave", "firefox"]);
    window.set_bot_check_affected_count(2);
    window.set_bot_check_default_pick(SharedString::from("brave"));

    let tokens = window.global::<DesignTokens<'_>>();

    // Dark first.
    tokens.set_dark_mode(true);
    window.set_bot_check_open(true);
    assert!(tokens.get_dark_mode());
    assert!(window.get_bot_check_open());

    // Flip to light with the modal still open — AC#14 contract.
    tokens.set_dark_mode(false);
    assert!(!tokens.get_dark_mode());
    assert!(window.get_bot_check_open());

    // And back to dark — exercises both transition directions.
    tokens.set_dark_mode(true);
    assert!(tokens.get_dark_mode());

    // Close the modal — exercises the inverse mount path.
    window.set_bot_check_open(false);
    assert!(!window.get_bot_check_open());
}

/// AC#12 — close → reopen pins the developer's `states […]` reset fallback
/// at the property-write level. Slint 1.16.1's `<prop>-changed(value) => …`
/// callback syntax does not work as a Rectangle property reset hook in this
/// version; the implementation falls back to a state-machine that re-declares
/// `picked = root.default-pick` and `remember = false` whenever `open`
/// transitions from `closed` → `open-state`. We cannot read the modal's
/// internal `picked` / `remember` from the test (they are not exposed as
/// in-out props on `MainWindow`), so this test pins the host-observable
/// invariant: the host can flip open false → true repeatedly with different
/// `default-pick` values and the property writes round-trip correctly.
#[test]
fn bot_check_modal_survives_open_close_open_with_different_default_pick() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_bot_check_options(options_model(&["brave", "chrome", "firefox"]));
    set_has_flags(&window, &["brave", "chrome", "firefox"]);
    window.set_bot_check_affected_count(1);

    // First open — first-detected is the default.
    window.set_bot_check_default_pick(SharedString::from("brave"));
    window.set_bot_check_open(true);
    assert!(window.get_bot_check_open());
    assert_eq!(window.get_bot_check_default_pick(), "brave");

    // Close — the modal subtree should unmount or hide cleanly.
    window.set_bot_check_open(false);
    assert!(!window.get_bot_check_open());

    // Second open — default-pick now reflects a session-cached last-pick.
    // (Drives the `states […]` open-state entry path with a different
    // declared-value branch; per AC#12 the modal honors this on each
    // open without leaking state from the prior cycle.)
    window.set_bot_check_default_pick(SharedString::from("chrome"));
    window.set_bot_check_last_pick(SharedString::from("chrome"));
    window.set_bot_check_open(true);
    assert!(window.get_bot_check_open());
    assert_eq!(window.get_bot_check_default_pick(), "chrome");

    // Third open — verify the host can swap to yet another browser without
    // the modal getting stuck on a prior pick at the property level.
    window.set_bot_check_open(false);
    window.set_bot_check_default_pick(SharedString::from("firefox"));
    window.set_bot_check_last_pick(SharedString::from("firefox"));
    window.set_bot_check_open(true);
    assert_eq!(window.get_bot_check_default_pick(), "firefox");
}
