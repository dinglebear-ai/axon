use super::*;

#[test]
fn reset_is_admin_only_and_rejects_unknown_subactions() {
    assert_eq!(required_scope_for("reset", "plan"), Some("axon:admin"));
    assert_eq!(required_scope_for("reset", "exec"), Some("axon:admin"));
    assert_eq!(required_scope_for("reset", "purge"), Some("__deny__"));
}

#[test]
fn collections_is_read_only_and_rejects_mutation_subactions() {
    assert_eq!(required_scope_for("collections", "list"), Some("axon:read"));
    assert_eq!(required_scope_for("collections", "get"), Some("axon:read"));
    assert_eq!(
        required_scope_for("collections", "delete"),
        Some("__deny__")
    );
}

#[test]
fn uploads_split_read_and_write_scopes_and_reject_unknown_subactions() {
    for subaction in ["list", "get"] {
        assert_eq!(required_scope_for("uploads", subaction), Some("axon:read"));
    }
    for subaction in ["create", "put_content", "complete", "abort"] {
        assert_eq!(required_scope_for("uploads", subaction), Some("axon:write"));
    }
    assert_eq!(required_scope_for("uploads", "delete"), Some("__deny__"));
}

#[test]
fn artifacts_and_chat_are_read_scoped() {
    for subaction in ["list", "get", "content"] {
        assert_eq!(
            required_scope_for("artifacts", subaction),
            Some("axon:read")
        );
    }
    assert_eq!(required_scope_for("artifacts", "delete"), Some("__deny__"));
    assert_eq!(required_scope_for("chat", ""), Some("axon:read"));
    assert_eq!(required_scope_for("chat", "stream"), Some("__deny__"));
}

fn auth_context_with_scopes(scopes: &[&str]) -> AuthContext {
    AuthContext {
        sub: "mcp-caller".to_string(),
        actor_key: None,
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        issuer: "test".to_string(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

/// CWE-863 regression: `mutates_if_upgrade` reports `axon:write` for
/// `search`/`research`, but the broad `check_scope` (backed by
/// `axon_authz::scope_satisfies`) already treats `axon:read` as satisfying
/// `axon:write`, making the elevation a silent no-op if it were used here.
/// `check_scope_explicit` (backed by `axon_authz::has_explicit_scope`) must
/// reject a caller holding only `axon:read`.
#[test]
fn check_scope_explicit_rejects_read_only_caller_for_elevated_write() {
    let required = mutates_if_upgrade("search").expect("search is elevated to axon:write");
    assert_eq!(required, "axon:write");

    let read_only = auth_context_with_scopes(&["axon:read"]);
    // The broad check would incorrectly allow this — pin the contrast.
    assert!(scope_satisfies(&read_only.scopes, required));
    assert!(check_scope(&read_only, required, "search").is_ok());
    // The strict elevation check must reject it.
    assert!(check_scope_explicit(&read_only, required, "search").is_err());
}

#[test]
fn check_scope_explicit_allows_caller_holding_write_explicitly() {
    let required = mutates_if_upgrade("research").expect("research is elevated to axon:write");
    let writer = auth_context_with_scopes(&["axon:write"]);
    assert!(check_scope_explicit(&writer, required, "research").is_ok());
}

#[test]
fn mutates_if_upgrade_only_covers_search_and_research() {
    assert_eq!(mutates_if_upgrade("search"), Some("axon:write"));
    assert_eq!(mutates_if_upgrade("research"), Some("axon:write"));
    for action in ["ask", "evaluate", "suggest", "summarize", "query", "status"] {
        assert_eq!(
            mutates_if_upgrade(action),
            None,
            "{action} must not be silently elevated"
        );
    }
}
