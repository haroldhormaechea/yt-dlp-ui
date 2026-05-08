//! UC 09 AC#14 — Cookies-tab empty-state branch policy.
//!
//! The Slint `Select` enabled flag and the hint string are derived from
//! `cookies_options.len()`. Slint-side bindings are not directly testable
//! headlessly, so we mirror the spec as a pure function and pin it here —
//! same fixture-as-test pattern used in `footer_counts.rs`. If the Slint
//! side ever drifts from this policy, manual smoke per `CONTRIBUTING.md`
//! catches it; this test catches the model-level regression.

#[derive(Debug, PartialEq, Eq)]
struct CookiesUiState {
    select_enabled: bool,
    hint: &'static str,
}

/// Mirror of the AC#14 branch policy.
///
/// `option_count` includes the leading "None" entry that the bridge always
/// pushes, so the empty-detected case is `1` (not `0`).
fn cookies_ui_state(option_count: usize) -> CookiesUiState {
    if option_count <= 1 {
        CookiesUiState {
            select_enabled: false,
            hint: "No supported browsers detected on this machine.",
        }
    } else {
        CookiesUiState {
            select_enabled: true,
            hint: "Used only when YouTube asks for verification.",
        }
    }
}

#[test]
fn empty_detected_disables_select_and_shows_empty_hint() {
    // Bridge pushes a single "None" entry when zero browsers are detected.
    let state = cookies_ui_state(1);
    assert_eq!(
        state,
        CookiesUiState {
            select_enabled: false,
            hint: "No supported browsers detected on this machine.",
        }
    );
}

#[test]
fn zero_options_treated_as_empty() {
    // Defensive: even if the bridge ever skips the leading "None" sentinel,
    // zero options must still render the empty-state branch.
    let state = cookies_ui_state(0);
    assert!(!state.select_enabled);
    assert_eq!(
        state.hint,
        "No supported browsers detected on this machine."
    );
}

#[test]
fn one_detected_browser_enables_select() {
    // "None" + one detected browser = 2 options.
    let state = cookies_ui_state(2);
    assert_eq!(
        state,
        CookiesUiState {
            select_enabled: true,
            hint: "Used only when YouTube asks for verification.",
        }
    );
}

#[test]
fn multiple_detected_browsers_enables_select() {
    // "None" + several detected browsers.
    for opts in [3usize, 5, 9] {
        let state = cookies_ui_state(opts);
        assert!(state.select_enabled, "{opts} options must enable select");
        assert_eq!(state.hint, "Used only when YouTube asks for verification.");
    }
}

#[test]
fn all_eight_browsers_plus_none_enables_select() {
    // The maximum realistic case: every supported browser detected.
    let state = cookies_ui_state(9);
    assert!(state.select_enabled);
    assert_eq!(state.hint, "Used only when YouTube asks for verification.");
}

#[test]
fn empty_state_hint_is_verbatim_per_ac_14() {
    // Pin AC#14's verbatim copy. The same UI string is asserted in the
    // manual smoke checklist; this one catches accidental refactor edits.
    let state = cookies_ui_state(1);
    assert_eq!(
        state.hint,
        "No supported browsers detected on this machine."
    );
}

#[test]
fn populated_state_hint_is_verbatim_per_ac_14() {
    let state = cookies_ui_state(4);
    assert_eq!(state.hint, "Used only when YouTube asks for verification.");
}
