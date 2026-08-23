use super::*;

#[test]
fn projection_registry_has_five_unique_transport_names() {
    validate_projection_registry(PROJECTION_OPERATIONS).unwrap();
    assert_eq!(PROJECTION_OPERATIONS.len(), 5);
}

#[test]
fn code_search_registry_entry_is_read_only_without_idempotency() {
    let spec = PROJECTION_OPERATIONS
        .iter()
        .find(|spec| spec.operation == ProjectionOperation::CodeSearch)
        .unwrap();

    assert_eq!(spec.cli_name, "code-search");
    assert_eq!(spec.mcp_name, "code_search");
    assert_eq!(spec.auth_scope, "axon:read");
    assert!(!spec.mutating);
    assert!(!spec.supports_idempotency);
}

#[test]
fn restored_projection_dtos_are_owned_not_removed() {
    for name in crate::schema_registry::projection_dto_names() {
        assert!(!crate::schema_registry::removed_dto_names().contains(name));
    }
}
