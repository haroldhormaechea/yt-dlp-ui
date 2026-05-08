//! Tests for [`crate::db::settings`].

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use yt_dlp_bridge::FormatPref;

use crate::browsers::Browser;
use crate::db::migrations::run_migrations;
use crate::db::settings;

fn fresh_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open :memory:");
    run_migrations(&mut conn).unwrap();
    conn
}

#[test]
fn kv_round_trip() {
    let conn = fresh_db();
    settings::set_string(&conn, "k1", "v1").unwrap();
    let got = settings::get_string(&conn, "k1").unwrap();
    assert_eq!(got.as_deref(), Some("v1"));
}

#[test]
fn kv_get_missing_returns_none() {
    let conn = fresh_db();
    let got = settings::get_string(&conn, "missing").unwrap();
    assert!(got.is_none());
}

#[test]
fn kv_set_overwrites() {
    let conn = fresh_db();
    settings::set_string(&conn, "k", "first").unwrap();
    settings::set_string(&conn, "k", "second").unwrap();
    let got = settings::get_string(&conn, "k").unwrap();
    assert_eq!(got.as_deref(), Some("second"));
}

#[test]
fn concurrency_cap_default_is_3() {
    let conn = fresh_db();
    let cap = settings::get_concurrency_cap(&conn).unwrap();
    assert_eq!(cap, 3, "default concurrency cap is 3");
}

#[test]
fn concurrency_cap_clamp_low() {
    let conn = fresh_db();
    settings::set_concurrency_cap(&conn, 0).unwrap();
    let cap = settings::get_concurrency_cap(&conn).unwrap();
    assert_eq!(cap, 1, "cap 0 clamped to 1");
}

#[test]
fn concurrency_cap_clamp_high() {
    let conn = fresh_db();
    settings::set_concurrency_cap(&conn, 999).unwrap();
    let cap = settings::get_concurrency_cap(&conn).unwrap();
    assert_eq!(cap, 10, "cap 999 clamped to 10");
}

#[test]
fn concurrency_cap_in_range_round_trip() {
    let conn = fresh_db();
    for value in [1u32, 2, 3, 5, 10] {
        settings::set_concurrency_cap(&conn, value).unwrap();
        assert_eq!(settings::get_concurrency_cap(&conn).unwrap(), value);
    }
}

#[test]
fn concurrency_cap_read_side_clamp_when_db_value_out_of_range() {
    // Bypass the setter to write an out-of-range raw value, simulating an
    // older build or a hand-edited DB.
    let conn = fresh_db();
    settings::set_string(&conn, "concurrency_cap", "42").unwrap();
    let cap = settings::get_concurrency_cap(&conn).unwrap();
    assert_eq!(cap, 10, "out-of-range raw values are clamped on read");
}

#[test]
fn concurrency_cap_unparseable_returns_default() {
    let conn = fresh_db();
    settings::set_string(&conn, "concurrency_cap", "not-a-number").unwrap();
    let cap = settings::get_concurrency_cap(&conn).unwrap();
    assert_eq!(cap, 3, "unparseable raw value falls back to default");
}

#[test]
fn format_pref_default_is_best_heuristic() {
    let conn = fresh_db();
    let pref = settings::get_format_pref(&conn).unwrap();
    assert_eq!(pref, FormatPref::BestHeuristic);
}

#[test]
fn format_pref_round_trip_each_variant() {
    let conn = fresh_db();
    for variant in [
        FormatPref::BestVideo,
        FormatPref::BestAudioMp3,
        FormatPref::BestAudioOpus,
        FormatPref::BestHeuristic,
    ] {
        settings::set_format_pref(&conn, variant).unwrap();
        let got = settings::get_format_pref(&conn).unwrap();
        assert_eq!(got, variant);
    }
}

#[test]
fn format_pref_malformed_falls_back_to_default() {
    let conn = fresh_db();
    settings::set_string(&conn, "format_pref", "not-json").unwrap();
    let pref = settings::get_format_pref(&conn).unwrap();
    assert_eq!(
        pref,
        FormatPref::BestHeuristic,
        "malformed JSON should use default"
    );
}

#[test]
fn dest_dir_default_when_unset() {
    let conn = fresh_db();
    let default = PathBuf::from("/some/default");
    let got = settings::get_dest_dir(&conn, &default).unwrap();
    assert_eq!(got, default);
}

#[test]
fn dest_dir_round_trip() {
    let conn = fresh_db();
    let custom = Path::new("/custom/path");
    settings::set_dest_dir(&conn, custom).unwrap();
    let got = settings::get_dest_dir(&conn, Path::new("/unused")).unwrap();
    assert_eq!(got, PathBuf::from("/custom/path"));
}

// UC 16: normalization on write — `set_dest_dir` strips a trailing path
// separator and expands a leading `~/` to the user's home directory.

#[test]
fn dest_dir_normalization_strips_trailing_separator() {
    // UC 16 § Pitfalls — `rfd` and hand-typed paths can carry trailing
    // separators; persistence normalizes them away so the value passed to
    // yt-dlp is in a canonical shape.
    let conn = fresh_db();
    let trailing = format!("/some/path{}", std::path::MAIN_SEPARATOR);
    settings::set_dest_dir(&conn, Path::new(&trailing)).unwrap();
    let got = settings::get_dest_dir(&conn, Path::new("/unused")).unwrap();
    assert_eq!(
        got,
        PathBuf::from("/some/path"),
        "trailing separator must be stripped on persist"
    );
}

#[test]
fn dest_dir_normalization_preserves_root_only_path() {
    // Defensive: `/` (Unix) is the only character of the path; the strip
    // logic must NOT remove it.
    let conn = fresh_db();
    let root = format!("{}", std::path::MAIN_SEPARATOR);
    settings::set_dest_dir(&conn, Path::new(&root)).unwrap();
    let got = settings::get_dest_dir(&conn, Path::new("/unused")).unwrap();
    assert_eq!(
        got,
        PathBuf::from(&root),
        "root-only path must survive normalization unchanged"
    );
}

#[test]
fn dest_dir_normalization_expands_leading_tilde_when_home_set() {
    // UC 16 § Pitfalls — a hand-typed `~/Downloads/yt-dlp-ui` should be
    // expanded to the absolute path before being passed to yt-dlp. We
    // rely on the test runner's HOME / USERPROFILE being set (a
    // reasonable assumption — `cargo test` inherits the caller's env).
    // If the env var is missing in some sandboxed environment, the
    // normalization is a no-op (the path stays as `~/...`); in that
    // case we skip the expansion-shape assertion.
    let conn = fresh_db();
    let home_var = if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    };
    let Ok(home) = std::env::var(home_var) else {
        eprintln!("skipping tilde-expansion assertion: {home_var} unset");
        return;
    };
    if home.is_empty() {
        eprintln!("skipping tilde-expansion assertion: {home_var} empty");
        return;
    }

    settings::set_dest_dir(&conn, Path::new("~/Downloads/yt-dlp-ui")).unwrap();
    let got = settings::get_dest_dir(&conn, Path::new("/unused")).unwrap();

    let expected = {
        let mut p = PathBuf::from(&home);
        p.push("Downloads");
        p.push("yt-dlp-ui");
        p
    };
    assert_eq!(
        got, expected,
        "`~/...` must be expanded to <home>/<rest> on persist (HOME = {home})"
    );
    // Defensive: regardless of HOME's exact value, the persisted path
    // must NOT begin with the literal `~`.
    assert!(
        !got.to_string_lossy().starts_with('~'),
        "tilde must be replaced, never persisted verbatim"
    );
}

#[test]
fn dest_dir_normalization_no_tilde_passthrough() {
    // Sanity — a path without a leading `~/` is unaffected by the
    // tilde-expansion branch.
    let conn = fresh_db();
    settings::set_dest_dir(&conn, Path::new("/already/absolute")).unwrap();
    let got = settings::get_dest_dir(&conn, Path::new("/unused")).unwrap();
    assert_eq!(got, PathBuf::from("/already/absolute"));
}

#[test]
fn dest_dir_empty_string_treated_as_absent() {
    let conn = fresh_db();
    settings::set_string(&conn, "dest_dir", "").unwrap();
    let default = PathBuf::from("/default");
    let got = settings::get_dest_dir(&conn, &default).unwrap();
    assert_eq!(got, default, "empty string falls back to default");
}

// -- cookies_browser ---------------------------------------------------------

#[test]
fn cookies_browser_default_is_none() {
    let conn = fresh_db();
    let got = settings::get_cookies_browser(&conn).unwrap();
    assert_eq!(got, None, "default cookies_browser is None");
}

#[test]
fn cookies_browser_round_trip_each_variant() {
    let conn = fresh_db();
    for variant in [
        Browser::Brave,
        Browser::Chrome,
        Browser::Chromium,
        Browser::Edge,
        Browser::Firefox,
        Browser::Opera,
        Browser::Safari,
        Browser::Vivaldi,
    ] {
        settings::set_cookies_browser(&conn, Some(variant)).unwrap();
        let got = settings::get_cookies_browser(&conn).unwrap();
        assert_eq!(got, Some(variant), "round-trip {variant:?}");
    }
}

#[test]
fn cookies_browser_explicit_none_round_trips_as_none() {
    let conn = fresh_db();
    // First set a real browser, then clear back to None to make sure the
    // setter handles None as "remembered no choice" rather than leaving the
    // prior value behind.
    settings::set_cookies_browser(&conn, Some(Browser::Firefox)).unwrap();
    settings::set_cookies_browser(&conn, None).unwrap();
    let got = settings::get_cookies_browser(&conn).unwrap();
    assert_eq!(got, None);
}

#[test]
fn cookies_browser_malformed_json_falls_back_to_none() {
    // An old build / hand-edited DB might leave non-JSON in the column.
    // Reader must fall back to None and log a WARN (the WARN itself is not
    // asserted here — just the safe fallback).
    let conn = fresh_db();
    settings::set_string(&conn, settings::KEY_COOKIES_BROWSER, "not-json").unwrap();
    let got = settings::get_cookies_browser(&conn).unwrap();
    assert_eq!(
        got, None,
        "malformed cookies_browser JSON falls back to None"
    );
}

#[test]
fn cookies_browser_empty_string_treated_as_none() {
    let conn = fresh_db();
    settings::set_string(&conn, settings::KEY_COOKIES_BROWSER, "").unwrap();
    let got = settings::get_cookies_browser(&conn).unwrap();
    assert_eq!(got, None);
}

// -- theme -------------------------------------------------------------------

use crate::db::settings::{ExplicitTheme, ThemePref};

#[test]
fn theme_default_is_system() {
    let conn = fresh_db();
    let got = settings::get_theme(&conn).unwrap();
    assert_eq!(
        got,
        ThemePref::System,
        "fresh settings table returns System per AC#5"
    );
}

#[test]
fn theme_round_trip_light() {
    let conn = fresh_db();
    settings::set_theme(&conn, ExplicitTheme::Light).unwrap();
    let got = settings::get_theme(&conn).unwrap();
    assert_eq!(got, ThemePref::Light);
}

#[test]
fn theme_round_trip_dark() {
    let conn = fresh_db();
    settings::set_theme(&conn, ExplicitTheme::Dark).unwrap();
    let got = settings::get_theme(&conn).unwrap();
    assert_eq!(got, ThemePref::Dark);
}

#[test]
fn theme_unknown_value_falls_back_to_system() {
    // Bypass `set_theme` to inject a bogus stored value, simulating a
    // hand-edited DB or a forward-compat scenario.
    let conn = fresh_db();
    settings::set_string(&conn, settings::KEY_THEME, "plaid").unwrap();
    let got = settings::get_theme(&conn).unwrap();
    assert_eq!(
        got,
        ThemePref::System,
        "unknown theme value falls back to System (and logs warn)"
    );
}

#[test]
fn theme_explicit_light_to_dark_overwrites() {
    // The toggle path overwrites in place — make sure the second write wins
    // and the read reflects the most recent explicit choice.
    let conn = fresh_db();
    settings::set_theme(&conn, ExplicitTheme::Light).unwrap();
    settings::set_theme(&conn, ExplicitTheme::Dark).unwrap();
    let got = settings::get_theme(&conn).unwrap();
    assert_eq!(got, ThemePref::Dark);
}

#[test]
fn theme_stored_value_is_lowercase_string() {
    // Lock the on-disk format so a future refactor cannot silently change it
    // (would break forward compatibility for existing installs).
    let conn = fresh_db();
    settings::set_theme(&conn, ExplicitTheme::Light).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_THEME)
            .unwrap()
            .as_deref(),
        Some("light")
    );
    settings::set_theme(&conn, ExplicitTheme::Dark).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_THEME)
            .unwrap()
            .as_deref(),
        Some("dark")
    );
}

#[test]
fn theme_explicit_into_themepref() {
    // The `From<ExplicitTheme> for ThemePref` impl is used by the toggle
    // path; lock it down so the mapping cannot drift.
    assert_eq!(ThemePref::from(ExplicitTheme::Light), ThemePref::Light);
    assert_eq!(ThemePref::from(ExplicitTheme::Dark), ThemePref::Dark);
}

// -- focus_mode (UC 09 AC#16) ------------------------------------------------

#[test]
fn focus_mode_default_is_false() {
    let conn = fresh_db();
    let got = settings::get_focus_mode(&conn).unwrap();
    assert!(!got, "default focus_mode is false (AC#16)");
}

#[test]
fn focus_mode_round_trip() {
    let conn = fresh_db();
    settings::set_focus_mode(&conn, true).unwrap();
    assert!(settings::get_focus_mode(&conn).unwrap());
    settings::set_focus_mode(&conn, false).unwrap();
    assert!(!settings::get_focus_mode(&conn).unwrap());
}

#[test]
fn focus_mode_unparseable_falls_back_to_false() {
    // Hand-edited DB or forward-compat scenario: anything that is not the
    // literal "true" must read as false.
    let conn = fresh_db();
    settings::set_string(&conn, settings::KEY_FOCUS_MODE, "garbage").unwrap();
    let got = settings::get_focus_mode(&conn).unwrap();
    assert!(!got, "unparseable focus_mode falls back to false");
}

#[test]
fn focus_mode_stored_value_is_lowercase_string() {
    // Lock the on-disk format so a future refactor cannot silently change it.
    let conn = fresh_db();
    settings::set_focus_mode(&conn, true).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_FOCUS_MODE)
            .unwrap()
            .as_deref(),
        Some("true")
    );
    settings::set_focus_mode(&conn, false).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_FOCUS_MODE)
            .unwrap()
            .as_deref(),
        Some("false")
    );
}

// -- ads_personalized (UC 09 AC#17) ------------------------------------------

#[test]
fn ads_personalized_default_is_true() {
    // AC#17: personalization is opt-in by default; the user opts out
    // explicitly via the panel toggle.
    let conn = fresh_db();
    let got = settings::get_ads_personalized(&conn).unwrap();
    assert!(got, "default ads_personalized is true (AC#17)");
}

#[test]
fn ads_personalized_round_trip() {
    let conn = fresh_db();
    settings::set_ads_personalized(&conn, false).unwrap();
    assert!(!settings::get_ads_personalized(&conn).unwrap());
    settings::set_ads_personalized(&conn, true).unwrap();
    assert!(settings::get_ads_personalized(&conn).unwrap());
}

#[test]
fn ads_personalized_unparseable_falls_back_to_true() {
    // Inverse default vs. focus_mode — a malformed value must not silently
    // opt the user out. Only the literal "false" disables personalization.
    let conn = fresh_db();
    settings::set_string(&conn, settings::KEY_ADS_PERSONALIZED, "garbage").unwrap();
    let got = settings::get_ads_personalized(&conn).unwrap();
    assert!(got, "unparseable ads_personalized falls back to true");
}

#[test]
fn ads_personalized_stored_value_is_lowercase_string() {
    let conn = fresh_db();
    settings::set_ads_personalized(&conn, true).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_ADS_PERSONALIZED)
            .unwrap()
            .as_deref(),
        Some("true")
    );
    settings::set_ads_personalized(&conn, false).unwrap();
    assert_eq!(
        settings::get_string(&conn, settings::KEY_ADS_PERSONALIZED)
            .unwrap()
            .as_deref(),
        Some("false")
    );
}
