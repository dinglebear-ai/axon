use super::*;
use serde_json::json;

#[test]
fn payload_fields_wrap_under_display_cap() {
    let stats = json!({
        "payload_fields": [
            "arxiv_id",
            "chunk_index",
            "chunking_method",
            "devto_author",
            "domain",
            "extractor_name",
            "gh_file_language",
            "gh_file_type",
            "gh_forks",
            "gh_is_archived",
            "gh_is_fork",
            "gh_language",
            "gh_line_end",
            "gh_line_start",
            "gh_stars",
            "gh_topics"
        ]
    });

    let rendered = render_payload_fields(&stats).expect("fields should render");

    assert!(
        rendered.lines().all(
            |line| line.chars().count() <= STATS_TEXT_DISPLAY_LIMIT - STATS_CONTINUATION_INDENT
        ),
        "payload field list exceeded display cap:\n{rendered}"
    );
    assert!(rendered.contains('\n'));
}

#[test]
fn payload_fields_truncate_single_oversized_field() {
    let stats = json!({
        "payload_fields": [format!("field_{}", "x".repeat(200))]
    });

    let rendered = render_payload_fields(&stats).expect("fields should render");

    assert!(
        rendered.lines().all(
            |line| line.chars().count() <= STATS_TEXT_DISPLAY_LIMIT - STATS_CONTINUATION_INDENT
        ),
        "payload field list exceeded display cap:\n{rendered}"
    );
    assert!(rendered.contains('…'));
}

#[test]
fn fmt_age_secs_just_now() {
    assert_eq!(fmt_age_secs(0), "just now");
    assert_eq!(fmt_age_secs(59), "just now");
}

#[test]
fn fmt_age_secs_minutes() {
    assert_eq!(fmt_age_secs(60), "1m ago");
    assert_eq!(fmt_age_secs(3_599), "59m ago");
}

#[test]
fn fmt_age_secs_hours_no_minutes() {
    assert_eq!(fmt_age_secs(3_600), "1h ago");
    assert_eq!(fmt_age_secs(7_200), "2h ago");
}

#[test]
fn fmt_age_secs_hours_with_minutes() {
    assert_eq!(fmt_age_secs(3_660), "1h 1m ago");
    assert_eq!(fmt_age_secs(86_399), "23h 59m ago");
}

#[test]
fn fmt_age_secs_days_no_hours() {
    assert_eq!(fmt_age_secs(86_400), "1d ago");
    assert_eq!(fmt_age_secs(172_800), "2d ago");
}

#[test]
fn fmt_age_secs_days_with_hours() {
    assert_eq!(fmt_age_secs(90_000), "1d 1h ago");
    assert_eq!(fmt_age_secs(93_600), "1d 2h ago");
}
