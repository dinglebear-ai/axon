use super::*;

#[test]
fn extracts_video_id_from_watch_url() {
    assert_eq!(
        extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
}

#[test]
fn extracts_video_id_from_short_url() {
    assert_eq!(
        extract_video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
}

#[test]
fn extracts_video_id_from_shorts_and_embed_paths() {
    assert_eq!(
        extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
    assert_eq!(
        extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
}

#[test]
fn extracts_bare_video_id() {
    assert_eq!(
        extract_video_id("dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
}

#[test]
fn rejects_non_video_id_bare_string() {
    assert_eq!(extract_video_id("not-a-video-id"), None);
    assert_eq!(extract_video_id("short"), None);
}

#[test]
fn detects_playlist_and_channel_urls() {
    assert!(is_playlist_or_channel_url(
        "https://www.youtube.com/playlist?list=PLabc123"
    ));
    assert!(is_playlist_or_channel_url(
        "https://www.youtube.com/channel/UCabc123"
    ));
    assert!(is_playlist_or_channel_url(
        "https://www.youtube.com/@SomeHandle"
    ));
    assert!(is_playlist_or_channel_url(
        "https://www.youtube.com/c/SomeChannel"
    ));
    assert!(is_playlist_or_channel_url(
        "https://www.youtube.com/user/SomeUser"
    ));
    assert!(!is_playlist_or_channel_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
}

#[test]
fn watch_url_with_list_and_v_is_not_playlist() {
    // A watch URL that happens to carry a `list=` param alongside `v=` is
    // still a single-video target — the video takes priority.
    assert!(!is_playlist_or_channel_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLabc123"
    ));
}

#[test]
fn parse_youtube_target_classifies_video() {
    let target = parse_youtube_target("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    assert_eq!(target.scope, SourceScope::Video);
    assert_eq!(target.video_id.as_deref(), Some("dQw4w9WgXcQ"));
    assert_eq!(
        target.canonical_uri,
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    );
}

#[test]
fn parse_youtube_target_classifies_channel() {
    let target = parse_youtube_target("https://www.youtube.com/@SomeHandle").unwrap();
    assert_eq!(target.scope, SourceScope::Channel);
    assert_eq!(target.video_id, None);
}

#[test]
fn parse_youtube_target_normalizes_bare_handle() {
    let target = parse_youtube_target("@SomeHandle").unwrap();
    assert_eq!(target.scope, SourceScope::Channel);
    assert_eq!(target.canonical_uri, "https://www.youtube.com/@SomeHandle");
}

#[test]
fn parse_youtube_target_accepts_bare_video_id() {
    let target = parse_youtube_target("dQw4w9WgXcQ").unwrap();
    assert_eq!(target.scope, SourceScope::Video);
    assert_eq!(target.video_id.as_deref(), Some("dQw4w9WgXcQ"));
}

#[test]
fn parse_youtube_target_rejects_garbage() {
    let err = parse_youtube_target("not a youtube target at all").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.youtube.target.invalid");
}
