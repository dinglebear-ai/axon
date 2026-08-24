use axon_api::source::{PROJECTION_OPERATIONS, ProjectionOperation, validate_projection_registry};
use std::collections::BTreeSet;

#[test]
fn restored_projection_registry_is_bijective_across_transports() {
    validate_projection_registry(PROJECTION_OPERATIONS).unwrap();
    let cli = PROJECTION_OPERATIONS
        .iter()
        .map(|spec| spec.cli_name)
        .collect::<BTreeSet<_>>();
    let mcp = PROJECTION_OPERATIONS
        .iter()
        .map(|spec| spec.mcp_name)
        .collect::<BTreeSet<_>>();
    let rest = PROJECTION_OPERATIONS
        .iter()
        .map(|spec| spec.rest_path)
        .collect::<BTreeSet<_>>();
    assert_eq!(cli.len(), 5);
    assert_eq!(mcp.len(), 5);
    assert_eq!(rest.len(), 5);
    assert!(cli.contains("code-search"));
    assert!(mcp.contains("code_search"));
    assert!(rest.contains("/v1/code-search"));
    assert!(
        PROJECTION_OPERATIONS
            .iter()
            .any(|spec| spec.operation == ProjectionOperation::CodeSearch && !spec.mutating)
    );
}
