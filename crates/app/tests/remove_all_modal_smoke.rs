//! UC 12 — Headless smoke construction of `MainWindow` with the Remove-all
//! confirmation modal mounted, exercised through every modal-related
//! property the implementation seeds. Pins that the Slint compile, the widget
//! tree, and every primitive used by the modal (the layered backdrop +
//! centered card under `if root.open :`, the body-line-2 omission `if`
//! branch, the `DButton` ghost / danger pair) construct without panic — both
//! with the modal closed and with `remove-all-confirm-open=true` — across
//! both theme polarities.
//!
//! Per the brief's MVP testing posture (Quality & Standards § Testing) and
//! mirroring the rationale in `bot_check_modal_smoke.rs` /
//! `about_modal_smoke.rs`, "no true UI automation at MVP" — driving the full
//! event loop or asserting on click-through behavior of the danger button is
//! not exposed by Slint's testing backend in 1.16.1. The same posture
//! applies to two pieces of UC 12 wiring that live entirely on the Slint
//! side:
//!
//!   1. AC #4 — ESC routing precedence (`remove-all-confirm > bot-check >
//!      about > settings`) lives in the top-level `FocusScope`
//!      `key-pressed` handler in `main_window.slint:158-…`, which only fires
//!      under the live event loop. Pinned at the property level instead:
//!      the host can write `remove-all-confirm-open = false` (the same
//!      effect the ESC handler produces) without panicking.
//!
//!   2. AC #6 — backdrop-click and Cancel-button dismissal are routed
//!      through the modal's internal `cancel()` callback, which is also
//!      event-loop bound. Pinned at the property level instead: open →
//!      close → reopen via property writes, the same observable effect the
//!      cancel routes produce.
//!
//! Both are documented in `CONTRIBUTING.UI.md`'s manual smoke addendum
//! (AC #15) so the runtime behavior gets coverage at release time.
//!
//! We deliberately do NOT run the Slint event loop (mirrors the rationale in
//! `bot_check_modal_smoke.rs`):
//!
//! - on macOS, `winit` requires the event loop on the main thread; cargo
//!   test runs each test on a worker thread, which deadlocks/panics.
//! - on Linux without an X / Wayland display, no backend is available.

use app::{DesignTokens, MainWindow};
use slint::{ComponentHandle, SharedString};

/// Helper — install the headless backend and try to construct a window.
/// Mirrors `bot_check_modal_smoke.rs::try_make_window`. Returns `None` if no
/// backend is available so the test skips cleanly on bare CI hosts rather
/// than failing — the code-under-test isn't the regression in that case.
fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping remove-all-modal smoke: MainWindow::new failed ({err})");
            None
        }
    }
}

/// Seeds the host-driven in-properties the way `ui_bridge::on_remove_all_clicked`
/// does at runtime — counts, pre-rendered body lines, danger-button label.
fn seed_typical_state(window: &MainWindow, total: i32, in_flight: i32) {
    window.set_remove_all_total_count(total);
    window.set_remove_all_in_flight_count(in_flight);
    let line_1 = format!(
        "This will remove {total} {} from the queue.",
        if total == 1 { "item" } else { "items" }
    );
    let line_2 = if in_flight == 0 {
        String::new()
    } else {
        format!(
            "{in_flight} {} still in flight and will be cancelled first. This cannot be undone.",
            if in_flight == 1 { "is" } else { "are" }
        )
    };
    window.set_remove_all_body_line_1(SharedString::from(line_1.as_str()));
    window.set_remove_all_body_line_2(SharedString::from(line_2.as_str()));
    window.set_remove_all_primary_label(SharedString::from(format!("Remove {total}").as_str()));
}

/// AC #3 + AC #5 + AC #6 — drives the modal open with a typical mixed-state
/// queue (5 total, 1 in-flight). Pins that every property write the host
/// performs (counts, body lines, label, open) is accepted by the generated
/// bindings and that the modal subtree (backdrop, centered card, ghost +
/// danger `DButton` pair) compiles and constructs.
#[test]
fn remove_all_modal_constructs_with_typical_seeded_state() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 5, 1);
    window.set_remove_all_confirm_open(true);

    // Round-trip — readbacks confirm Slint accepted every write.
    assert!(window.get_remove_all_confirm_open());
    assert_eq!(window.get_remove_all_total_count(), 5);
    assert_eq!(window.get_remove_all_in_flight_count(), 1);
    assert_eq!(window.get_remove_all_primary_label(), "Remove 5");
    assert_eq!(
        window.get_remove_all_body_line_1(),
        "This will remove 5 items from the queue."
    );
    assert_eq!(
        window.get_remove_all_body_line_2(),
        "1 is still in flight and will be cancelled first. This cannot be undone."
    );
}

/// AC #5 — the `if root.body-line-2 != ""` guard on the second `Text` must
/// collapse cleanly when K == 0. Drive the modal open with an in-flight
/// count of zero and an empty body-line-2; the modal subtree must still
/// construct and the open property must round-trip.
#[test]
fn remove_all_modal_omits_in_flight_line_when_k_zero() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 7, 0);
    window.set_remove_all_confirm_open(true);

    assert!(window.get_remove_all_confirm_open());
    assert_eq!(
        window.get_remove_all_body_line_1(),
        "This will remove 7 items from the queue."
    );
    assert_eq!(
        window.get_remove_all_body_line_2(),
        "",
        "K == 0 must yield an empty body-line-2 (AC #5)"
    );
}

/// AC #6 — Cancel button, ESC key, and backdrop click all dismiss the modal
/// without changes. The actual button / key / click routing requires the
/// live event loop, but the host-observable effect is a flip of
/// `remove-all-confirm-open` from true → false. Pin that the host can flip
/// the property repeatedly without the modal subtree getting stuck or
/// panicking.
#[test]
fn remove_all_modal_survives_open_close_open_cycle() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 3, 0);

    // First open.
    window.set_remove_all_confirm_open(true);
    assert!(window.get_remove_all_confirm_open());

    // Close — same effect ESC / Cancel button / backdrop click produce.
    window.set_remove_all_confirm_open(false);
    assert!(!window.get_remove_all_confirm_open());

    // Reopen with a different total — exercises the open path with a fresh
    // host-rendered label / body-line on every cycle. The modal has no
    // `states […]` reset block (per the .slint comments), so this also
    // pins that re-opening with new in-properties refreshes correctly.
    seed_typical_state(&window, 12, 2);
    window.set_remove_all_confirm_open(true);
    assert!(window.get_remove_all_confirm_open());
    assert_eq!(window.get_remove_all_primary_label(), "Remove 12");
    assert_eq!(
        window.get_remove_all_body_line_2(),
        "2 are still in flight and will be cancelled first. This cannot be undone."
    );

    // Final close.
    window.set_remove_all_confirm_open(false);
    assert!(!window.get_remove_all_confirm_open());
}

/// AC #12 — `DesignTokens` flip with the modal open. The modal binds every
/// visual element (surface, border, text, text-2, shadow-lg-*) to the design
/// tokens; flipping `dark-mode` while the modal is mounted must not panic
/// and must leave `remove-all-confirm-open` intact. Mirrors the theme-flip
/// posture in `bot_check_modal_smoke.rs` and `about_modal_smoke.rs`.
#[test]
fn remove_all_modal_handles_theme_flip_with_modal_open() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 4, 1);

    let tokens = window.global::<DesignTokens<'_>>();

    // Dark first.
    tokens.set_dark_mode(true);
    window.set_remove_all_confirm_open(true);
    assert!(tokens.get_dark_mode());
    assert!(window.get_remove_all_confirm_open());

    // Flip to light with the modal still open.
    tokens.set_dark_mode(false);
    assert!(!tokens.get_dark_mode());
    assert!(window.get_remove_all_confirm_open());

    // And back to dark — exercises both transition directions.
    tokens.set_dark_mode(true);
    assert!(tokens.get_dark_mode());
    assert!(window.get_remove_all_confirm_open());

    // Close the modal — exercises the inverse mount path.
    window.set_remove_all_confirm_open(false);
    assert!(!window.get_remove_all_confirm_open());
}

/// AC #13 z-order — the Remove-all confirm modal stacks above `SettingsPanel`
/// (UC 09), the About modal (UC 18), and `BotCheckModal` (UC 10). The actual
/// stacking is enforced by mount order in `main_window.slint`; this test
/// pins that the host can have all four open-state properties true
/// simultaneously without the construction panicking, which would happen if
/// any of the modal subtrees had a clashing top-level binding.
#[test]
fn remove_all_modal_constructs_alongside_other_modals_open() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 2, 0);

    // Open every modal we have. The Remove-all modal is mounted top-most so
    // it must construct and round-trip even when its peers are also open.
    window.set_settings_open(true);
    window.set_about_open(true);
    window.set_bot_check_open(true);
    window.set_remove_all_confirm_open(true);

    assert!(window.get_settings_open());
    assert!(window.get_about_open());
    assert!(window.get_bot_check_open());
    assert!(window.get_remove_all_confirm_open());

    // Close in reverse order — pins the inverse mount path also has no
    // ordering surprise.
    window.set_remove_all_confirm_open(false);
    window.set_bot_check_open(false);
    window.set_about_open(false);
    window.set_settings_open(false);

    assert!(!window.get_remove_all_confirm_open());
    assert!(!window.get_bot_check_open());
    assert!(!window.get_about_open());
    assert!(!window.get_settings_open());
}

/// Defensive case — the modal must not panic with `total-count == 0` and an
/// empty primary label. The host filters this out via the AC #2 enable
/// gate (button is disabled on an empty queue), but a regression that
/// raised `remove-all-confirm-open` with zero counts should not crash the
/// modal subtree. Pins that the empty-string `primary-button-label` and the
/// "remove 0 items" body line both render without panic.
#[test]
fn remove_all_modal_constructs_with_zero_total_count() {
    let Some(window) = try_make_window() else {
        return;
    };

    seed_typical_state(&window, 0, 0);
    window.set_remove_all_confirm_open(true);

    assert!(window.get_remove_all_confirm_open());
    assert_eq!(window.get_remove_all_total_count(), 0);
    assert_eq!(window.get_remove_all_primary_label(), "Remove 0");
    // line_1 is still rendered ("This will remove 0 items from the queue.").
    assert_eq!(
        window.get_remove_all_body_line_1(),
        "This will remove 0 items from the queue."
    );
    // K == 0 → line_2 omitted.
    assert_eq!(window.get_remove_all_body_line_2(), "");
}
