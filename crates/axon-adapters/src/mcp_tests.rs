use crate::mcp_tool::{
    McpExecutionMode, McpToolCaller, McpToolError, McpToolTarget, RedactionStatus,
    resolve_and_acquire,
};

struct FakeCaller {
    response: &'static str,
}

impl McpToolCaller for FakeCaller {
    fn call(&self, _target: &McpToolTarget) -> Result<String, McpToolError> {
        Ok(self.response.to_string())
    }
}

#[tokio::test]
async fn mcp_tool_source_indexes_schema_without_calling_by_default() {
    let result = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::MetadataOnly,
        false,
        &[],
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
async fn mcp_tool_call_requires_execute_scope_and_allowlist() {
    let err = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        false,
        &[("server", "tool")],
        None,
    )
    .unwrap_err();
    assert_eq!(err.code, "auth.scope_required");

    let err = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        true,
        &[("other-server", "other-tool")],
        None,
    )
    .unwrap_err();
    assert_eq!(err.code, "mcp.tool_denied");

    let err = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        true,
        &[("server", "tool")],
        None,
    )
    .unwrap_err();
    assert_eq!(err.code, "mcp.caller_missing");
}

#[tokio::test]
async fn mcp_tool_call_invokes_the_injected_caller_and_redacts_output() {
    let caller = FakeCaller {
        response: secret_output_fixture(),
    };

    let result = resolve_and_acquire(
        "mcp://server/tool",
        McpExecutionMode::Execute,
        true,
        &[("server", "tool")],
        Some(&caller),
    )
    .unwrap();

    assert_eq!(result.tool_call_count, 1);
    assert_eq!(result.redaction_status, RedactionStatus::Redacted);
    assert!(!result.vector_payload_contains("authorization"));
    assert!(!result.vector_payload_contains("Bearer secret"));

    // The real redacted tool-call output must land in `documents`, not just
    // be reachable via the substring helper — this is what gets persisted
    // and embedded.
    assert_eq!(result.documents.len(), 1);
    let doc = &result.documents[0];
    assert_eq!(doc.content_kind, "tool_output");
    assert!(doc.content.contains("[redacted-secret]"));
    assert!(doc.content.contains("\"body\":\"ok\""));
    assert!(!doc.content.to_ascii_lowercase().contains("authorization"));
    assert!(!doc.content.contains("Bearer secret"));

    assert!(
        result
            .tool_facts
            .iter()
            .any(|fact| fact.name == "server" && fact.value == "server")
    );
    assert!(
        result
            .tool_facts
            .iter()
            .any(|fact| fact.name == "tool" && fact.value == "tool")
    );
    assert_eq!(result.graph_nodes.len(), 1);
    assert_eq!(result.graph_nodes[0].node_kind, "mcp_tool");
    assert_eq!(result.graph_nodes[0].stable_key, "server/tool");
}

#[tokio::test]
async fn mcp_tool_call_rejects_invalid_uri() {
    let err = resolve_and_acquire(
        "not-an-mcp-uri",
        McpExecutionMode::MetadataOnly,
        false,
        &[],
        None,
    )
    .unwrap_err();
    assert_eq!(err.code, "mcp.uri_invalid");
}

fn secret_output_fixture() -> &'static str {
    r#"{"headers":{"authorization":"Bearer secret"},"body":"ok"}"#
}
