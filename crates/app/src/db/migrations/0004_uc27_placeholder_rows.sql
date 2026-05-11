-- UC 27: optimistic placeholder rows on Add.
--
-- Three new columns on `queue_items`:
--
--   * `kind` discriminates a row's meaning:
--       'video'   — a fully-known video row (today's only kind);
--       'pending' — an optimistic placeholder; enumeration has not yet
--                   resolved whether this URL is a single video or a
--                   playlist. The queue runner MUST NOT auto-promote a
--                   `pending` row to `in_flight` — only an explicit Start
--                   click sets `start_requested = 1` and triggers the
--                   downstream promotion after enumeration completes.
--
--   * `start_requested` is the latched "user clicked Start while the
--     placeholder was still resolving" intent. Reset to 0 when the
--     placeholder is promoted to a real video row (single-video case) or
--     replaced with playlist children (the intent does NOT propagate to
--     children — they pick up the user's enumeration-resolved click via
--     normal queue rules).
--
--   * `display_order` is the per-process monotonically increasing sort key
--     that replaces `created_at ASC` for queue ordering. The app seeds it
--     at startup from `SELECT COALESCE(MAX(display_order), 0) + 1_048_576`
--     and advances by 2^20 per placeholder Add; playlist expansion
--     allocates children inside the placeholder's slot via a sub-stride
--     so children stay adjacent to where the placeholder rendered.
--
-- Defaults fill existing rows ('video', 0, 0) so the migration is a pure
-- column-addition.

ALTER TABLE queue_items ADD COLUMN kind            TEXT    NOT NULL DEFAULT 'video';
ALTER TABLE queue_items ADD COLUMN start_requested INTEGER NOT NULL DEFAULT 0;
ALTER TABLE queue_items ADD COLUMN display_order   INTEGER NOT NULL DEFAULT 0;
