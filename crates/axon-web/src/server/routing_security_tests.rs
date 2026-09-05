use super::*;

#[test]
fn panel_identity_is_scoped_and_distinct_from_rest_bearer() {
    let auth = panel_auth_context();

    assert_eq!(auth.issuer, PANEL_AUTH_ISSUER);
    assert_eq!(auth.sub, "axon-panel");
    assert_eq!(auth.scopes, ["axon:read", "axon:write"]);
    assert!(!auth.scopes.iter().any(|scope| scope == "axon:admin"));
    assert!(!auth.scopes.iter().any(|scope| scope == "axon:local"));
}
