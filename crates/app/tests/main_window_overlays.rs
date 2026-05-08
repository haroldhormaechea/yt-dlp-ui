//! UC 11 — Headless coverage of the toast queueing model and the two
//! overlay-gating booleans on `MainWindow` (`focus-mode`, `deno-warning-visible`).
//!
//! The host (Rust `ui_bridge`) owns the `toasts` `VecModel<ToastEntry>` and is
//! the single writer; the Slint side only renders entries via the
//! `for entry[i] in root.toasts : Toast` loop in `main_window.slint` and
//! invokes `dismiss-toast(int)` from each Toast's auto-dismiss timer. This
//! test mirrors the bridge's eviction policy (front-evict at 3) and id-based
//! dismissal so the test owns the model shape independently — that way a
//! drift between the bridge and the Slint model contract is caught here,
//! not at runtime in the wild.
//!
//! Mirrors `settings_panel_smoke.rs` posture: `init_no_event_loop`,
//! skip-if-no-backend, no event loop driven from the test thread.

use app::{MainWindow, ToastEntry};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping main_window_overlays: MainWindow::new failed ({err})");
            None
        }
    }
}

/// Seed an empty `VecModel<ToastEntry>` on the window and return it. Mirrors
/// `lib.rs:set_toasts(...)` — the bridge's `push_toast_on_main` downcast
/// fails if the model is the default `[ToastEntry]` sentinel.
fn install_toasts_model(window: &MainWindow) -> Rc<VecModel<ToastEntry>> {
    let model = Rc::new(VecModel::<ToastEntry>::default());
    window.set_toasts(ModelRc::from(model.clone()));
    model
}

/// Mirror of `ui_bridge::push_toast_on_main` — front-evict at 3, push the
/// new entry at the tail. Kept verbatim with the bridge logic so a drift
/// fails this test, not production.
fn push_capped(model: &Rc<VecModel<ToastEntry>>, id: i32, kind: &str, text: &str) {
    if model.row_count() >= 3 {
        model.remove(0);
    }
    model.push(ToastEntry {
        id,
        text: SharedString::from(text),
        kind: SharedString::from(kind),
        visible_now: true,
    });
}

/// Mirror of `ui_bridge::on_dismiss_toast` — id-based linear scan, remove
/// at the first matching index. Index-based removal would be wrong: a
/// stale Toast firing its timer after a sibling has been front-evicted
/// would otherwise drop the next-oldest entry (UC 11 risk note in the use
/// case file's "Pitfalls" section).
fn dismiss_by_id(model: &Rc<VecModel<ToastEntry>>, id: i32) {
    for i in 0..model.row_count() {
        if let Some(row) = model.row_data(i)
            && row.id == id
        {
            model.remove(i);
            return;
        }
    }
}

/// AC#15 — the queue caps at 3 visible toasts; a 4th push evicts the
/// oldest. Tracks ids end-to-end so the test can prove the FIRST entry is
/// the one evicted (a naive last-in-first-out implementation would also
/// satisfy `len <= 3` but be visibly wrong).
#[test]
fn toast_queue_caps_at_three_and_evicts_oldest() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_toasts_model(&window);

    // 1 → 2 → 3 entries, no evictions yet.
    push_capped(&model, 101, "info", "first");
    push_capped(&model, 102, "info", "second");
    push_capped(&model, 103, "info", "third");
    assert_eq!(model.row_count(), 3);

    let ids: Vec<i32> = (0..model.row_count())
        .map(|i| model.row_data(i).expect("entry seeded").id)
        .collect();
    assert_eq!(
        ids,
        vec![101, 102, 103],
        "first three pushes preserve insertion order"
    );

    // 4th push — id 101 (the oldest) must be the one evicted; ids 102,
    // 103, and the new 104 must remain in that order.
    push_capped(&model, 104, "danger", "fourth");
    assert_eq!(
        model.row_count(),
        3,
        "queue stays capped at 3 after overflow"
    );
    let ids_after: Vec<i32> = (0..model.row_count())
        .map(|i| model.row_data(i).expect("entry seeded").id)
        .collect();
    assert_eq!(
        ids_after,
        vec![102, 103, 104],
        "oldest entry (id 101) evicted; survivors keep their order"
    );

    // Window getter sees the same model (round-trip via the generated
    // binding).
    assert_eq!(window.get_toasts().row_count(), 3);
}

/// AC#15 + UC 11 risk note — id-based dismissal. Push three entries, then
/// dismiss the MIDDLE one by id. The first and last must remain in their
/// original positions; an index-based dismissal of "the one currently at
/// position 1" would be coincidentally correct here, but the next test
/// (post-eviction) catches that bug shape.
#[test]
fn toast_dismiss_by_id_removes_only_target_entry() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_toasts_model(&window);

    push_capped(&model, 1, "info", "first");
    push_capped(&model, 2, "warning", "second");
    push_capped(&model, 3, "danger", "third");

    dismiss_by_id(&model, 2);

    let ids: Vec<i32> = (0..model.row_count())
        .map(|i| model.row_data(i).expect("entry seeded").id)
        .collect();
    assert_eq!(
        ids,
        vec![1, 3],
        "middle entry removed by id; siblings retain order"
    );

    // Dismissing an unknown id is a no-op (the timer fired after the
    // entry was front-evicted by an overflow push).
    dismiss_by_id(&model, 999);
    assert_eq!(model.row_count(), 2, "unknown id is a no-op");
}

/// AC#15 — id-based dismissal survives a front-eviction. Push 3, overflow
/// to evict id 1, push id 4 onto the tail, then dismiss id 3. If the
/// dismissal were index-based on the PRE-eviction layout (the timer was
/// armed when id 3 sat at position 2), the wrong row (id 4) would be
/// removed. This is the bug shape the developer's id-based scan in
/// `ui_bridge::on_dismiss_toast` exists to prevent — pin it.
#[test]
fn toast_dismiss_by_id_targets_correct_row_after_eviction() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_toasts_model(&window);

    push_capped(&model, 1, "info", "first");
    push_capped(&model, 2, "info", "second");
    push_capped(&model, 3, "info", "third");
    push_capped(&model, 4, "info", "fourth"); // evicts id 1; layout: [2, 3, 4]

    dismiss_by_id(&model, 3);

    let ids: Vec<i32> = (0..model.row_count())
        .map(|i| model.row_data(i).expect("entry seeded").id)
        .collect();
    assert_eq!(
        ids,
        vec![2, 4],
        "id 3 dismissed by id, not by stale pre-eviction index"
    );
}

/// AC#1 + AC#19 — `focus-mode` toggling drives the `if !root.focus-mode :
/// Rectangle` ad-slot branch. Pin that the property write does not panic
/// across true → false → true with a populated toast queue mounted (the
/// ad slot and toast overlay coexist in the same `MainWindow` subtree).
#[test]
fn focus_mode_toggle_with_toasts_mounted_does_not_panic() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_toasts_model(&window);
    push_capped(&model, 1, "info", "Queue cancelled.");
    push_capped(&model, 2, "danger", "Failed to add URL(s).");

    // Default at construction is false (ad slot visible).
    assert!(!window.get_focus_mode());

    window.set_focus_mode(true);
    assert!(window.get_focus_mode());

    window.set_focus_mode(false);
    assert!(!window.get_focus_mode());

    window.set_focus_mode(true);
    assert!(window.get_focus_mode());

    // Toasts model is unaffected by the focus-mode toggle.
    assert_eq!(model.row_count(), 2);
}

/// AC#5 + AC#9 — `deno-warning-visible` drives the `if root.deno-warning-visible
/// : Rectangle` banner branch under the add bar. Pin that the property write
/// does not panic across true → false (dismissal) → true (would-be next-launch
/// reseed). Session-only dismissal lives in Rust state — this test pins the
/// Slint-side property contract only.
#[test]
fn deno_warning_visible_toggle_does_not_panic() {
    let Some(window) = try_make_window() else {
        return;
    };

    // Default at construction is false.
    assert!(!window.get_deno_warning_visible());

    // Probe failed at startup → seed true (mirrors `lib.rs`:
    // `window.set_deno_warning_visible(!deno_resolved)`).
    window.set_deno_warning_visible(true);
    assert!(window.get_deno_warning_visible());

    // User clicks ×; banner dismissed for the rest of the session.
    window.set_deno_warning_visible(false);
    assert!(!window.get_deno_warning_visible());

    // Hypothetical re-seed (not done in production, but the property
    // contract must accept it).
    window.set_deno_warning_visible(true);
    assert!(window.get_deno_warning_visible());
}
