use super::*;

#[test]
fn redacts_lines_with_authorization_headers() {
    let (out, redacted) = redact_text("ok\nAuthorization: Bearer sk-abc123\nmore-ok");
    assert!(redacted);
    assert!(!out.contains("sk-abc123"));
    assert!(out.contains("ok"));
    assert!(out.contains("more-ok"));
}

#[test]
fn leaves_clean_output_untouched() {
    let (out, redacted) = redact_text("hello\nworld");
    assert_eq!(out, "hello\nworld");
    assert!(!redacted);
}

#[test]
fn redacts_password_shaped_lines() {
    let (out, redacted) = redact_text("db_password=hunter2");
    assert_eq!(out, "db_password=[REDACTED]");
    assert!(redacted);
}

#[test]
fn preserves_authentication_documentation_and_benign_configuration() {
    let input = "Bearer authentication uses an Authorization header.\nPORT=3000";
    let (out, redacted) = redact_text(input);
    assert_eq!(out, input);
    assert!(!redacted);
}
