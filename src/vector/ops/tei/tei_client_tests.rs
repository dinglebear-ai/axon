use super::redact_url_for_log;

#[test]
fn redact_url_for_log_removes_credentials_query_and_fragment() {
    let redacted = redact_url_for_log("http://user:secret@tei.example:8080/embed?token=abc#frag");

    assert_eq!(
        redacted,
        "http://%3Credacted%3E:%3Credacted%3E@tei.example:8080/embed"
    );
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("token=abc"));
}

#[test]
fn redact_url_for_log_handles_unparseable_urls() {
    assert_eq!(redact_url_for_log("not a url?token=secret"), "not a url");
}
