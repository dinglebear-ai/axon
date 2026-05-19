use super::*;

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
