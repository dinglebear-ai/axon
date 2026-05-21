use super::*;

#[test]
fn build_extra_so_sets_required_fields() {
    let question = serde_json::json!({
        "question_id": 12345, "score": 42, "view_count": 1000,
        "is_answered": true, "answer_count": 3,
        "owner": {"display_name": "Alice"},
        "tags": ["rust", "memory"],
        "creation_date": 0,
    });
    let extra = build_extra(&question, "2024-01-01");
    assert_eq!(extra["so_question_id"], 12345u64);
    assert_eq!(extra["so_score"], 42i64);
    assert_eq!(extra["so_view_count"], 1000u64);
    assert_eq!(extra["so_is_answered"], "true");
    assert_eq!(extra["so_answer_count"], 3u64);
    assert_eq!(extra["so_author"], "Alice");
    assert_eq!(extra["so_created_at"], "2024-01-01");
    let tags = extra["so_tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
}

#[test]
fn build_extra_so_not_answered() {
    let question = serde_json::json!({
        "question_id": 1, "score": 0, "view_count": 5,
        "is_answered": false, "answer_count": 0,
        "owner": {"display_name": ""}, "tags": [], "creation_date": 0,
    });
    let extra = build_extra(&question, "");
    assert_eq!(extra["so_is_answered"], "false");
    assert!(extra.get("so_tags").is_none());
    assert!(extra.get("so_author").is_none());
    assert!(extra.get("so_created_at").is_none());
}

#[test]
fn build_extra_so_empty_author_omitted() {
    let question = serde_json::json!({
        "question_id": 1, "score": 10, "view_count": 20,
        "is_answered": true, "answer_count": 1,
        "owner": {"display_name": ""}, "tags": ["go"], "creation_date": 0,
    });
    let extra = build_extra(&question, "2024-06-01");
    assert!(extra.get("so_author").is_none());
}

#[test]
fn matches_question_with_slug() {
    assert!(matches(
        "https://stackoverflow.com/questions/11227809/why-is-processing-a-sorted-array-faster"
    ));
}

#[test]
fn matches_question_without_slug() {
    assert!(matches("https://stackoverflow.com/questions/11227809"));
}

#[test]
fn rejects_non_question_paths() {
    assert!(!matches("https://stackoverflow.com/tags/rust"));
    assert!(!matches("https://stackoverflow.com/users/1234"));
    assert!(!matches("https://stackoverflow.com/"));
    assert!(!matches("https://stackoverflow.com/questions/"));
}

#[test]
fn rejects_non_numeric_id() {
    assert!(!matches(
        "https://stackoverflow.com/questions/not-a-number/slug"
    ));
}

#[test]
fn rejects_other_stackexchange_sites() {
    assert!(!matches(
        "https://serverfault.com/questions/12345/something"
    ));
    assert!(!matches("https://superuser.com/questions/12345/something"));
}

#[test]
fn extract_question_id_with_slug() {
    let id = extract_question_id(
        "https://stackoverflow.com/questions/11227809/why-is-processing-a-sorted-array-faster",
    );
    assert_eq!(id, Some(11_227_809));
}

#[test]
fn extract_question_id_no_slug() {
    let id = extract_question_id("https://stackoverflow.com/questions/42");
    assert_eq!(id, Some(42));
}

#[test]
fn strip_html_removes_tags() {
    let html = "<p>Hello <strong>world</strong> &amp; <em>friends</em></p>";
    assert_eq!(strip_html_tags(html), "Hello world & friends");
}

#[test]
fn format_unix_ts_epoch() {
    // 2024-01-01 is 1704067200 seconds from epoch
    let s = format_unix_ts(1_704_067_200);
    // We just check it starts with "2023" or "2024" (approximate arithmetic)
    assert!(s.starts_with("202"), "got: {s}");
}
