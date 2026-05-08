//! Tests for [`crate::format::FormatPref::to_yt_dlp_args`].

use crate::format::FormatPref;

#[test]
fn snapshot_best_video() {
    let args = FormatPref::BestVideo.to_yt_dlp_args();
    insta::assert_debug_snapshot!(args);
}

#[test]
fn snapshot_best_heuristic() {
    let args = FormatPref::BestHeuristic.to_yt_dlp_args();
    insta::assert_debug_snapshot!(args);
}

#[test]
fn snapshot_best_audio_mp3() {
    let args = FormatPref::BestAudioMp3.to_yt_dlp_args();
    insta::assert_debug_snapshot!(args);
}

#[test]
fn snapshot_best_audio_opus() {
    let args = FormatPref::BestAudioOpus.to_yt_dlp_args();
    insta::assert_debug_snapshot!(args);
}

#[test]
fn snapshot_best_audio_m4a() {
    let args = FormatPref::BestAudioM4a.to_yt_dlp_args();
    insta::assert_debug_snapshot!(args);
}

#[test]
fn default_is_best_heuristic() {
    let default = FormatPref::default();
    assert_eq!(default, FormatPref::BestHeuristic);
}

#[test]
fn serde_round_trip_each_variant() {
    for variant in [
        FormatPref::BestVideo,
        FormatPref::BestAudioMp3,
        FormatPref::BestAudioOpus,
        FormatPref::BestAudioM4a,
        FormatPref::BestHeuristic,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: FormatPref = serde_json::from_str(&json).unwrap();
        assert_eq!(
            variant, back,
            "round-trip preserves variant for {variant:?}"
        );
    }
}

#[test]
fn args_are_appendable_token_strings() {
    // Each token must be a single non-empty string (no embedded shell metachars
    // expected — yt-dlp is invoked via Command::new directly).
    for variant in [
        FormatPref::BestVideo,
        FormatPref::BestAudioMp3,
        FormatPref::BestAudioOpus,
        FormatPref::BestAudioM4a,
        FormatPref::BestHeuristic,
    ] {
        let args = variant.to_yt_dlp_args();
        assert!(!args.is_empty(), "variant {variant:?} must produce args");
        for arg in args {
            assert!(!arg.is_empty(), "no empty argument tokens");
        }
    }
}
