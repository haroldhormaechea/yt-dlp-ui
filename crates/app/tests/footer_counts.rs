//! UC 08 AC#14 — Footer counts (`active`, `queued`, `done`, `waiting`)
//! computed from the current queue model.
//!
//! `crates/app/src/ui_bridge.rs::recompute_counts` is private and consumes a
//! `MainWindow`, so it cannot be invoked headlessly. We mirror its semantics
//! here as a pure function and pin the spec; if the Slint-side counter ever
//! drifts from this, the rendering smoke test (`row_visual_smoke.rs`) fails
//! independently. Mirror function lives in this test file by design — it is
//! the test fixture, not a production helper.

#[derive(Debug, Clone, Copy)]
struct CountInputs<'a> {
    status: &'a str,
    waiting_on_user: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    active: i32,
    queued: i32,
    done: i32,
    waiting: i32,
}

/// Mirror of `ui_bridge::recompute_counts`. Source of truth for the spec.
fn count(rows: &[CountInputs<'_>]) -> Counts {
    let mut c = Counts::default();
    for row in rows {
        if row.waiting_on_user {
            c.waiting += 1;
        }
        match row.status {
            "in_flight" => c.active += 1,
            "queued" => c.queued += 1,
            "done" => c.done += 1,
            _ => {}
        }
    }
    c
}

#[test]
fn empty_queue_yields_zero_counts() {
    let c = count(&[]);
    assert_eq!(c, Counts::default());
}

#[test]
fn single_in_flight_increments_active_only() {
    let c = count(&[CountInputs {
        status: "in_flight",
        waiting_on_user: false,
    }]);
    assert_eq!(
        c,
        Counts {
            active: 1,
            queued: 0,
            done: 0,
            waiting: 0,
        }
    );
}

#[test]
fn mixed_queue_counts_each_state_independently() {
    let rows = [
        CountInputs {
            status: "in_flight",
            waiting_on_user: false,
        },
        CountInputs {
            status: "in_flight",
            waiting_on_user: false,
        },
        CountInputs {
            status: "queued",
            waiting_on_user: false,
        },
        CountInputs {
            status: "queued",
            waiting_on_user: false,
        },
        CountInputs {
            status: "queued",
            waiting_on_user: false,
        },
        CountInputs {
            status: "done",
            waiting_on_user: false,
        },
    ];
    let c = count(&rows);
    assert_eq!(
        c,
        Counts {
            active: 2,
            queued: 3,
            done: 1,
            waiting: 0,
        }
    );
}

#[test]
fn waiting_on_user_is_orthogonal_to_status() {
    // A row can be both `in_flight` AND `waiting_on_user` (the bot-check
    // dialog opens while the row is in flight).
    let rows = [
        CountInputs {
            status: "in_flight",
            waiting_on_user: true,
        },
        CountInputs {
            status: "in_flight",
            waiting_on_user: false,
        },
    ];
    let c = count(&rows);
    assert_eq!(c.active, 2, "both in-flight rows count toward active");
    assert_eq!(c.waiting, 1, "only the waiting row counts toward waiting");
}

#[test]
fn cancelled_and_error_dont_count_in_any_bucket() {
    // AC#14: only active/queued/done are surfaced. Terminal-but-not-done
    // states (cancelled, error) and transients (cancelling) are visually
    // present but excluded from the footer counts.
    let rows = [
        CountInputs {
            status: "cancelled",
            waiting_on_user: false,
        },
        CountInputs {
            status: "error",
            waiting_on_user: false,
        },
        CountInputs {
            status: "cancelling",
            waiting_on_user: false,
        },
    ];
    let c = count(&rows);
    assert_eq!(c, Counts::default(), "cancelled/error/cancelling all zero");
}

#[test]
fn unknown_status_does_not_panic_and_counts_nothing() {
    let rows = [CountInputs {
        status: "weird-future-status",
        waiting_on_user: false,
    }];
    let c = count(&rows);
    assert_eq!(c, Counts::default());
}

#[test]
fn waiting_does_not_double_count_when_status_is_done() {
    // Edge case: a row could carry `waiting_on_user=true` even though its
    // status is terminal; the count function still flags it as waiting.
    // This documents the exact policy in case `recompute_counts` ever changes.
    let rows = [CountInputs {
        status: "done",
        waiting_on_user: true,
    }];
    let c = count(&rows);
    assert_eq!(c.done, 1);
    assert_eq!(c.waiting, 1);
    assert_eq!(c.active, 0);
    assert_eq!(c.queued, 0);
}

#[test]
fn large_queue_counts_match_input() {
    let mut rows = Vec::new();
    for _ in 0..50 {
        rows.push(CountInputs {
            status: "queued",
            waiting_on_user: false,
        });
    }
    for _ in 0..3 {
        rows.push(CountInputs {
            status: "in_flight",
            waiting_on_user: false,
        });
    }
    for _ in 0..10 {
        rows.push(CountInputs {
            status: "done",
            waiting_on_user: false,
        });
    }
    let c = count(&rows);
    assert_eq!(
        c,
        Counts {
            active: 3,
            queued: 50,
            done: 10,
            waiting: 0,
        }
    );
}
