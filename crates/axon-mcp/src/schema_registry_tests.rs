use super::*;
use std::collections::BTreeSet;

#[test]
fn generated_actions_exactly_match_runtime_and_leaf_inventory() {
    let runtime: BTreeSet<_> = server_authz::MCP_ACTION_SPECS
        .iter()
        .map(|spec| spec.name)
        .collect();
    let schema: BTreeSet<_> = action_registry().iter().map(|spec| spec.action).collect();
    let operations: BTreeSet<_> = operation_registry()
        .iter()
        .map(|operation| operation.action)
        .collect();
    assert_eq!(schema.len(), action_registry().len());
    assert_eq!(runtime, schema);
    assert_eq!(runtime, operations);
    for action in ["artifacts", "chat", "codex", "source", "query"] {
        assert!(schema.contains(action));
    }
    assert!(!schema.contains("config"));
    for spec in action_registry() {
        jsonschema::validator_for(&request_schema_for(spec.request_dto)).unwrap();
    }
}
