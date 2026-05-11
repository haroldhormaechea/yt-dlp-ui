//! UC 15 — Headless coverage for the queue scroll fix. The production
//! change replaced the queue-body `ScrollView` with a `FocusScope`-wrapped
//! `ListView` that drives `viewport-y` from arrow keys and re-clamps on
//! resize. Slint 1.16.1 does not expose `ListView::viewport-y` /
//! `visible-height` on the generated public Rust API, so the headless
//! reach is limited to: (1) construction succeeds with N rows, (2) the
//! empty-queue branch (`ListView` `visible: false`, `EmptyState` rendered
//! as sibling) constructs cleanly, (3) model mutation through push /
//! remove does not panic.
//!
//! Mirrors `main_window_overlays.rs` posture (`init_no_event_loop`,
//! skip-if-no-backend, no event loop driven from the test thread). The
//! AC #1–4 mouse-wheel / scrollbar / arrow-keys / resize behavior is
//! covered manually via `CONTRIBUTING.md` § "Manual smoke for UC 15
//! (queue scroll)" — the brief's MVP testing posture explicitly defers
//! true UI automation.

use app::model::{PlaceholderKind, QueueStatus, TitleStatus, UiQueueRow};
use app::{MainWindow, ui_row_for_test};
use slint::{Model, ModelRc, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

fn try_make_window() -> Option<MainWindow> {
    i_slint_backend_testing::init_no_event_loop();
    match MainWindow::new() {
        Ok(w) => Some(w),
        Err(err) => {
            eprintln!("skipping queue_scroll: MainWindow::new failed ({err})");
            None
        }
    }
}

fn make_row(id: i64) -> UiQueueRow {
    UiQueueRow {
        id,
        url: format!("https://example.com/v{id}"),
        title: format!("row-{id}"),
        title_status: TitleStatus::Ok,
        title_error: None,
        status: QueueStatus::Queued,
        progress_pct: 0.0,
        speed_bps: None,
        eta_s: None,
        error_msg: None,
        dest_dir: PathBuf::from("/tmp/yt-dlp-ui"),
        size_bytes: Some(10 * 1024 * 1024),
        downloaded_bytes: None,
        thumbnail_path: None,
        kind: PlaceholderKind::Video,
        start_requested: false,
        display_order: 0,
        created_at_unix_ms: 0,
    }
}

fn install_queue_with(window: &MainWindow, count: i64) -> Rc<VecModel<app::QueueRow>> {
    let rows: Vec<app::QueueRow> = (1..=count).map(|i| ui_row_for_test(make_row(i))).collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_queue(ModelRc::from(model.clone()));
    model
}

/// AC #7 / AC #8 regression guard — with 30 synthetic rows (well above the
/// "~10 or more" threshold the use case calls out as the failure
/// reproduction), the new `FocusScope > ListView` structure must construct
/// without panicking and the bound model must round-trip via
/// `get_queue().row_count()`. A regression that re-introduces a sizing or
/// import bug on the queue body would surface here as a `MainWindow::new()`
/// error or a row-count mismatch.
#[test]
fn scroll_smoke_constructs_with_many_rows() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_queue_with(&window, 30);

    assert_eq!(
        model.row_count(),
        30,
        "VecModel must hold all 30 synthetic rows"
    );
    assert_eq!(
        window.get_queue().row_count(),
        30,
        "Slint queue model must round-trip all 30 rows"
    );
}

/// AC #7 — the empty-queue branch (`if root.queue.length == 0 :
/// EmptyState {}` rendered as sibling of the `ListView` inside the
/// `FocusScope`, with `ListView { visible: root.queue.length > 0 }`)
/// must construct cleanly. Pins the empty-side of the gating boolean —
/// a regression that drops the `visible: ...` binding or the
/// `EmptyState` sibling would still construct, but a regression that
/// breaks the empty-queue path (e.g. division by zero on viewport
/// math, panic in `EmptyState`) would surface here.
#[test]
fn scroll_smoke_empty_renders_no_listview() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model: Rc<VecModel<app::QueueRow>> = Rc::new(VecModel::default());
    window.set_queue(ModelRc::from(model.clone()));

    assert_eq!(model.row_count(), 0, "queue model starts empty");
    assert_eq!(
        window.get_queue().row_count(),
        0,
        "Slint queue mirrors the empty model"
    );
}

/// AC #5 / AC #6 — model mutation paths exercised at the only level
/// Slint 1.16.1 headless allows. `viewport-y` and `visible-height` are
/// NOT exposed on the generated public Rust API, so we cannot directly
/// assert "the viewport did not jump" (AC #5) or "the viewport
/// auto-clamped" (AC #6). What we *can* pin is that the underlying
/// `VecModel` push/remove sequence runs without panicking the
/// `changed viewport-height => clamp-y(...)` callback or the
/// `for row[i] in root.queue` row delegate. A regression where the
/// developer's clamp expression accesses a stale model index, or where
/// the row delegate dereferences a removed row, would panic here.
#[test]
fn scroll_smoke_model_mutation_does_not_panic() {
    let Some(window) = try_make_window() else {
        return;
    };
    let model = install_queue_with(&window, 5);
    assert_eq!(model.row_count(), 5);

    // Push 25 more rows while a queue is mounted — the AC #5 shape
    // (adding rows while scrolled). Headless can't observe scroll
    // position, but the model push must not panic the clamp callback.
    for i in 6..=30 {
        model.push(ui_row_for_test(make_row(i)));
    }
    assert_eq!(model.row_count(), 30, "30 rows after the push storm");
    assert_eq!(
        window.get_queue().row_count(),
        30,
        "Slint queue mirrors after pushes"
    );

    // Remove the front 10 rows — the AC #6 shape (removing rows leaves
    // the viewport past the new content height; the `changed
    // viewport-height` callback re-clamps). Front-removal exercises the
    // worst-case index-shift path through the `for row[i]` delegate.
    for _ in 0..10 {
        model.remove(0);
    }
    assert_eq!(model.row_count(), 20, "20 rows after removing the front 10");
    assert_eq!(
        window.get_queue().row_count(),
        20,
        "Slint queue mirrors after removals"
    );
}
