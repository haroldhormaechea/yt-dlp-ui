//! KV accessors for the `settings` table, plus typed wrappers for the
//! settings UC 01 introduces (`concurrency_cap`, `format_pref`, `dest_dir`).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use yt_dlp_bridge::FormatPref;

use super::Result;
use crate::browsers::Browser;

#[cfg(test)]
#[path = "settings_test.rs"]
mod settings_tests;

const KEY_CONCURRENCY_CAP: &str = "concurrency_cap";
const KEY_FORMAT_PREF: &str = "format_pref";
const KEY_DEST_DIR: &str = "dest_dir";
pub const KEY_COOKIES_BROWSER: &str = "cookies_browser";
// `deno_warning_dismissed` KV key was retired in UC 11 (banner dismissal is
// session-only). Do not re-introduce a key with that name; pre-existing rows
// on upgraded installs are dead data and shadow nothing.
pub const KEY_THEME: &str = "theme";
pub const KEY_FOCUS_MODE: &str = "focus_mode";
pub const KEY_ADS_PERSONALIZED: &str = "ads_personalized";

/// User-facing theme preference.
///
/// `System` is the first-launch default and means "follow the OS color
/// scheme". The in-app toggle persists `Light` or `Dark` and exits system
/// mode permanently — so this enum has a dedicated *write* sibling,
/// [`ExplicitTheme`], that the toggle path uses to make a `System` write
/// impossible by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePref {
    Light,
    Dark,
    System,
}

/// Subset of [`ThemePref`] that excludes `System` — used by the toggle write
/// path so a `System` value cannot be persisted accidentally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplicitTheme {
    Light,
    Dark,
}

impl From<ExplicitTheme> for ThemePref {
    fn from(t: ExplicitTheme) -> Self {
        match t {
            ExplicitTheme::Light => Self::Light,
            ExplicitTheme::Dark => Self::Dark,
        }
    }
}

/// Default concurrency cap (per `PROJECT_BRIEF.md` § Architecture —
/// "default 3, range 1..=10").
const DEFAULT_CONCURRENCY_CAP: u32 = 3;
const MIN_CONCURRENCY_CAP: u32 = 1;
const MAX_CONCURRENCY_CAP: u32 = 10;

/// Reads the value of a string setting. Returns `Ok(None)` when the key is
/// absent.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_string(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ? LIMIT 1")?;
    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        let v: String = row.get(0)?;
        Ok(Some(v))
    } else {
        Ok(None)
    }
}

/// Sets the value of a string setting. Upserts on the key.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_string(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Reads the concurrency cap setting.
///
/// Defaults to 3 when unset; clamps to `1..=10` when out of range. Out-of-range
/// values are not rewritten back to the DB — the clamp is read-side only,
/// since the next `set_concurrency_cap` will overwrite anyway.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_concurrency_cap(conn: &Connection) -> Result<u32> {
    let raw = get_string(conn, KEY_CONCURRENCY_CAP)?;
    let value = raw
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_CONCURRENCY_CAP);
    Ok(value.clamp(MIN_CONCURRENCY_CAP, MAX_CONCURRENCY_CAP))
}

/// Stores the concurrency cap (clamped to `1..=10`).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_concurrency_cap(conn: &Connection, cap: u32) -> Result<()> {
    let clamped = cap.clamp(MIN_CONCURRENCY_CAP, MAX_CONCURRENCY_CAP);
    set_string(conn, KEY_CONCURRENCY_CAP, &clamped.to_string())
}

/// Reads the format preference.
///
/// Defaults to [`FormatPref::default`] when unset. Malformed JSON is also
/// treated as "use the default" rather than a hard failure — the user can
/// always change it via the UI.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_format_pref(conn: &Connection) -> Result<FormatPref> {
    let raw = get_string(conn, KEY_FORMAT_PREF)?;
    if let Some(json) = raw {
        match serde_json::from_str::<FormatPref>(&json) {
            Ok(p) => return Ok(p),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "malformed format_pref in settings; falling back to default"
                );
            }
        }
    }
    Ok(FormatPref::default())
}

/// Stores the format preference.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure or [`DbError::Json`] if
/// serialization fails.
pub fn set_format_pref(conn: &Connection, pref: FormatPref) -> Result<()> {
    let json = serde_json::to_string(&pref)?;
    set_string(conn, KEY_FORMAT_PREF, &json)
}

/// Reads the download destination directory, falling back to `default_dir`
/// when the setting is absent. Empty strings in the DB are treated as
/// "absent".
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_dest_dir(conn: &Connection, default_dir: &Path) -> Result<PathBuf> {
    let raw = get_string(conn, KEY_DEST_DIR)?;
    Ok(raw
        .filter(|s| !s.is_empty())
        .map_or_else(|| default_dir.to_path_buf(), PathBuf::from))
}

/// Stores the destination directory.
///
/// Normalizes the path before persistence (UC 16): trailing separator stripped
/// and a leading `~/` expanded to the user's home directory. The value the
/// app uses at spawn time is whatever `get_dest_dir` returns, so normalizing
/// at the write boundary keeps `rfd`-supplied or hand-typed paths from
/// reaching `yt-dlp` in a malformed shape.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_dest_dir(conn: &Connection, dir: &Path) -> Result<()> {
    let normalized = normalize_dest_dir(dir);
    set_string(conn, KEY_DEST_DIR, &normalized.to_string_lossy())
}

/// Trims a trailing path separator and expands a leading `~/` to the user's
/// home directory. Empty home env (Unix `$HOME` / Windows `%USERPROFILE%`)
/// leaves the path as-is so we never produce a path beginning with `/`.
fn normalize_dest_dir(dir: &Path) -> PathBuf {
    let s = dir.to_string_lossy();

    let home_var = if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    };
    let expanded: String = if let Some(rest) = s.strip_prefix("~/") {
        match std::env::var(home_var) {
            Ok(home) if !home.is_empty() => {
                let sep = std::path::MAIN_SEPARATOR;
                format!("{home}{sep}{rest}")
            }
            _ => s.into_owned(),
        }
    } else {
        s.into_owned()
    };

    let trimmed = expanded.trim_end_matches(std::path::MAIN_SEPARATOR);
    // Don't strip the only character of a root-only path (`/` on Unix).
    if trimmed.is_empty() {
        PathBuf::from(expanded)
    } else {
        PathBuf::from(trimmed)
    }
}

/// Reads the cookies-source browser preference.
///
/// `Ok(None)` when unset, when the stored value is the empty string, or when
/// the JSON cannot be deserialized into a [`Browser`] (e.g. an old enum
/// variant the user is running with after a downgrade).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_cookies_browser(conn: &Connection) -> Result<Option<Browser>> {
    let raw = get_string(conn, KEY_COOKIES_BROWSER)?;
    let Some(json) = raw else { return Ok(None) };
    if json.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Browser>(&json) {
        Ok(b) => Ok(Some(b)),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "malformed cookies_browser in settings; falling back to None"
            );
            Ok(None)
        }
    }
}

/// Stores the cookies-source browser preference. `None` is encoded as the
/// empty string (sentinel) so a remembered "no cookies" round-trips faithfully
/// without removing the row.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure or [`DbError::Json`] on
/// serialization failure.
pub fn set_cookies_browser(conn: &Connection, choice: Option<Browser>) -> Result<()> {
    match choice {
        None => set_string(conn, KEY_COOKIES_BROWSER, ""),
        Some(b) => {
            let json = serde_json::to_string(&b)?;
            set_string(conn, KEY_COOKIES_BROWSER, &json)
        }
    }
}

/// Reads the persisted theme preference.
///
/// Defaults to `System` when the key is absent. Unknown stored values log a
/// `tracing::warn!` and fall back to `System` — matching the
/// `get_format_pref` / `get_cookies_browser` recovery style.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_theme(conn: &Connection) -> Result<ThemePref> {
    let raw = get_string(conn, KEY_THEME)?;
    match raw.as_deref() {
        Some("light") => Ok(ThemePref::Light),
        Some("dark") => Ok(ThemePref::Dark),
        Some("system") | None => Ok(ThemePref::System),
        Some(other) => {
            tracing::warn!(
                value = other,
                "unknown theme value in settings; falling back to System"
            );
            Ok(ThemePref::System)
        }
    }
}

/// Reads the focus-mode flag (UC 09).
///
/// Defaults to `false` when unset, empty, or unparseable. The user can flip
/// it from the settings panel either way, so a malformed value never blocks
/// UX.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_focus_mode(conn: &Connection) -> Result<bool> {
    let raw = get_string(conn, KEY_FOCUS_MODE)?;
    Ok(raw.as_deref() == Some("true"))
}

/// Persists the focus-mode flag (UC 09).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_focus_mode(conn: &Connection, on: bool) -> Result<()> {
    set_string(conn, KEY_FOCUS_MODE, if on { "true" } else { "false" })
}

/// Reads the ad-personalization preference (UC 09).
///
/// Defaults to `true` (personalization opt-in by default) when unset; the
/// user opts out via the settings panel. Unparseable / unknown values fall
/// back to `true` for the same reason — opt-out is an explicit user action.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn get_ads_personalized(conn: &Connection) -> Result<bool> {
    let raw = get_string(conn, KEY_ADS_PERSONALIZED)?;
    match raw.as_deref() {
        Some("false") => Ok(false),
        _ => Ok(true),
    }
}

/// Persists the ad-personalization preference (UC 09).
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_ads_personalized(conn: &Connection, on: bool) -> Result<()> {
    set_string(
        conn,
        KEY_ADS_PERSONALIZED,
        if on { "true" } else { "false" },
    )
}

/// Persists an explicit theme choice.
///
/// Only `Light` or `Dark` is writable — by construction (`ExplicitTheme`
/// excludes `System`), the toggle path cannot accidentally persist `System`,
/// which matches the AC#6 contract: once the user picks, system mode is
/// exited permanently.
///
/// The signature deliberately rejects `ThemePref` to keep `System` out of
/// the write path at compile time:
///
/// ```compile_fail
/// use app::db::settings::{set_theme, ThemePref};
/// use rusqlite::Connection;
/// let conn = Connection::open_in_memory().unwrap();
/// // `set_theme` takes `ExplicitTheme`, not `ThemePref` — this must not compile.
/// set_theme(&conn, ThemePref::System).unwrap();
/// ```
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] on any DB failure.
pub fn set_theme(conn: &Connection, pref: ExplicitTheme) -> Result<()> {
    let value = match pref {
        ExplicitTheme::Light => "light",
        ExplicitTheme::Dark => "dark",
    };
    set_string(conn, KEY_THEME, value)
}
