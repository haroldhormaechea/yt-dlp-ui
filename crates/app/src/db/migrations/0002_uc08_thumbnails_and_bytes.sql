-- UC 08 — adds the columns the row delegate needs to render the design's
-- mono lines (size, downloaded) and the per-row thumbnail cache path. All
-- nullable; existing rows continue to work with NULLs.
ALTER TABLE queue_items ADD COLUMN thumbnail_path  TEXT;
ALTER TABLE queue_items ADD COLUMN size_bytes      INTEGER;
ALTER TABLE queue_items ADD COLUMN downloaded_bytes INTEGER;
