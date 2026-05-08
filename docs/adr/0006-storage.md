# 0006 — Storage

- **Status:** accepted
- **Date:** 2026-04-25

## Context

The application needs to persist:

- The download queue (URL, format choice, status, progress, errors).
- User settings (default download folder, format preset, concurrency cap,
  ad-consent state, focus mode flag).
- Download history (completed downloads with paths and timestamps).

Persistence must survive app restarts. The queue must support an
**unlimited** number of items, each individually cancellable, plus
whole-queue cancellation. Resume-after-restart is required (PROJECT_BRIEF.md
§ Architecture § Download concurrency model — option R2).

## Decision

Use **SQLite** via the **`rusqlite`** crate with the **`bundled`** feature.

- `rusqlite` provides synchronous, ergonomic SQL access. For a single-
  process desktop app, sync SQL is the cleanest fit; `sqlx` adds an async
  layer that is unnecessary here.
- The `bundled` feature compiles SQLite into the binary, decoupling us
  from system-libsqlite. This avoids signing/notarization issues with
  system-library linking on macOS and version-skew issues across distros.
- A hand-rolled `schema_version` table tracks migrations; one migration
  function per version, called at app startup. `refinery` and similar
  tools are overkill at this size.
- The database file lives at the per-OS app-data location resolved by the
  `directories` crate:
  - Linux: `~/.local/share/yt-dlp-ui/db.sqlite`
  - macOS: `~/Library/Application Support/yt-dlp-ui/db.sqlite`
  - Windows: `%LOCALAPPDATA%\yt-dlp-ui\db.sqlite`

## Consequences

**Positive:**
- Atomic transactions for queue mutations (status changes, progress
  updates, history inserts) — no risk of half-written state on crash.
- Real SQL queries enable filtered views (e.g., "show only completed",
  "show errors from the last 24h") that JSON-per-list cannot.
- Bundled SQLite means the same schema and behavior on all three OSes; no
  "works on macOS, breaks on Linux because of libsqlite version" surprises.
- `directories` crate centralizes per-OS path resolution so we don't write
  three platform-specific path-builders.

**Negative:**
- Bundled SQLite adds ~500 KB to the binary. Acceptable within the
  bundle-size budget.
- Schema migrations require discipline — every new column / table needs a
  migration function bumping `schema_version`.
- Only one writer at a time (SQLite's lock model). A single-process
  desktop app naturally satisfies this; not a real constraint.

## Alternatives considered

- **JSON files** (one per concern) — simplest, but breaks down at scale
  for the queue and requires careful atomic-write handling to survive
  crashes.
- **Per-framework KV** (e.g., Tauri Store) — n/a since we're not on Tauri.
- **`sqlx` (async SQL)** — adds an async layer with no benefit for a
  single-process desktop app; complicates testing.
- **System-libsqlite linking** — version skew + signing/notarization
  problems. Not worth the ~500 KB savings.

## References

- PROJECT_BRIEF.md § Architecture § Download concurrency model
- PROJECT_BRIEF.md § Architecture § Storage layout
