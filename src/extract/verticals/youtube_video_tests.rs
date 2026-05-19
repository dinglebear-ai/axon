use super::*;

#[test]
fn matches_watch_url() {
    assert!(matches("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
    assert!(matches("https://youtube.com/watch?v=abc123"));
    assert!(matches("https://m.youtube.com/watch?v=abc123"));
}

#[test]
fn matches_youtu_be() {
    assert!(matches("https://youtu.be/dQw4w9WgXcQ"));
    assert!(!matches("https://youtu.be/"));
}

#[test]
fn matches_shorts_and_live() {
    assert!(matches("https://www.youtube.com/shorts/abc123"));
    assert!(matches("https://www.youtube.com/live/abc123"));
    assert!(matches("https://www.youtube.com/embed/abc123"));
}

#[test]
fn rejects_non_youtube() {
    assert!(!matches("https://vimeo.com/123456"));
    assert!(!matches("https://youtube.com/channel/abc"));
    assert!(!matches("https://youtube.com/"));
}

#[test]
fn rejects_missing_v_param() {
    assert!(!matches("https://www.youtube.com/watch?list=PL123"));
}

#[test]
fn format_duration_seconds() {
    assert_eq!(format_duration(65), "1:05");
    assert_eq!(format_duration(3600), "1:00:00");
    assert_eq!(format_duration(3661), "1:01:01");
    assert_eq!(format_duration(59), "0:59");
}

#[test]
fn format_view_count_units() {
    assert_eq!(format_view_count(999), "999");
    assert_eq!(format_view_count(1_500), "1.5K");
    assert_eq!(format_view_count(2_500_000), "2.5M");
    assert_eq!(format_view_count(1_200_000_000), "1.2B");
}

#[test]
fn brace_count_handles_nested_json() {
    let html = r#"var ytInitialPlayerResponse = {"videoDetails":{"title":"Test","author":"Chan","viewCount":"1000","lengthSeconds":"180","shortDescription":"desc","keywords":["a","b"]},"microformat":{"playerMicroformatRenderer":{"uploadDate":"2024-01-01","category":"Music"}},"captions":{}};"#;
    let result = extract_player_response(html);
    assert!(result.is_some());
    let v = result.unwrap();
    assert_eq!(v["videoDetails"]["title"].as_str(), Some("Test"));
}
