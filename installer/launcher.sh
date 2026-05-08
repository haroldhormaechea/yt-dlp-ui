#!/bin/sh
# /usr/bin/yt-dlp-ui — thin launcher for the bundled app at /opt/yt-dlp-ui/.
#
# `current_exe()` (used by paths.rs) returns the real binary path after
# `exec`, not the wrapper, so paths.rs Linux branch resolves bundled
# binaries (yt-dlp, deno, ad-window) at /opt/yt-dlp-ui/ correctly without
# any code change.
exec /opt/yt-dlp-ui/yt-dlp-ui "$@"
