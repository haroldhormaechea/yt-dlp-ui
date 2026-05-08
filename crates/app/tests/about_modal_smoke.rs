//! UC 18 — Headless smoke construction of `MainWindow` with the About
//! modal mounted, exercised through every modal-related property the
//! implementation seeds. Pins that the Slint compile, the widget tree, and
//! every primitive used by the modal (the entries `for`-loop with the
//! per-row "View full license" `DButton`, the conditional `view == "summary"`
//! / `view == "license"` mounts, the source-notice `if` branch, the
//! `ScrollView` with min/max-height-locked license body) construct without
//! panic — both with the modal closed and with `about-open=true` — across
//! both theme polarities.
//!
//! Per the brief's MVP testing posture (Quality & Standards § Testing) and
//! mirroring the rationale in `bot_check_modal_smoke.rs` /
//! `settings_panel_smoke.rs`, "no true UI automation at MVP" — driving the
//! full event loop or asserting on the modal's internal `view` /
//! `current-entry-index` post-transition values is not exposed by Slint's
//! testing backend in 1.16.1. The same posture applies to two pieces of
//! UC 18 wiring that live entirely on the Slint side:
//!
//!   1. AC#9 — the Settings panel's "About yt-dlp-ui" row's `about-clicked`
//!      callback flips the host's `about-open` to true via
//!      `main_window.slint:471`. The `about-clicked` callback is internal to
//!      the `SettingsPanel` component and is NOT exposed as a top-level
//!      callback on `MainWindow`, so the test cannot invoke it from outside
//!      the event loop. Pinned at the property level instead: a host write
//!      to `set_about_open(true)` (the same effect the Slint wire produces)
//!      round-trips correctly and the modal subtree mounts.
//!
//!   2. AC#11 — Esc-key dismissal is routed by the top-level `FocusScope`
//!      `key-pressed` handler in `main_window.slint:141-144`, which only
//!      fires under the live event loop. Pinned at the property level
//!      instead: open → close → reopen via property writes, which is the
//!      same observable effect the Esc handler produces.
//!
//! Both are documented in `CONTRIBUTING.md` § "Manual smoke for UC 18
//! (About dialog)" so the runtime behavior gets coverage at release time.
//!
//! We deliberately do NOT run the Slint event loop (mirrors the rationale
//! in `bot_check_modal_smoke.rs`):
//!
//! - on macOS, `winit` requires the event loop on the main thread; cargo
//!   test runs each test on a worker thread, which deadlocks/panics.
//! - on Linux without an X / Wayland display, no backend is available.

use app::{AboutEntry, DesignTokens, MainWindow};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// Helper — install the headless backend and try to construct a window.
/// Mirrors `bot_check_modal_smoke.rs::try_make_window`. Returns `None` if no
/// backend is available so the test skips cleanly on bare CI hosts rather
/// than failing — the code-under-test isn't the regression in that case.
fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping about-modal smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

/// Build a representative entries model that mirrors the real
/// `app::about::entries()` shape (six entries, ffmpeg carrying a non-empty
/// source-notice). Uses static-lifetime strings so the test doesn't have to
/// import the full `app::about` module just to forward bytes.
fn seed_entries(window: &MainWindow) {
    let entries = vec![
        AboutEntry {
            name: SharedString::from("yt-dlp-ui"),
            version: SharedString::from("0.5.0"),
            license_name: SharedString::from("PolyForm Noncommercial 1.0.0"),
            license_text: SharedString::from("PolyForm Noncommercial license body…"),
            source_notice: SharedString::from(""),
        },
        AboutEntry {
            name: SharedString::from("yt-dlp"),
            version: SharedString::from("2025.10.26"),
            license_name: SharedString::from("Unlicense"),
            license_text: SharedString::from("This is free and unencumbered software…"),
            source_notice: SharedString::from(""),
        },
        AboutEntry {
            name: SharedString::from("deno"),
            version: SharedString::from("2.5.6"),
            license_name: SharedString::from("MIT"),
            license_text: SharedString::from("Copyright (c) 2018-Present the Deno authors…"),
            source_notice: SharedString::from(""),
        },
        AboutEntry {
            name: SharedString::from("ffmpeg"),
            version: SharedString::from("7.1"),
            license_name: SharedString::from("LGPL-2.1-or-later"),
            license_text: SharedString::from("GNU LESSER GENERAL PUBLIC LICENSE…"),
            source_notice: SharedString::from(
                "Source available at: https://ffmpeg.org/ — see scripts/build-ffmpeg-macos.sh for the rebuild recipe",
            ),
        },
        AboutEntry {
            name: SharedString::from("Inter"),
            version: SharedString::from("Variable"),
            license_name: SharedString::from("SIL OFL 1.1"),
            license_text: SharedString::from("SIL OPEN FONT LICENSE Version 1.1…"),
            source_notice: SharedString::from(""),
        },
        AboutEntry {
            name: SharedString::from("JetBrains Mono"),
            version: SharedString::from("Variable"),
            license_name: SharedString::from("SIL OFL 1.1"),
            license_text: SharedString::from("SIL OPEN FONT LICENSE Version 1.1…"),
            source_notice: SharedString::from(""),
        },
    ];
    window.set_about_entries(ModelRc::from(Rc::new(VecModel::from(entries))));
}

/// AC#2 + AC#10 — drives the modal open with seeded version + entries.
/// Pins that every property write the host performs (`app-version`,
/// `about-entries`, `about-open=true`) is accepted by the generated bindings
/// and that the modal subtree (entries `for`-loop, per-row "View full
/// license" button, header / footer chrome) compiles and constructs.
#[test]
fn about_modal_constructs_with_typical_seeded_state() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_app_version(SharedString::from("0.5.0"));
    seed_entries(&window);

    // Open the modal.
    window.set_about_open(true);

    // Round-trip — readbacks confirm Slint accepted every write.
    assert!(window.get_about_open());
    assert_eq!(window.get_app_version(), "0.5.0");
    assert_eq!(window.get_about_entries().row_count(), 6);
}

/// AC#11 (Esc / Close / backdrop dismiss) — the actual key-event routing
/// requires the live event loop, but the host-observable effect is a flip
/// of `about-open` from true → false. Pin that the host can flip the
/// property repeatedly without the modal subtree getting stuck or panicking.
/// This is the same posture `bot_check_modal_smoke.rs` takes for its
/// equivalent `states […]` reset coverage.
#[test]
fn about_modal_survives_open_close_open_cycle() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_app_version(SharedString::from("0.5.0"));
    seed_entries(&window);

    // First open.
    window.set_about_open(true);
    assert!(window.get_about_open());

    // Close — same effect Esc / Close button / backdrop click produce.
    window.set_about_open(false);
    assert!(!window.get_about_open());

    // Reopen — exercises the `states […]` reset path on `about_modal.slint`
    // that re-declares `view = "summary"` and `current-entry-index = 0`
    // on the closed → open-state transition.
    window.set_about_open(true);
    assert!(window.get_about_open());

    // Final close.
    window.set_about_open(false);
    assert!(!window.get_about_open());
}

/// AC#9 — the Settings panel's "About yt-dlp-ui" row triggers the modal.
/// The Slint wire (`main_window.slint:471 about-clicked => { root.about-open
/// = true; }`) is between two child components and is NOT reachable as a
/// top-level invokable callback on `MainWindow`. Pin the host-observable
/// equivalent: when a setter emulates the Slint wire's effect, the modal
/// reflects the state. Manual smoke covers the actual click in
/// `CONTRIBUTING.md`.
#[test]
fn about_modal_opens_when_host_emulates_settings_about_click() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_app_version(SharedString::from("0.5.0"));
    seed_entries(&window);

    // Settings panel is open (mirrors the user landing on it before
    // clicking "About"); about-modal sits above it in the z-stack per
    // `main_window.slint:474-490`.
    window.set_settings_open(true);

    // The Slint wire `about-clicked => { root.about-open = true; }`
    // produces this exact effect at runtime.
    window.set_about_open(true);

    assert!(window.get_about_open());
    assert!(window.get_settings_open());
}

/// AC#10 — `DesignTokens` flip with the modal open. The modal honors the
/// design-system tokens (surface, border, divider, accent-text); flipping
/// `dark-mode` while the modal is mounted must not panic and must leave the
/// `about-open` write intact. Mirrors `bot_check_modal_smoke.rs`'s
/// theme-flip coverage.
#[test]
fn about_modal_handles_theme_flip_with_modal_open() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_app_version(SharedString::from("0.5.0"));
    seed_entries(&window);

    let tokens = window.global::<DesignTokens<'_>>();

    // Dark first.
    tokens.set_dark_mode(true);
    window.set_about_open(true);
    assert!(tokens.get_dark_mode());
    assert!(window.get_about_open());

    // Flip to light with the modal still open.
    tokens.set_dark_mode(false);
    assert!(!tokens.get_dark_mode());
    assert!(window.get_about_open());

    // And back to dark — exercises both transition directions.
    tokens.set_dark_mode(true);
    assert!(tokens.get_dark_mode());

    // Close the modal — exercises the inverse mount path.
    window.set_about_open(false);
    assert!(!window.get_about_open());
}

/// Defensive case — the modal must not panic if it is ever opened with an
/// empty entries list. The host always seeds at least the project's own
/// entry (`app::about::entries()` has six static entries), but a regression
/// that drops the seed call should not crash the modal subtree. Pins the
/// empty-`for`-body branch and the `entries.length == 0` short-circuit in
/// the per-current-entry helper functions
/// (`about_modal.slint:345-365`).
#[test]
fn about_modal_constructs_with_empty_entries() {
    let Some(window) = try_make_window() else {
        return;
    };

    window.set_app_version(SharedString::from("0.5.0"));
    window.set_about_entries(ModelRc::from(Rc::new(VecModel::<AboutEntry>::default())));

    window.set_about_open(true);

    assert!(window.get_about_open());
    assert_eq!(window.get_about_entries().row_count(), 0);
}
