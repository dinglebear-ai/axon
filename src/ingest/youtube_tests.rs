use super::*;

#[test]
fn extracts_video_ids_from_supported_target_forms() {
    for input in [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ",
        "dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
        "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
    ] {
        assert_eq!(extract_video_id(input), Some("dQw4w9WgXcQ".to_string()));
    }
}

#[test]
fn detects_playlist_and_channel_urls() {
    for input in [
        "https://www.youtube.com/playlist?list=UUZDfnUn74N0WeAPvMqTOrtA",
        "https://www.youtube.com/@SpaceinvaderOne",
        "https://www.youtube.com/channel/UCZDfnUn74N0WeAPvMqTOrtA",
        "https://www.youtube.com/c/SpaceinvaderOne",
    ] {
        assert!(is_playlist_or_channel_url(input));
    }
    assert!(!is_playlist_or_channel_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
}

#[test]
fn canonicalizes_enumerated_playlist_rows_to_watch_urls() {
    let rows = vec![
        "dQw4w9WgXcQ".to_string(),
        " https://youtu.be/abcDEF123_4 ".to_string(),
        "https://www.youtube.com/shorts/ZYxwvUTsr-1".to_string(),
        "".to_string(),
        "not a video".to_string(),
    ];

    assert_eq!(
        canonicalize_enumerated_video_rows(rows),
        vec![
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=abcDEF123_4",
            "https://www.youtube.com/watch?v=ZYxwvUTsr-1",
        ]
    );
}

#[test]
fn classifies_youtube_targets_for_source_dispatch() {
    for (target, kind) in [
        ("dQw4w9WgXcQ", YoutubeTargetKind::SingleVideo),
        (
            "https://www.youtube.com/@SpaceinvaderOne",
            YoutubeTargetKind::PlaylistOrChannel,
        ),
        ("@SpaceinvaderOne", YoutubeTargetKind::PlaylistOrChannel),
    ] {
        assert_eq!(classify_youtube_target(target).unwrap(), kind);
    }
    assert!(classify_youtube_target("https://example.com/nope").is_err());
}
