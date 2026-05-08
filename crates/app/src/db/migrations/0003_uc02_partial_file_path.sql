-- UC 02: capture the on-disk path of yt-dlp's `.part` file so Remove can
-- delete it from disk. Set by the bridge when it parses the
-- `[download] Destination: <path>` line from yt-dlp stdout; cleared on
-- restart-for-resume by the manager.

ALTER TABLE queue_items ADD COLUMN partial_file_path TEXT;
