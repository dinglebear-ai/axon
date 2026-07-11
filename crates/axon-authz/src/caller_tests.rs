use super::*;

#[test]
fn trusted_local_caller_sets_local_trust_and_internal_ceiling() {
    let caller = trusted_local_caller("jmagar", vec![crate::AXON_READ_SCOPE.to_string()]);
    assert!(caller.trusted_local);
    assert_eq!(caller.transport, TransportKind::Cli);
    assert_eq!(caller.auth_mode, AuthMode::TrustedLocal);
    assert_eq!(caller.caller_id.as_deref(), Some("jmagar"));
}

#[test]
fn system_caller_is_trusted_local_with_no_scopes() {
    let caller = system_caller();
    assert!(caller.trusted_local);
    assert!(caller.scopes.is_empty());
    assert_eq!(caller.transport, TransportKind::System);
}

#[test]
fn scoped_caller_never_sets_trusted_local() {
    let caller = scoped_caller(
        Some("user@example.com".to_string()),
        TransportKind::Rest,
        vec![crate::AXON_READ_SCOPE.to_string()],
        AuthMode::Oauth,
        Some("tok-1".to_string()),
        Some("User".to_string()),
    );
    assert!(!caller.trusted_local);
    assert_eq!(caller.transport, TransportKind::Rest);
    assert_eq!(caller.auth_mode, AuthMode::Oauth);
    assert_eq!(caller.token_id.as_deref(), Some("tok-1"));
}

#[test]
fn anonymous_caller_has_no_identity_or_scopes() {
    let caller = anonymous_caller(TransportKind::Mcp);
    assert!(!caller.trusted_local);
    assert!(caller.caller_id.is_none());
    assert!(caller.scopes.is_empty());
    assert_eq!(caller.auth_mode, AuthMode::None);
}
