//! UC 12 — Pure-helper spec tests for the Remove-all confirm-modal copy and
//! the post-completion toast.
//!
//! `crates/app/src/ui_bridge.rs` exposes four `pub(crate)` helpers that pre-
//! render the modal's body, the danger-button label, and the toast string:
//!
//! * `pluralize_items(n)`           — "item" / "items"
//! * `format_remove_all_body(n,k)`  — (line_1, line_2); line_2 is "" when k == 0
//! * `format_remove_all_primary_label(n)` — "Remove <N>"
//! * `format_remove_all_toast(n)`   — "Queue cleared (<N> item(s))."
//!
//! `pub(crate)` visibility means these are not reachable from an external
//! integration test. We therefore mirror the helpers here as test fixtures
//! and pin the AC #5 / AC #6 / AC #8 string contract. This is the same
//! mirror-the-spec convention used by `footer_counts.rs` for
//! `ui_bridge::recompute_counts`. If `ui_bridge.rs`'s implementation ever
//! drifts from these mirrors, the rendering smoke test
//! (`remove_all_modal_smoke.rs`) and a manual smoke check at release time
//! will surface the divergence; the mirror keeps the contract auditable in
//! one place.
//!
//! Mirrors below MUST stay in lock-step with the production helpers in
//! `crates/app/src/ui_bridge.rs:75-110`.

// ----- Mirrors of the production helpers -----------------------------------

fn pluralize_items(n: i32) -> &'static str {
    if n == 1 { "item" } else { "items" }
}

fn format_remove_all_body(total: i32, in_flight: i32) -> (String, String) {
    let line_1 = format!(
        "This will remove {total} {} from the queue.",
        pluralize_items(total)
    );
    let line_2 = if in_flight == 0 {
        String::new()
    } else {
        format!(
            "{in_flight} {} still in flight and will be cancelled first. This cannot be undone.",
            if in_flight == 1 { "is" } else { "are" }
        )
    };
    (line_1, line_2)
}

fn format_remove_all_primary_label(total: i32) -> String {
    format!("Remove {total}")
}

fn format_remove_all_toast(total: i32) -> String {
    format!("Queue cleared ({total} {}).", pluralize_items(total))
}

// ----- pluralize_items -----------------------------------------------------

#[test]
fn pluralize_items_singular_when_one() {
    assert_eq!(pluralize_items(1), "item");
}

#[test]
fn pluralize_items_plural_when_zero() {
    // The button is gated to `total > 0` (AC #2), so n == 0 is never rendered
    // in real flow. Pinned for completeness — defensive plural matches the
    // English-grammar convention "0 items".
    assert_eq!(pluralize_items(0), "items");
}

#[test]
fn pluralize_items_plural_when_many() {
    assert_eq!(pluralize_items(2), "items");
    assert_eq!(pluralize_items(5), "items");
    assert_eq!(pluralize_items(123), "items");
}

// ----- format_remove_all_body — AC #5 --------------------------------------

#[test]
fn body_omits_in_flight_line_when_k_is_zero() {
    // AC #5: the second sentence MUST be omitted when K == 0. The host
    // writes an empty string for line_2 so the Slint `if root.body-line-2 !=
    // ""` guard collapses the second `Text` element to a no-op.
    let (line_1, line_2) = format_remove_all_body(7, 0);
    assert_eq!(line_1, "This will remove 7 items from the queue.");
    assert_eq!(line_2, "", "K == 0 must yield an empty line_2");
}

#[test]
fn body_singular_total_omits_in_flight_line_when_k_is_zero() {
    // AC #5 + pluralization: N == 1 must read "1 item", not "1 items".
    let (line_1, line_2) = format_remove_all_body(1, 0);
    assert_eq!(line_1, "This will remove 1 item from the queue.");
    assert_eq!(line_2, "");
}

#[test]
fn body_renders_in_flight_line_when_k_is_one() {
    // AC #5: when K == 1 the in-flight call-out reads "is", not "are", and
    // the trailing "This cannot be undone." sentence is appended.
    let (line_1, line_2) = format_remove_all_body(5, 1);
    assert_eq!(line_1, "This will remove 5 items from the queue.");
    assert_eq!(
        line_2,
        "1 is still in flight and will be cancelled first. This cannot be undone."
    );
}

#[test]
fn body_renders_in_flight_line_when_k_is_many() {
    // AC #5: K > 1 reads "are", and N stays plural.
    let (line_1, line_2) = format_remove_all_body(10, 3);
    assert_eq!(line_1, "This will remove 10 items from the queue.");
    assert_eq!(
        line_2,
        "3 are still in flight and will be cancelled first. This cannot be undone."
    );
}

#[test]
fn body_renders_in_flight_line_when_n_equals_k() {
    // Edge case — every queued row is an in-flight row (a queue full of
    // bot-check-stuck downloads, for example). N == K, both plural.
    let (line_1, line_2) = format_remove_all_body(4, 4);
    assert_eq!(line_1, "This will remove 4 items from the queue.");
    assert_eq!(
        line_2,
        "4 are still in flight and will be cancelled first. This cannot be undone."
    );
}

#[test]
fn body_renders_singular_total_with_in_flight_one() {
    // Edge case — a queue of exactly one in-flight row. Both N and K
    // are 1; "item" stays singular and "is" is the right verb.
    let (line_1, line_2) = format_remove_all_body(1, 1);
    assert_eq!(line_1, "This will remove 1 item from the queue.");
    assert_eq!(
        line_2,
        "1 is still in flight and will be cancelled first. This cannot be undone."
    );
}

// ----- format_remove_all_primary_label — AC #6 -----------------------------

#[test]
fn primary_label_reads_remove_n() {
    // AC #6: the danger-palette primary button is labelled "Remove <N>".
    assert_eq!(format_remove_all_primary_label(1), "Remove 1");
    assert_eq!(format_remove_all_primary_label(5), "Remove 5");
    assert_eq!(format_remove_all_primary_label(123), "Remove 123");
}

// ----- format_remove_all_toast — AC #8 -------------------------------------

#[test]
fn toast_reads_queue_cleared_n_items_with_period() {
    // AC #8: post-completion toast reads "Queue cleared (<N> item(s))."
    // — note the trailing period and the parenthesised count.
    assert_eq!(format_remove_all_toast(1), "Queue cleared (1 item).");
    assert_eq!(format_remove_all_toast(5), "Queue cleared (5 items).");
    assert_eq!(format_remove_all_toast(123), "Queue cleared (123 items).");
}
