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
fn canonicalizes_enumerated_playlist_rows_to_youtube_video_urls() {
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

fn meta_fixture() -> meta::YoutubeVideoMeta {
    meta::YoutubeVideoMeta {
        title: "Video title".to_string(),
        video_id: "abc123def45".to_string(),
        thumbnail: "https://i.ytimg.com/vi/abc/default.jpg".to_string(),
        channel: "Channel".to_string(),
        channel_url: "https://www.youtube.com/@channel".to_string(),
        uploader_id: "channel".to_string(),
        upload_date: "20260101".to_string(),
        description: "Description body".to_string(),
        duration_string: "1:23".to_string(),
        view_count: Some(100),
        like_count: Some(5),
        tags: vec!["rust".to_string()],
        categories: vec!["Education".to_string()],
    }
}

#[test]
fn youtube_doc_preparation_preserves_transcript_and_description_metadata() {
    let source_url = "https://www.youtube.com/watch?v=abc123def45".to_string();
    let meta = meta_fixture();
    let docs = prepare_youtube_video_docs(
        source_url.clone(),
        &meta.title,
        "Transcript body".to_string(),
        Some(&meta),
    )
    .expect("docs");

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].url, source_url);
    assert_eq!(docs[0].source_type, "youtube");
    assert_eq!(docs[0].content_type, "text");
    assert_eq!(
        docs[0].extra.as_ref().unwrap()["yt_video_id"],
        "abc123def45"
    );
    assert_eq!(docs[0].chunk_extra[0]["chunk_content_kind"], "plain_text");
    assert_eq!(
        docs[1].url,
        "https://www.youtube.com/watch?v=abc123def45?section=description"
    );
    assert_eq!(docs[1].title.as_deref(), Some("Video title — description"));
    assert_eq!(docs[1].extra.as_ref().unwrap()["yt_channel"], "Channel");
    assert_eq!(docs[1].chunk_extra[0]["chunk_content_kind"], "plain_text");
}
