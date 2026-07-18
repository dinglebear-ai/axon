use super::*;

// Fixtures use unambiguous `EXAMPLE-*` placeholders rather than realistic
// token shapes: the URL-aware path redacts userinfo and secret-bearing query
// values by structure, not by matching a secret pattern, so the placeholder
// content is irrelevant to what these assert — and keeps secret scanners quiet.

#[test]
fn redacts_userinfo_password_keeps_host_and_path() {
    let out = redact_status_subject("https://user:EXAMPLE-pw-not-real@example.com/path/page");
    assert!(
        !out.contains("EXAMPLE-pw-not-real"),
        "password leaked: {out}"
    );
    assert!(
        out.contains("example.com/path/page"),
        "host/path lost: {out}"
    );
}

#[test]
fn redacts_credential_in_userinfo_username() {
    // `https://<cred>@host` — the credential is the username (e.g. a token used
    // as the username in a git remote). URL-aware redaction masks all userinfo.
    let out = redact_status_subject("https://EXAMPLE-userinfo-cred@github.com/owner/repo");
    assert!(
        !out.contains("EXAMPLE-userinfo-cred"),
        "username credential leaked: {out}"
    );
    assert!(
        out.contains("github.com/owner/repo"),
        "host/path lost: {out}"
    );
}

#[test]
fn redacts_only_sensitive_query_value() {
    let out = redact_status_subject(
        "https://api.example.com/v1/data?access_token=EXAMPLE-token-value&page=2",
    );
    assert!(
        !out.contains("EXAMPLE-token-value"),
        "token value leaked: {out}"
    );
    assert!(
        out.contains("access_token=REDACTED"),
        "key/marker lost: {out}"
    );
    assert!(out.contains("page=2"), "non-sensitive param dropped: {out}");
    assert!(
        out.contains("api.example.com/v1/data"),
        "host/path lost: {out}"
    );
}

#[test]
fn preserves_ordinary_url_byte_for_byte() {
    let subject = "https://www.reddit.com/r/rust/";
    assert_eq!(redact_status_subject(subject), subject);
}

#[test]
fn preserves_url_with_long_slug_unchanged() {
    // The shared high-entropy rule (32+ char runs) would partially redact this
    // slug; the URL-aware path must leave non-secret path segments intact —
    // this is the second half of the reported over-redaction bug.
    let subject = "https://corrode.dev/blog/2023-11-12-announcing-tokio-console-0-1-8/";
    assert_eq!(redact_status_subject(subject), subject);
}

#[test]
fn preserves_url_that_merely_contains_token_substring() {
    // The exact class the old blunt redactor wholesale-[REDACTED]'d: a URL with
    // `access_token=` in the query must keep its structure, redacting only the
    // value — never collapse the whole URL.
    let subject = "https://example.com/oauth/callback?access_token=EXAMPLE&state=ok";
    let out = redact_status_subject(subject);
    assert_ne!(out, "[REDACTED]", "whole URL was wholesale-redacted: {out}");
    assert!(
        out.contains("example.com/oauth/callback"),
        "host/path lost: {out}"
    );
    assert!(
        out.contains("state=ok"),
        "non-sensitive param dropped: {out}"
    );
    assert!(
        !out.contains("access_token=EXAMPLE"),
        "token value leaked: {out}"
    );
}

#[test]
fn non_url_subject_routes_to_full_scrubber_unchanged_when_clean() {
    // `source_type: target` style labels aren't URLs, so they route to the full
    // `redact_secrets` scrubber (whose own scrubbing is covered by redact_tests).
    // With nothing sensitive present, that path is a passthrough.
    let subject = "reddit: r/rust";
    assert_eq!(redact_status_subject(subject), subject);
}
