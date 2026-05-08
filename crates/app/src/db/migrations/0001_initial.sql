-- Initial schema for yt-dlp-ui.
--
-- Tables:
--   queue_items  — one row per URL in the queue (or per playlist entry after
--                  expansion). UNIQUE on `url` so duplicate adds are rejected.
--   settings     — KV table for app preferences.
--   history      — append-only completed-download log.

CREATE TABLE queue_items (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  url             TEXT NOT NULL UNIQUE,
  title           TEXT,
  title_status    TEXT NOT NULL,
  title_error     TEXT,
  status          TEXT NOT NULL,
  progress_pct    REAL,
  speed_bps       INTEGER,
  eta_s           INTEGER,
  error_msg       TEXT,
  format_pref     TEXT NOT NULL,
  dest_dir        TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  started_at      TEXT,
  finished_at     TEXT
);

CREATE INDEX queue_items_status_idx ON queue_items(status);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE history (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  queue_item_id   INTEGER NOT NULL REFERENCES queue_items(id),
  file_path       TEXT NOT NULL,
  bytes           INTEGER,
  completed_at    TEXT NOT NULL
);
