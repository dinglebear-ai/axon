use super::*;
use crate::youtube::dump::read_youtube_dump;

fn sample_info_json() -> Value {
    // A trimmed but representative yt-dlp `--dump-json` object. Note yt-dlp
    // names the video id `id`, NOT `video_id`.
    json!({
        "id": "dQw4w9WgXcQ",
        "title": "Never Gonna Give You Up",
        "channel": "Rick Astley",
        "channel_url": "https://www.youtube.com/@RickAstleyYT",
        "uploader_id": "RickAstleyYT",
        "upload_date": "20091025",
        "description": "The official video.",
        "duration_string": "3:33",
        "view_count": 1_000_000_u64,
        "like_count": 10_000_u64,
        "tags": ["music", "80s"],
        "categories": ["Music"],
        "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
        // yt-dlp carries no `transcript` field — it is injected by the mapper.
    })
}

#[test]
fn video_dump_json_remaps_id_to_video_id_and_injects_transcript() {
    let mapped = video_dump_json(&sample_info_json(), "never gonna give you up");
    assert_eq!(mapped["video_id"], json!("dQw4w9WgXcQ"));
    assert_eq!(mapped["title"], json!("Never Gonna Give You Up"));
    assert_eq!(mapped["transcript"], json!("never gonna give you up"));
    assert_eq!(mapped["view_count"], json!(1_000_000_u64));
    assert_eq!(mapped["tags"], json!(["music", "80s"]));
    // The raw yt-dlp `id` key must NOT survive into the dump shape.
    assert!(mapped.get("id").is_none());
}

#[test]
fn video_dump_json_tolerates_missing_fields() {
    let mapped = video_dump_json(&json!({"id": "abcdefghijk"}), "hi");
    assert_eq!(mapped["video_id"], json!("abcdefghijk"));
    assert_eq!(mapped["title"], json!(""));
    assert_eq!(mapped["view_count"], Value::Null);
    assert_eq!(mapped["tags"], json!([]));
    assert_eq!(mapped["transcript"], json!("hi"));
}

#[test]
fn mapped_dump_round_trips_through_adapter_reader() {
    // The highest-risk assertion: assemble the exact `{"videos":[...]}` shape
    // fetch_youtube_dump writes, then confirm the ADAPTER's own reader accepts
    // it and reads every field back. If field names drift, this fails.
    let video = video_dump_json(&sample_info_json(), "transcript text here");
    let dump = json!({ "videos": [video] });

    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), serde_json::to_vec(&dump).unwrap()).expect("write dump");

    let videos = read_youtube_dump(tmp.path()).expect("adapter must accept the mapped dump shape");
    assert_eq!(videos.len(), 1);
    let v = &videos[0];
    assert_eq!(v.video_id, "dQw4w9WgXcQ");
    assert_eq!(v.title, "Never Gonna Give You Up");
    assert_eq!(v.channel, "Rick Astley");
    assert_eq!(v.channel_url, "https://www.youtube.com/@RickAstleyYT");
    assert_eq!(v.uploader_id, "RickAstleyYT");
    assert_eq!(v.upload_date, "20091025");
    assert_eq!(v.description, "The official video.");
    assert_eq!(v.duration_string, "3:33");
    assert_eq!(v.view_count, Some(1_000_000));
    assert_eq!(v.like_count, Some(10_000));
    assert_eq!(v.tags, vec!["music".to_string(), "80s".to_string()]);
    assert_eq!(v.categories, vec!["Music".to_string()]);
    assert_eq!(
        v.thumbnail,
        "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"
    );
    assert_eq!(v.transcript, "transcript text here");
}

#[test]
fn canonicalize_rows_drops_empty_and_invalid_rows() {
    let rows = vec![
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
        "  ".to_string(),
        "dQw4w9WgXcQ".to_string(),
        "not a video".to_string(),
        "https://youtu.be/abcdefghijk".to_string(),
    ];
    let out = canonicalize_enumerated_video_rows(rows);
    assert_eq!(
        out,
        vec![
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            "https://www.youtube.com/watch?v=abcdefghijk".to_string(),
        ]
    );
}

#[test]
fn parse_vtt_strips_header_timestamps_and_dedupes() {
    let vtt = "WEBVTT\n\n\
        00:00:00.000 --> 00:00:02.000\n\
        <c>hello world</c>\n\n\
        00:00:02.000 --> 00:00:04.000\n\
        hello world\n\n\
        00:00:04.000 --> 00:00:06.000\n\
        goodbye\n";
    let text = parse_vtt_to_text(vtt);
    // Header + timestamps stripped; consecutive dup removed; tags stripped.
    assert_eq!(text, "hello world goodbye");
}
