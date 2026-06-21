use super::*;

#[test]
fn code_challenge_matches_rfc7636_test_vector() {
    // RFC 7636 Appendix B canonical pair.
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        code_challenge_s256(verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn generated_verifier_is_valid_pkce_shape() {
    let verifier = generate_code_verifier();
    // RFC 7636 §4.1: 43..=128 chars from the unreserved set.
    assert_eq!(
        verifier.len(),
        43,
        "32 bytes base64url-nopad is always 43 chars"
    );
    assert!(
        verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')),
        "verifier contains a reserved character: {verifier}"
    );
}

#[test]
fn generated_values_are_unique_per_call() {
    assert_ne!(generate_code_verifier(), generate_code_verifier());
    assert_ne!(generate_state(), generate_state());
}
