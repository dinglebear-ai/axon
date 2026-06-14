use super::*;

// Secret-shaped fixtures are assembled at runtime from a prefix plus a benign
// body so the contiguous, scanner-recognised pattern never appears as a literal
// in this source file (keeps GitGuardian/gitleaks quiet) while still exercising
// the redactor's structured rules at full length.
fn google_api_key() -> String {
    format!("AIza{}", "a".repeat(35)) // AIza + 35
}
fn google_oauth_token() -> String {
    format!("ya29.{}", "a".repeat(40))
}
fn openai_key() -> String {
    format!("sk-{}", "a".repeat(36))
}

#[test]
fn redacts_google_api_key() {
    let key = google_api_key();
    let out = redact_secrets(&format!("error: key {key} rejected"));
    assert!(!out.contains(&key), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));
    assert!(out.contains("error:") && out.contains("rejected"));
}

#[test]
fn redacts_google_api_key_without_surrounding_whitespace() {
    // The original token-splitting redactors missed this: the secret is glued
    // to a header label with no whitespace boundary.
    let key = google_api_key();
    let out = redact_secrets(&format!("Authorization:Bearer {key}"));
    assert!(!out.contains(&key), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));

    let glued = redact_secrets(&format!("x-goog-api-key:{key}"));
    assert!(!glued.contains(&key), "leaked: {glued}");
    assert!(glued.contains(REDACTION_PLACEHOLDER));
}

#[test]
fn redacts_google_oauth_token() {
    let token = google_oauth_token();
    // Wrapped without a `token=`/`:` marker so the `ya29.` rule itself is what
    // catches it, not the assignment rule.
    let out = redact_secrets(&format!("oauth {token} end"));
    assert!(!out.contains(&token), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));
}

#[test]
fn redacts_openai_key() {
    let key = openai_key();
    let out = redact_secrets(&format!("using {key} now"));
    assert!(!out.contains(&key), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));
    assert!(out.contains("now"));
}

#[test]
fn redacts_github_tokens_all_prefixes() {
    for prefix in ["ghp", "gho", "ghu", "ghs", "ghr"] {
        // Low-entropy 36-char body (0 bits/char) so ONLY the structured
        // `gh[pousr]_` branch can match — the high-entropy fallback can't
        // mask a regression in the structured rule.
        let token = format!("{prefix}_{}", "a".repeat(36));
        let out = redact_secrets(&format!("git remote: {token}"));
        assert_eq!(out, "git remote: [REDACTED]", "leaked {prefix}: {out}");
    }
}

#[test]
fn redacts_github_token_with_long_body_without_leaking_tail() {
    // A body longer than the canonical 36 chars must be consumed whole — the
    // any-length quantifier guards against redacting only a prefix and leaking
    // the remainder. Low entropy so the fallback can't rescue it.
    let token = format!("ghp_{}", "a".repeat(50));
    let out = redact_secrets(&format!("token {token} here"));
    assert_eq!(out, "token [REDACTED] here", "leaked tail: {out}");
}

#[test]
fn redacts_short_prefixed_tokens() {
    // Token-anchored prefix rules catch short/malformed tokens too (a truncated
    // token in an error tail must not leak), matching the prior heuristics.
    for token in ["sk-short", "ghp_short", "atk_x"] {
        let out = redact_secrets(&format!("oops {token} here"));
        assert_eq!(out, "oops [REDACTED] here", "leaked {token}: {out}");
    }
}

#[test]
fn redacts_authorization_header_value() {
    let out = redact_secrets("request failed Authorization:Bearer sk-secret-value normal");
    assert!(out.contains("[REDACTED]"), "no redaction: {out}");
    assert!(out.contains("normal"));
    assert!(
        !out.contains("Authorization:Bearer"),
        "leaked header: {out}"
    );
    assert!(!out.contains("sk-secret-value"), "leaked token: {out}");
}

#[test]
fn does_not_redact_sk_dash_mid_word() {
    // The `\b` anchor keeps the `sk-` rule from firing inside ordinary words —
    // `task-force` must survive untouched.
    let benign = "the task-force will ask-questions about disk-usage";
    assert_eq!(redact_secrets(benign), benign);
}

#[test]
fn redacts_atk_token() {
    let token = format!("atk_{}", "live0abc123def456ghi0");
    let out = redact_secrets(&format!("atk field {token} end"));
    assert!(!out.contains(&token), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));
}

#[test]
fn redacts_high_entropy_run() {
    // 39-char high-entropy run that matches no named prefix (assembled from
    // halves so no contiguous high-entropy literal sits in source).
    let blob = format!("{}{}", "Zm9vYmFyMTIzNDU2Nzg", "5MGFiY2RlZmdoaWprbA9");
    assert!(blob.len() >= 32);
    let out = redact_secrets(&format!("opaque {blob} value"));
    assert!(!out.contains(&blob), "leaked: {out}");
    assert!(out.contains(REDACTION_PLACEHOLDER));
}

#[test]
fn redacts_low_entropy_structured_secret() {
    // A validly-shaped sk- key whose body is too low-entropy for the fallback
    // (0 bits/char) must still be caught by the structured `sk-` branch — this
    // fails loudly if anyone drops the structured rules in favour of entropy.
    let token = format!("sk-{}", "a".repeat(25));
    let out = redact_secrets(&format!("key {token} end"));
    assert_eq!(out, "key [REDACTED] end", "leaked: {out}");
}

#[test]
fn high_entropy_fallback_respects_32_char_length_floor() {
    // 31 distinct-ish chars: high entropy but below the `{32,}` floor → kept.
    let short = "abcdefghijklmnopqrstuvwxyz01234"; // 31 chars
    assert_eq!(short.len(), 31);
    assert_eq!(redact_secrets(short), short, "31-char run should be kept");

    // 32 chars: now over the floor → redacted.
    let long = "abcdefghijklmnopqrstuvwxyz012345"; // 32 chars
    assert_eq!(long.len(), 32);
    assert_eq!(redact_secrets(long), REDACTION_PLACEHOLDER);
}

#[test]
fn redacts_key_value_assignment_rules() {
    for raw in [
        "OPENAI_API_KEY=sk-secret", // gitleaks:allow — synthetic test fixture
        "TOKEN=atk_token",          // gitleaks:allow — synthetic test fixture
        "MY_SECRET=hunter2",        // gitleaks:allow — synthetic test fixture
        "token=abc123",             // gitleaks:allow — synthetic test fixture
    ] {
        let out = redact_secrets(raw);
        assert_eq!(out, REDACTION_PLACEHOLDER, "unexpected for {raw}: {out}");
    }
    // Case-insensitive marker matching.
    let mixed = redact_secrets("Api_Key=deadbeef"); // gitleaks:allow — synthetic test fixture
    assert_eq!(mixed, REDACTION_PLACEHOLDER);
}

#[test]
fn preserves_non_secret_text() {
    let benign = "model not found: invalid_request_error for gpt-4o-mini";
    assert_eq!(redact_secrets(benign), benign);

    let short = "the quick brown fox jumps over 12345";
    assert_eq!(redact_secrets(short), short);
}

#[test]
fn redacts_multiple_secrets_in_one_string() {
    let gkey = google_api_key();
    let skey = openai_key();
    let out = redact_secrets(&format!("k1={gkey} k2={skey} done"));
    assert!(!out.contains(&gkey), "leaked g-key: {out}");
    assert!(!out.contains(&skey), "leaked sk: {out}");
    assert!(out.contains("done"));
    assert_eq!(out.matches(REDACTION_PLACEHOLDER).count(), 2);
}

#[test]
fn preserves_low_entropy_repeated_run() {
    // A long run of a single repeated character is zero-entropy padding, not a
    // secret — the high-entropy fallback must leave it intact so downstream
    // truncation logic still sees the original length.
    let padding = "x".repeat(511);
    assert_eq!(redact_secrets(&padding), padding);

    let alternating = "ab".repeat(40); // 80 chars, 1.0 bit/char
    assert_eq!(redact_secrets(&alternating), alternating);
}

#[test]
fn empty_string_is_noop() {
    assert_eq!(redact_secrets(""), "");
}
