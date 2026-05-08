//! UC 08 AC#12 — Action-button-set per row state.
//!
//! Pure-Rust fixture pinning the spec. The actual button rendering lives in
//! `crates/app/ui/queue_row.slint` (`RowActions`); this test is the
//! source-of-truth contract that the design and dev-team agreed on. If the
//! Slint file diverges from this, an explicit follow-up is required.
//!
//! Source: `PROJECT_BRIEF.md` → `use-cases/08-reskin-main-shell-and-rows.md` AC#12.

/// Returns the expected action buttons (in left-to-right order) for the
/// given row status string. Status strings mirror `QueueStatus::as_str` plus
/// the transient `"cancelling"` and `"waiting_on_user"` variants the row
/// component understands.
fn expected_buttons_for(status: &str) -> &'static [&'static str] {
    match status {
        "queued" => &["Download", "Cancel", "Remove"],
        "in_flight" | "waiting_on_user" => &["Cancel", "Remove"],
        "cancelling" => &["Cancelling…"],
        "cancelled" => &["Restart", "Remove"],
        "done" | "error" => &["Remove"],
        _ => &[],
    }
}

#[test]
fn queued_has_download_cancel_remove() {
    assert_eq!(
        expected_buttons_for("queued"),
        &["Download", "Cancel", "Remove"]
    );
}

#[test]
fn in_flight_has_cancel_remove() {
    assert_eq!(expected_buttons_for("in_flight"), &["Cancel", "Remove"]);
}

#[test]
fn cancelling_has_single_disabled_button() {
    let buttons = expected_buttons_for("cancelling");
    assert_eq!(
        buttons.len(),
        1,
        "cancelling shows a single disabled button"
    );
    assert_eq!(buttons[0], "Cancelling…");
}

#[test]
fn cancelled_has_restart_remove() {
    assert_eq!(expected_buttons_for("cancelled"), &["Restart", "Remove"]);
}

#[test]
fn done_has_remove_only() {
    assert_eq!(expected_buttons_for("done"), &["Remove"]);
}

#[test]
fn error_has_remove_only() {
    assert_eq!(expected_buttons_for("error"), &["Remove"]);
}

#[test]
fn waiting_on_user_has_cancel_remove() {
    assert_eq!(
        expected_buttons_for("waiting_on_user"),
        &["Cancel", "Remove"]
    );
}

#[test]
fn unknown_status_yields_empty_button_set() {
    assert!(
        expected_buttons_for("nonsense").is_empty(),
        "unknown status → no buttons (defensive default)"
    );
}

#[test]
fn cancel_only_present_when_action_is_meaningful() {
    // Cancel must NOT appear on terminal states (done, error, cancelled).
    for terminal in ["done", "error", "cancelled"] {
        let buttons = expected_buttons_for(terminal);
        assert!(
            !buttons.contains(&"Cancel"),
            "Cancel must not appear on terminal state {terminal}: {buttons:?}"
        );
    }
}

#[test]
fn restart_only_on_cancelled() {
    // Restart is the cancelled-state recovery path; no other state shows it.
    for state in [
        "queued",
        "in_flight",
        "cancelling",
        "done",
        "error",
        "waiting_on_user",
    ] {
        let buttons = expected_buttons_for(state);
        assert!(
            !buttons.contains(&"Restart"),
            "Restart must only appear on cancelled, not on {state}: {buttons:?}"
        );
    }
    assert!(expected_buttons_for("cancelled").contains(&"Restart"));
}

#[test]
fn remove_present_in_every_terminal_or_user_actionable_state() {
    // Per AC#12: Remove appears in every state that surfaces buttons except
    // the transient `cancelling` one.
    for state in [
        "queued",
        "in_flight",
        "cancelled",
        "done",
        "error",
        "waiting_on_user",
    ] {
        let buttons = expected_buttons_for(state);
        assert!(
            buttons.contains(&"Remove"),
            "Remove must appear in {state}: {buttons:?}"
        );
    }
    assert!(
        !expected_buttons_for("cancelling").contains(&"Remove"),
        "cancelling shows ONLY the disabled `Cancelling…` button"
    );
}
