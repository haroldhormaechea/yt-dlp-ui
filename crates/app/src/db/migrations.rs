//! Hand-rolled migration runner.
//!
//! Per `PROJECT_BRIEF.md` § Architecture — "`refinery` is overkill at this
//! size". Each migration is a `(version, sql)` pair; the runner applies
//! everything with `version > current` inside individual transactions, then
//! records the version in `schema_version` with `CURRENT_TIMESTAMP`.

use rusqlite::Connection;

use super::Result;

#[cfg(test)]
#[path = "migrations_test.rs"]
mod migrations_tests;

/// One migration step.
struct Migration {
    version: i64,
    sql: &'static str,
}

/// All known migrations, in ascending version order. New migrations are
/// appended to the end with the next integer version; existing entries are
/// never modified once shipped.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/0002_uc08_thumbnails_and_bytes.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/0003_uc02_partial_file_path.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("migrations/0004_uc27_placeholder_rows.sql"),
    },
];

/// Runs all migrations whose version is greater than the current
/// `schema_version`.
///
/// Creates the `schema_version` table on first run.
///
/// # Errors
///
/// Returns [`DbError::Sqlite`] if any migration step fails.
pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
        [],
    )?;

    let current: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap_or(None);
    let current = current.unwrap_or(0);

    for migration in MIGRATIONS {
        if migration.version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?, CURRENT_TIMESTAMP)",
            [migration.version],
        )?;
        tx.commit()?;
        tracing::info!(version = migration.version, "applied migration");
    }

    Ok(())
}
