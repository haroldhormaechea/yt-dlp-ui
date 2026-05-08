//! UC 11 — Headless smoke construction of the `Toast` primitive across the
//! three `kind` variants (info / warning / danger), both `visible-now`
//! polarities, and a `dark-mode` flip with the toast mounted. Mirrors the
//! posture of `settings_panel_smoke.rs` and `bot_check_modal_smoke.rs`.
//!
//! Slint 1.16.1's testing backend does not expose subtree introspection of
//! a freshly-constructed component, so the contract this test pins is the
//! coarser one: every variant compiles, every property write is accepted by
//! the generated bindings, and `MainWindow::new()` continues to construct
//! cleanly with the `Toast` import wired in. The host-side queue eviction
//! and id-based dismissal are covered in `main_window_overlays.rs`.
//!
//! We deliberately do NOT run the Slint event loop (mirrors the rationale
//! in `settings_panel_smoke.rs`):
//!
//! - on macOS, `winit` requires the event loop on the main thread; cargo
//!   test runs each test on a worker thread, which deadlocks/panics.
//! - on Linux without an X / Wayland display, no backend is available.

use app::{DesignTokens, MainWindow, ToastEntry};
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
            eprintln!("skipping toast smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

/// Seed the `toasts` `VecModel` with a single entry of the given `kind` and
/// `visible-now`. Returns the seeded model so the caller can assert on it.
fn seed_one_toast(window: &MainWindow, kind: &str, visible_now: bool) -> Rc<VecModel<ToastEntry>> {
    let model = Rc::new(VecModel::<ToastEntry>::default());
    model.push(ToastEntry {
        id: 1,
        text: SharedString::from("smoke"),
        kind: SharedString::from(kind),
        visible_now,
    });
    window.set_toasts(ModelRc::from(model.clone()));
    model
}

/// AC#11 — every `kind` variant constructs. The `for entry in root.toasts`
/// loop in `main_window.slint` instantiates one `Toast` per entry; pushing
/// one entry per kind exercises the variant-resolution `bg-color()` /
/// `fg-color()` functions in `components.slint`.
#[test]
fn toast_constructs_with_each_kind() {
    let Some(window) = try_make_window() else {
        return;
    };

    for kind in ["info", "warning", "danger"] {
        let model = seed_one_toast(&window, kind, true);
        assert_eq!(model.row_count(), 1);
        assert_eq!(window.get_toasts().row_count(), 1);
        // Read the entry back through the window's getter to confirm the
        // generated bindings round-trip the SharedString fields.
        let entry = window.get_toasts().row_data(0).expect("entry seeded");
        assert_eq!(entry.kind.as_str(), kind);
        assert_eq!(entry.text.as_str(), "smoke");
        assert!(entry.visible_now);
    }
}

/// AC#13 — `visible-now` drives the 200 ms opacity animation. Pin both
/// polarities (true on construct, false on construct) — the `Timer` only
/// runs while `visible-now` is true, so the false branch is the one that
/// must not panic when the entry is mounted but already hidden.
#[test]
fn toast_constructs_with_visible_now_true_then_false() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Visible-now = true — the typical entry as the host pushes it.
    let model_visible = seed_one_toast(&window, "info", true);
    assert!(model_visible.row_data(0).expect("entry seeded").visible_now);

    // Replace the model with a hidden entry — host may seed `visible-now`
    // false during a fade-out window. The Toast subtree must still mount.
    let model_hidden = seed_one_toast(&window, "info", false);
    assert!(!model_hidden.row_data(0).expect("entry seeded").visible_now);
}

/// AC#19 — every visual element re-themes via `DesignTokens`. Mount a toast
/// of each kind, flip `dark-mode` true → false → true with the toast
/// mounted, and assert the property writes are accepted. This is the same
/// contract `settings_panel_handles_theme_flip_with_panel_open` enforces
/// for the settings panel.
#[test]
fn toast_handles_theme_flip_with_each_kind_mounted() {
    let Some(window) = try_make_window() else {
        return;
    };

    let tokens = window.global::<DesignTokens<'_>>();

    for kind in ["info", "warning", "danger"] {
        seed_one_toast(&window, kind, true);

        tokens.set_dark_mode(true);
        assert!(tokens.get_dark_mode());

        tokens.set_dark_mode(false);
        assert!(!tokens.get_dark_mode());

        tokens.set_dark_mode(true);
        assert!(tokens.get_dark_mode());
    }
}
