use super::{
    AXON_ADMIN_SCOPE, AXON_EXECUTE_SCOPE, AXON_FULL_ACCESS_SCOPE, AXON_LOCAL_SCOPE,
    AXON_READ_SCOPE, AXON_WRITE_SCOPE, scope_satisfies,
};

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

#[test]
fn write_scope_does_not_imply_admin_execute_or_local() {
    let scopes = vec![AXON_WRITE_SCOPE.to_string()];
    assert!(!scope_satisfies(&scopes, AXON_ADMIN_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
}

#[test]
fn full_access_scope_does_not_imply_fine_grained_scopes() {
    let scopes = vec![AXON_FULL_ACCESS_SCOPE.to_string()];
    assert!(!scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
    // ...but full access still satisfies the broad read/write groups.
    assert!(scope_satisfies(&scopes, AXON_READ_SCOPE));
    assert!(scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

#[test]
fn fine_grained_scope_requires_exact_hold() {
    let scopes = vec![AXON_EXECUTE_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_ADMIN_SCOPE));
}

#[test]
fn fine_grained_scope_holder_still_satisfies_broad_groups() {
    // A caller holding only a fine-grained scope counts as authenticated Axon
    // access for the broad read/write route groups.
    let scopes = vec![AXON_LOCAL_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_READ_SCOPE));
    assert!(scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

#[test]
fn space_separated_fine_grained_scope_is_recognized() {
    let scopes = vec![format!("{AXON_READ_SCOPE} {AXON_LOCAL_SCOPE}")];
    assert!(scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
    assert!(scope_satisfies(&scopes, AXON_READ_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
}
