use super::super::server_authz::mcp_action_names;
use super::help_payload;
use std::collections::BTreeSet;

#[test]
fn help_payload_lists_every_supported_action() {
    let payload = help_payload();
    let help_actions = payload
        .pointer("/actions")
        .and_then(serde_json::Value::as_object)
        .expect("help payload should expose actions");
    let help_actions = help_actions
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let supported_actions = mcp_action_names().into_iter().collect::<BTreeSet<_>>();

    assert_eq!(help_actions, supported_actions);
}
