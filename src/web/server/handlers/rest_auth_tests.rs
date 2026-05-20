#[test]
fn unknown_mcp_action_is_explicitly_denied() {
    assert_eq!(
        crate::mcp::auth::scope_for_action("future_action", None),
        Some("__deny__")
    );
}
