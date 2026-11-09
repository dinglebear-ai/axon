use super::super::check_email_allowlist;

#[test]
fn empty_allowlist_permits_any_email() {
    assert!(check_email_allowlist(Some("anyone@example.com"), Some(true), &[]).is_ok());
}

#[test]
fn empty_allowlist_permits_even_unverified_email() {
    // When no allowlist is configured, email_verified is not enforced.
    assert!(check_email_allowlist(Some("anyone@example.com"), Some(false), &[]).is_ok());
}

#[test]
fn matching_verified_email_is_permitted() {
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(Some("alice@example.com"), Some(true), &list).is_ok());
}

#[test]
fn matching_email_is_case_insensitive() {
    // Allowlist is pre-normalized to lowercase at config load.
    // Incoming email from Google may have any case.
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(Some("Alice@Example.com"), Some(true), &list).is_ok());
}

#[test]
fn non_matching_email_is_rejected() {
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(Some("eve@example.com"), Some(true), &list).is_err());
}

#[test]
fn unverified_email_is_rejected_even_when_in_allowlist() {
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(Some("alice@example.com"), Some(false), &list).is_err());
}

#[test]
fn missing_email_verified_claim_is_rejected_when_allowlist_is_set() {
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(Some("alice@example.com"), None, &list).is_err());
}

#[test]
fn none_email_is_rejected_when_allowlist_is_set() {
    let list = vec!["alice@example.com".to_string()];
    assert!(check_email_allowlist(None, Some(true), &list).is_err());
}
