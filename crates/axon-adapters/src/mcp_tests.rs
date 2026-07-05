use crate::mcp_tool::{McpExecutionMode, RedactionStatus, resolve_and_acquire};

#[tokio::test]
async fn mcp_tool_source_indexes_schema_without_calling_by_default() {
    let result = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::MetadataOnly,
        false,
        None,
    )
    .unwrap();

    assert_eq!(result.tool_call_count, 0);
    assert!(
        result
            .documents
            .iter()
            .any(|doc| doc.content_kind == "structured")
    );
}

#[tokio::test]
async fn mcp_tool_call_requires_execute_scope_and_redacts_output() {
    let err = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        false,
        Some(secret_output_fixture()),
    )
    .unwrap_err();
    assert_eq!(err.code, "auth.scope_required");

    let result = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        true,
        Some(secret_output_fixture()),
    )
    .unwrap();

    assert_eq!(result.tool_call_count, 1);
    assert_eq!(result.redaction_status, RedactionStatus::Redacted);
    assert!(!result.vector_payload_contains("authorization"));
    assert!(!result.vector_payload_contains("Bearer secret"));
}

fn secret_output_fixture() -> &'static str {
    r#"{"headers":{"authorization":"Bearer secret"},"body":"ok"}"#
}
