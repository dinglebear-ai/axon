use super::*;

#[test]
fn first_trust_is_bound_to_the_confirmed_fingerprint() {
    assert!(confirmed_fingerprint_matches(
        Some("SHA256:key-a"),
        "SHA256:key-a"
    ));
    assert!(!confirmed_fingerprint_matches(
        Some("SHA256:key-a"),
        "SHA256:key-b"
    ));
    assert!(!confirmed_fingerprint_matches(None, "SHA256:key-a"));
}
