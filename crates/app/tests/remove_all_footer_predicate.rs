//! UC 12 AC #2 — Footer enable predicate for "Cancel all" and "Remove all".
//!
//! `crates/app/ui/footer.slint:79-86,98-101` widens the previous
//! "queued + active + waiting > 0" predicate to include cancelled / done /
//! error rows so the queue-clearing actions are reachable for any non-empty
//! queue:
//!
//! ```text
//! enabled = (queued + active + waiting + cancelled + done + error) > 0
//!           && !cancel-all-busy
//!           && !remove-all-busy
//! ```
//!
//! Both buttons share the predicate so they enable / disable in lock-step;
//! the busy gate is symmetric so a Cancel-all in flight disables Remove-all
//! and vice versa (no race between the two batch operations).
//!
//! The predicate lives in `.slint` markup and cannot be invoked directly
//! from a Rust integration test. We mirror it here as a pure function and
//! pin the AC #2 contract — the same mirror-the-spec convention used by
//! `footer_counts.rs` for `recompute_counts`. The headless smoke test
//! `remove_all_modal_smoke.rs` independently exercises the property writes
//! the Slint side consumes; if the rendered predicate ever drifts from this
//! mirror, manual smoke at release time will surface it.

#[derive(Debug, Clone, Copy)]
struct PredicateInputs {
    queued: i32,
    active: i32,
    waiting: i32,
    cancelled: i32,
    done: i32,
    error: i32,
    cancel_all_busy: bool,
    remove_all_busy: bool,
}

/// Mirror of the Slint footer's enable predicate for "Cancel all" and
/// "Remove all". Source of truth for the spec. AC #2.
fn enabled(i: PredicateInputs) -> bool {
    let any_row = (i.queued + i.active + i.waiting + i.cancelled + i.done + i.error) > 0;
    any_row && !i.cancel_all_busy && !i.remove_all_busy
}

fn empty() -> PredicateInputs {
    PredicateInputs {
        queued: 0,
        active: 0,
        waiting: 0,
        cancelled: 0,
        done: 0,
        error: 0,
        cancel_all_busy: false,
        remove_all_busy: false,
    }
}

#[test]
fn empty_queue_disables_both_buttons() {
    // AC #2: empty queue (every counter is zero) disables both buttons.
    assert!(!enabled(empty()));
}

#[test]
fn nonzero_queued_enables_when_idle() {
    let mut i = empty();
    i.queued = 3;
    assert!(enabled(i));
}

#[test]
fn nonzero_active_enables_when_idle() {
    let mut i = empty();
    i.active = 1;
    assert!(enabled(i));
}

#[test]
fn nonzero_waiting_enables_when_idle() {
    // `waiting` is the bot-check / waiting-on-user count; AC #2 includes it.
    let mut i = empty();
    i.waiting = 1;
    assert!(enabled(i));
}

#[test]
fn nonzero_cancelled_enables_when_idle() {
    // AC #2 widens the predicate so the user can clear a queue full of
    // cancelled rows without seeing a disabled button.
    let mut i = empty();
    i.cancelled = 5;
    assert!(enabled(i));
}

#[test]
fn nonzero_done_enables_when_idle() {
    // Mirror — done rows count toward "any row in the queue" for AC #2.
    let mut i = empty();
    i.done = 5;
    assert!(enabled(i));
}

#[test]
fn nonzero_error_enables_when_idle() {
    // Mirror — error rows count toward "any row in the queue" for AC #2.
    let mut i = empty();
    i.error = 5;
    assert!(enabled(i));
}

#[test]
fn cancel_all_busy_disables_even_with_full_queue() {
    // AC #2: while Cancel-all is mid-flight, the predicate must disable both
    // buttons (the busy gate is symmetric).
    let mut i = empty();
    i.queued = 3;
    i.active = 2;
    i.waiting = 1;
    i.cancelled = 4;
    i.done = 5;
    i.error = 2;
    i.cancel_all_busy = true;
    assert!(!enabled(i));
}

#[test]
fn remove_all_busy_disables_even_with_full_queue() {
    // AC #2 / UC 12-specific: while Remove-all is mid-flight (its own
    // confirmation dialog committed), both buttons must disable so a second
    // bulk op cannot start.
    let mut i = empty();
    i.queued = 3;
    i.active = 2;
    i.waiting = 1;
    i.cancelled = 4;
    i.done = 5;
    i.error = 2;
    i.remove_all_busy = true;
    assert!(!enabled(i));
}

#[test]
fn both_busy_disables_even_with_full_queue() {
    // Theoretically impossible (the predicate disables one when the other is
    // busy, so neither can start a sibling bulk op), but pinned defensively
    // so a regression that flips both flags simultaneously still disables.
    let mut i = empty();
    i.queued = 5;
    i.cancel_all_busy = true;
    i.remove_all_busy = true;
    assert!(!enabled(i));
}

#[test]
fn busy_gate_dominates_empty_queue() {
    // Empty queue + busy: both gates active, predicate must be false either
    // way. Sanity check that the conjunction is read correctly.
    let mut i = empty();
    i.cancel_all_busy = true;
    assert!(!enabled(i));
    let mut j = empty();
    j.remove_all_busy = true;
    assert!(!enabled(j));
}

#[test]
fn typical_mixed_queue_idle_enables_both_buttons() {
    // AC #2 happy-path — a queue with 3 queued, 1 active, 2 cancelled, and
    // 4 done rows (the most common state after a long session) enables both
    // buttons when neither bulk op is in flight.
    let i = PredicateInputs {
        queued: 3,
        active: 1,
        waiting: 0,
        cancelled: 2,
        done: 4,
        error: 0,
        cancel_all_busy: false,
        remove_all_busy: false,
    };
    assert!(enabled(i));
}
