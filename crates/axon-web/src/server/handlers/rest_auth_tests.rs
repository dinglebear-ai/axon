use axon_authz::http::scope_for_action;

#[test]
fn unknown_action_is_explicitly_denied() {
    assert_eq!(scope_for_action("future_action", None), Some("__deny__"));
}

#[test]
fn source_lifecycle_requires_write() {
    assert_eq!(scope_for_action("source", None), Some("axon:write"));
    assert_eq!(scope_for_action("watch", None), Some("axon:write"));
}

#[test]
fn read_surface_requires_read() {
    assert_eq!(scope_for_action("sources", None), Some("axon:read"));
    assert_eq!(scope_for_action("query", None), Some("axon:read"));
    assert_eq!(scope_for_action("ask", None), Some("axon:read"));
}

#[test]
fn mutating_subaction_on_read_family_requires_write() {
    assert_eq!(
        scope_for_action("sources", Some("create")),
        Some("axon:write")
    );
    assert_eq!(scope_for_action("jobs", Some("cancel")), Some("axon:write"));
}

#[test]
fn destructive_actions_require_admin() {
    assert_eq!(scope_for_action("prune", None), Some("axon:admin"));
    assert_eq!(scope_for_action("reset", None), Some("axon:admin"));
    assert_eq!(scope_for_action("dedupe", None), Some("axon:admin"));
}

#[test]
fn tool_execution_requires_execute_scope() {
    assert_eq!(scope_for_action("cli_tool", None), Some("axon:execute"));
    assert_eq!(scope_for_action("mcp_tool", None), Some("axon:execute"));
}

#[test]
fn local_source_requires_local_scope() {
    assert_eq!(scope_for_action("local", None), Some("axon:local"));
}

#[test]
fn help_carries_no_scope() {
    assert_eq!(scope_for_action("help", None), None);
}
