use super::*;

#[test]
fn resume_route_rejects_read_only_scope() {
    assert!(!resume_scope_allowed(&["axon:read".into()]));
}

#[test]
fn resume_route_accepts_write_and_full_access_scopes() {
    assert!(resume_scope_allowed(&["axon:write".into()]));
    assert!(resume_scope_allowed(&["axon:read axon:write".into()]));
}

#[test]
fn cancel_route_requires_explicit_write_scope() {
    assert!(!cancel_scope_allowed(&["axon:read".into()]));
    assert!(cancel_scope_allowed(&["axon:write".into()]));
    assert!(cancel_scope_allowed(&["axon:read axon:write".into()]));
}
