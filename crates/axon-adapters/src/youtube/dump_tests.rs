use super::*;

use std::fs;
use std::path::PathBuf;

fn scratch_file(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-youtube-dump-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

const VALID_DUMP: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up",
      "channel": "Rick Astley",
      "channel_url": "https://www.youtube.com/@RickAstleyYT",
      "uploader_id": "RickAstleyYT",
      "upload_date": "20091025",
      "description": "The official video.",
      "duration_string": "3:33",
      "view_count": 1000000,
      "like_count": 10000,
      "tags": ["music", "80s"],
      "categories": ["Music"],
      "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg",
      "transcript": "Never gonna give you up, never gonna let you down"
    }
  ]
}"#;

#[test]
fn reads_valid_dump() {
    let path = scratch_file("dump.json", VALID_DUMP);
    let videos = read_youtube_dump(&path).unwrap();
    assert_eq!(videos.len(), 1);
    let video = &videos[0];
    assert_eq!(video.video_id, "dQw4w9WgXcQ");
    assert_eq!(video.title, "Never Gonna Give You Up");
    assert_eq!(video.channel, "Rick Astley");
    assert_eq!(video.view_count, Some(1_000_000));
    assert_eq!(video.like_count, Some(10_000));
    assert_eq!(video.tags, vec!["music".to_string(), "80s".to_string()]);
    assert!(video.transcript.contains("Never gonna give you up"));
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn reads_dump_with_minimal_required_fields() {
    let minimal = r#"{"videos":[{"video_id":"abc12345678"}]}"#;
    let path = scratch_file("minimal.json", minimal);
    let videos = read_youtube_dump(&path).unwrap();
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].video_id, "abc12345678");
    assert_eq!(videos[0].title, "");
    assert_eq!(videos[0].transcript, "");
    assert!(videos[0].tags.is_empty());
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn empty_videos_array_parses_to_empty_vec() {
    let path = scratch_file("empty.json", r#"{"videos": []}"#);
    let videos = read_youtube_dump(&path).unwrap();
    assert!(videos.is_empty());
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn missing_videos_key_parses_to_empty_vec() {
    let path = scratch_file("no-videos-key.json", r#"{}"#);
    let videos = read_youtube_dump(&path).unwrap();
    assert!(videos.is_empty());
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn malformed_json_errors() {
    let path = scratch_file("malformed.json", "{ this is not json ");
    let err = read_youtube_dump(&path).unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.youtube.dump.invalid");
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn missing_video_id_errors() {
    let path = scratch_file("no-id.json", r#"{"videos":[{"title":"No ID here"}]}"#);
    let err = read_youtube_dump(&path).unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "adapter.youtube.dump.video_id.missing"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn blank_video_id_errors() {
    let path = scratch_file("blank-id.json", r#"{"videos":[{"video_id":"   "}]}"#);
    let err = read_youtube_dump(&path).unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "adapter.youtube.dump.video_id.missing"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn missing_file_errors() {
    let path = std::env::temp_dir().join("axon-youtube-dump-test-does-not-exist.json");
    let err = read_youtube_dump(&path).unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.youtube.dump.read_failed");
}
