use super::{
    AXON_ADMIN_SCOPE, AXON_EXECUTE_SCOPE, AXON_FULL_ACCESS_SCOPE, AXON_LOCAL_SCOPE,
    AXON_READ_SCOPE, AXON_WRITE_SCOPE, has_explicit_scope,
    http::{issuable_scopes, static_operator_scopes},
    scope_satisfies,
};

#[test]
fn axon_read_scope_satisfies_legacy_write_routes() {
    let scopes = vec![AXON_READ_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

/// The strict counterpart to
/// `axon_read_scope_satisfies_write_routes_by_design_compatibility_widening`:
/// `has_explicit_scope` must NOT apply the broad-scope widening, otherwise
/// elevation checks built on top of it (e.g. `require_mutates_if_write_scope`
/// in `axon-web`, `mutates_if_upgrade`/`check_scope_explicit` in `axon-mcp`)
/// would be silent no-ops, same as the original CWE-863 finding this type
/// exists to close.
#[test]
fn explicit_scope_check_rejects_broad_widening() {
    let scopes = vec![AXON_READ_SCOPE.to_string()];
    assert!(scope_satisfies(&scopes, AXON_WRITE_SCOPE));
    assert!(!has_explicit_scope(&scopes, AXON_WRITE_SCOPE));

    let write_scopes = vec![AXON_WRITE_SCOPE.to_string()];
    assert!(has_explicit_scope(&write_scopes, AXON_WRITE_SCOPE));
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
fn fine_grained_scopes_are_issuable_by_http_auth() {
    let scopes = issuable_scopes();
    for required in [
        AXON_READ_SCOPE,
        AXON_WRITE_SCOPE,
        AXON_ADMIN_SCOPE,
        AXON_EXECUTE_SCOPE,
        AXON_LOCAL_SCOPE,
    ] {
        assert!(
            scopes.iter().any(|scope| scope == required),
            "missing issuable scope {required}"
        );
    }
}

#[test]
fn static_operator_bearer_does_not_gain_execute_or_local() {
    let scopes = static_operator_scopes();
    assert!(scopes.iter().any(|scope| scope == AXON_READ_SCOPE));
    assert!(scopes.iter().any(|scope| scope == AXON_WRITE_SCOPE));
    assert!(scopes.iter().any(|scope| scope == AXON_ADMIN_SCOPE));
    assert!(!scopes.iter().any(|scope| scope == AXON_EXECUTE_SCOPE));
    assert!(!scopes.iter().any(|scope| scope == AXON_LOCAL_SCOPE));
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
fn fine_grained_scope_holder_does_not_gain_broad_access() {
    let scopes = vec![AXON_LOCAL_SCOPE.to_string()];
    assert!(!scope_satisfies(&scopes, AXON_READ_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_WRITE_SCOPE));
}

#[test]
fn space_separated_fine_grained_scope_is_recognized() {
    let scopes = vec![format!("{AXON_READ_SCOPE} {AXON_LOCAL_SCOPE}")];
    assert!(scope_satisfies(&scopes, AXON_LOCAL_SCOPE));
    assert!(scope_satisfies(&scopes, AXON_READ_SCOPE));
    assert!(!scope_satisfies(&scopes, AXON_EXECUTE_SCOPE));
}

#[test]
fn oauth_scope_implication_matrix_is_exhaustive() {
    let scopes = [
        AXON_READ_SCOPE,
        AXON_WRITE_SCOPE,
        AXON_ADMIN_SCOPE,
        AXON_EXECUTE_SCOPE,
        AXON_LOCAL_SCOPE,
    ];
    for held in scopes {
        for required in scopes {
            let expected = held == required
                || (matches!(held, AXON_READ_SCOPE | AXON_WRITE_SCOPE)
                    && matches!(required, AXON_READ_SCOPE | AXON_WRITE_SCOPE));
            assert_eq!(
                scope_satisfies(&[held.to_string()], required),
                expected,
                "unexpected implication {held} -> {required}",
            );
        }
    }
}
