//! MCP tool schema/call source contract.

pub const MODULE_NAME: &str = "mcp_tool";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpExecutionMode {
    MetadataOnly,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStatus {
    Clean,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolDocument {
    pub content_kind: &'static str,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolAcquireResult {
    pub documents: Vec<McpToolDocument>,
    pub tool_call_count: usize,
    pub redaction_status: RedactionStatus,
    vector_payload: String,
}

impl McpToolAcquireResult {
    pub fn vector_payload_contains(&self, needle: &str) -> bool {
        self.vector_payload
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for McpToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for McpToolError {}

pub fn resolve_and_acquire(
    uri: &str,
    mode: McpExecutionMode,
    has_execute_scope: bool,
    tool_output: Option<&str>,
) -> Result<McpToolAcquireResult, McpToolError> {
    if !uri.starts_with("mcp://") {
        return Err(McpToolError {
            code: "mcp.uri_invalid",
            message: "MCP source must use mcp://server/tool".to_string(),
        });
    }

    if mode == McpExecutionMode::Execute && !has_execute_scope {
        return Err(McpToolError {
            code: "auth.scope_required",
            message: "MCP tool call execution requires execute scope".to_string(),
        });
    }

    let schema_doc = McpToolDocument {
        content_kind: "structured",
        content: format!("schema for {uri}"),
    };
    let raw_payload = tool_output.unwrap_or(&schema_doc.content);
    let redacted_payload = redact_mcp_output(raw_payload);
    let redacted = redacted_payload != raw_payload;

    Ok(McpToolAcquireResult {
        documents: vec![schema_doc],
        tool_call_count: usize::from(mode == McpExecutionMode::Execute),
        redaction_status: if redacted {
            RedactionStatus::Redacted
        } else {
            RedactionStatus::Clean
        },
        vector_payload: redacted_payload,
    })
}

fn redact_mcp_output(output: &str) -> String {
    output
        .replace("authorization", "[redacted-header]")
        .replace("Authorization", "[redacted-header]")
        .replace("Bearer secret", "[redacted-secret]")
}
