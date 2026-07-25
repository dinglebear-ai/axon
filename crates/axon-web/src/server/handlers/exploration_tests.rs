use super::*;

fn auth_ctx(scopes: &[&str]) -> AuthContext {
    AuthContext {
        sub: "tester".to_string(),
        actor_key: None,
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        issuer: "test".to_string(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

/// `require_mutates_if_write_scope` exists specifically so `/v1/search` and
/// `/v1/research` — nominally `axon:read` routes that unconditionally enqueue
/// Source jobs — cannot be reached by a read-only caller. This pins the
/// CWE-863 fix: an `axon:read`-only caller must be rejected even though
/// `axon_authz::scope_satisfies(["axon:read"], "axon:write")` returns `true`
/// (the deliberate broad-scope compatibility widening documented on
/// `has_explicit_scope`). If this regresses back to `scope_satisfies`, this
/// test fails.
#[test]
fn read_only_caller_is_rejected_by_mutates_if_elevation() {
    assert!(axon_authz::scope_satisfies(
        &["axon:read".to_string()],
        "axon:write"
    ));

    let ext = Extension(auth_ctx(&["axon:read"]));
    let result = require_mutates_if_write_scope(Some(&ext));
    assert!(
        result.is_err(),
        "axon:read-only caller must not pass the axon:write elevation check"
    );
}

#[test]
fn explicit_write_scope_caller_passes_mutates_if_elevation() {
    let ext = Extension(auth_ctx(&["axon:write"]));
    assert!(require_mutates_if_write_scope(Some(&ext)).is_ok());
}

#[test]
fn full_access_scope_caller_passes_mutates_if_elevation() {
    // AXON_FULL_ACCESS_SCOPE ("axon:read axon:write") is issued to fully
    // authorized OAuth users and holds axon:write explicitly, so it must
    // still pass the strict elevation check.
    let ext = Extension(auth_ctx(&["axon:read axon:write"]));
    assert!(require_mutates_if_write_scope(Some(&ext)).is_ok());
}

#[test]
fn loopback_dev_with_no_auth_context_is_allowed() {
    assert!(require_mutates_if_write_scope(None).is_ok());
}
