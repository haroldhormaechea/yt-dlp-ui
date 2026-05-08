//! Format-preference selector for `yt-dlp`.
//!
//! Each variant maps to a fixed `yt-dlp` command-line argument tuple via
//! [`FormatPref::to_yt_dlp_args`]. The variant set is the user-facing menu
//! described in `PROJECT_BRIEF.md` § Use Cases — UC 01.

use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "format_test.rs"]
mod format_tests;

/// User-selectable format preference applied at download time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FormatPref {
    /// Best video stream + best audio merged into a single file.
    BestVideo,
    /// Audio-only, transcoded to MP3.
    BestAudioMp3,
    /// Audio-only, transcoded to Opus.
    BestAudioOpus,
    /// Audio-only, m4a container — AAC source streams are remuxed losslessly;
    /// opus-source streams are re-encoded to AAC by the bundled LGPL ffmpeg's
    /// native encoder (UC 17). UC 19.
    BestAudioM4a,
    /// `yt-dlp`'s built-in `bestvideo+bestaudio/best` heuristic — the default.
    #[default]
    BestHeuristic,
}

impl FormatPref {
    /// Returns the `yt-dlp` argument tuple for this preference.
    ///
    /// The result is intended to be appended directly to the argument list
    /// passed to `yt-dlp`. Order is preserved; callers should not reorder
    /// individual elements relative to one another.
    #[must_use]
    pub fn to_yt_dlp_args(self) -> Vec<String> {
        match self {
            Self::BestVideo | Self::BestHeuristic => {
                vec!["-f".to_string(), "bestvideo+bestaudio/best".to_string()]
            }
            Self::BestAudioMp3 => vec![
                "-x".to_string(),
                "--audio-format".to_string(),
                "mp3".to_string(),
            ],
            Self::BestAudioOpus => vec![
                "-x".to_string(),
                "--audio-format".to_string(),
                "opus".to_string(),
            ],
            Self::BestAudioM4a => vec![
                "-f".to_string(),
                "bestaudio[ext=m4a]/bestaudio[ext=mp4]/bestaudio".to_string(),
                "-x".to_string(),
                "--audio-format".to_string(),
                "m4a".to_string(),
            ],
        }
    }
}
