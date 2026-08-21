use super::*;

#[test]
fn redacts_lines_with_authorization_headers() {
    let token = format!("sk-{}", "a".repeat(28));
    let input = format!("ok\nAuthorization: Bearer {token}\nmore-ok");
    let (out, redacted) = redact_text(&input);
    assert!(redacted);
    assert!(!out.contains(&token));
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
    let (out, redacted) = redact_text("db_password=thisisarealpassphrase");
    assert_eq!(out, "db_password=[REDACTED]");
    assert!(redacted);
}

#[test]
fn redacts_standalone_bearer_and_passwd_assignments() {
    let input = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature\npasswd=thisisarealpassphrase";
    let (out, redacted) = redact_text(input);

    assert!(redacted);
    assert!(!out.contains("eyJhbGciOiJIUzI1NiJ9"));
    assert!(!out.contains("thisisarealpassphrase"));
    assert_eq!(out, "Bearer [REDACTED]\npasswd=[REDACTED]");
}

#[test]
fn preserves_authentication_documentation_and_benign_configuration() {
    let input = "Bearer authentication uses an Authorization header.\nPORT=3000";
    let (out, redacted) = redact_text(input);
    assert_eq!(out, input);
    assert!(!redacted);
}

#[test]
fn preserves_low_confidence_tutorial_credential_syntax() {
    let input = concat!(
        "Authorization: Bearer abc123\n",
        "TOKEN=abc123\n",
        "passwd=hunter2\n",
        "postgres://user:password@localhost/app",
    );
    let (out, redacted) = redact_text(input);
    assert_eq!(out, input);
    assert!(!redacted);
}
