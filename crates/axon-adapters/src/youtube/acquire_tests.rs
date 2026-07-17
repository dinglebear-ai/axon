use super::*;

#[test]
fn cache_path_is_deterministic_per_target() {
    let a = youtube_cache_path("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    let b = youtube_cache_path("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(a, b, "same target must map to the same cache path");

    // Whitespace around the target does not change the derived path (trimmed).
    assert_eq!(
        youtube_cache_path("  https://www.youtube.com/watch?v=dQw4w9WgXcQ  "),
        a
    );
}

#[test]
fn cache_path_differs_per_target() {
    assert_ne!(
        youtube_cache_path("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        youtube_cache_path("https://www.youtube.com/watch?v=abcdefghijk")
    );
}

#[test]
fn cache_path_lives_under_axon_youtube_dir() {
    let path = youtube_cache_path("dQw4w9WgXcQ");
    assert_eq!(
        path.parent().and_then(|p| p.file_name()),
        Some(std::ffi::OsStr::new("axon-youtube")),
        "dump path must live under <tmp>/axon-youtube/"
    );
    assert_eq!(
        path.extension(),
        Some(std::ffi::OsStr::new("json")),
        "dump path must be a .json file"
    );
    assert!(path.starts_with(std::env::temp_dir()));
}

#[tokio::test]
async fn fetch_youtube_dump_rejects_invalid_target_before_any_subprocess() {
    // An invalid target fails at parse time — before yt-dlp is ever spawned and
    // before any dump file is written. No network / subprocess required.
    let err = fetch_youtube_dump("not a valid youtube target!!")
        .await
        .expect_err("invalid target must fail");
    assert!(
        err.to_string().contains("invalid youtube target"),
        "expected invalid-target error, got: {err}"
    );
    assert!(
        !youtube_cache_path("not a valid youtube target!!").exists(),
        "no dump should be written for an invalid target"
    );
}

#[test]
fn ytdlp_binary_defaults_to_yt_dlp() {
    // With no override env set in the test harness, the default binary is
    // `yt-dlp` on PATH. (We do not mutate process env here — this crate denies
    // `unsafe`, which `set_var` now requires — so we only assert the default.)
    if std::env::var("AXON_YTDLP").is_err() && std::env::var("YT_DLP").is_err() {
        assert_eq!(ytdlp_binary(), "yt-dlp");
    }
}
