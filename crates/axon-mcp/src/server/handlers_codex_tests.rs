use super::*;

#[test]
fn codex_resource_enum_covers_management_reads() {
    assert_eq!(
        resource_action(&CodexResource::RateLimits).unwrap(),
        ControlAction::RateLimitsRead
    );
    assert_eq!(
        resource_action(&CodexResource::ModelProviderCapabilities).unwrap(),
        ControlAction::ModelProviderCapabilitiesRead
    );
    assert_eq!(
        resource_action(&CodexResource::McpResource).unwrap(),
        ControlAction::McpServerResourceRead
    );
}

#[test]
fn codex_mutation_enum_maps_without_stringly_typed_input() {
    assert_eq!(
        mutation_action(CodexMutationAction::McpServerToolCall).unwrap(),
        MutationAction::McpServerToolCall
    );
    assert_eq!(
        mutation_action(CodexMutationAction::ExternalAgentConfigImport).unwrap(),
        MutationAction::ExternalAgentConfigImport
    );
}

#[test]
fn event_cursor_requires_both_components() {
    assert!(event_cursor(None, None).unwrap().is_none());
    assert!(event_cursor(Some(1), Some(2)).unwrap().is_some());
    assert!(event_cursor(Some(1), None).is_err());
    assert!(event_cursor(None, Some(2)).is_err());
}
