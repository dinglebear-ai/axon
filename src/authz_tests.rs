use super::{AXON_READ_SCOPE, AXON_WRITE_SCOPE, scope_satisfies};

#[test]
fn axon_read_scope_satisfies_write_routes() {
    let scopes = vec![AXON_READ_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

#[test]
fn axon_write_scope_satisfies_read_routes() {
    let scopes = vec![AXON_WRITE_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_READ_SCOPE));
}

#[test]
fn unrelated_scope_does_not_satisfy_axon_routes() {
    let scopes = vec!["other:read".to_string()];
    assert!(!scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

#[test]
fn non_axon_scopes_still_require_exact_match() {
    let scopes = vec!["other:read".to_string()];
    assert!(scope_satisfies(&scopes, "other:read"));
    assert!(!scope_satisfies(&scopes, "other:write"));
}
