//! MCP tool schema/call source contract.
//!
//! Metadata-only by default: `resolve_and_acquire` only ever indexes the
//! tool's schema unless `mode == McpExecutionMode::Execute` *and* the caller
//! both holds execute scope and has allowlisted the resolved `(server,
//! tool)` pair. This crate has no MCP protocol client (that would be a
//! network dependency outside `axon-adapters`'s territory — see
//! `docs/pipeline-unification/sources/adapter-scopes.md`'s note that MCP
//! discovery may use a helper such as `mcporter`, but the source identity
//! and graph evidence describe the server/tool/call/result, not the helper).
//! Instead, callers that need to actually invoke the tool pass an
//! [`McpToolCaller`] implementation; without one, `Execute` mode still
//! enforces the allowlist but degrades to schema-only content (never
//! silently "succeeds" with fabricated output).

mod redact;

use redact::redact_mcp_output;

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

/// A lightweight fact about the resolved MCP tool, precursor to a
/// `SourceParseFacts` row once this adapter is wired into the full
/// discover/acquire/normalize `SourceAdapter` pipeline (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolFact {
    pub fact_kind: &'static str,
    pub name: String,
    pub value: String,
}

/// A precursor to a `GraphNodeCandidate` describing the external MCP
/// server/tool this source resolves to. Full `GraphCandidate` assembly
/// (which additionally requires `job_id`/`source_id`/`document_id`) happens
/// at the `SourceAdapter` integration layer — see module docs and the
/// crate-level follow-up note on wiring `cli_tool`/`mcp_tool` into
/// `SourceAdapter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpExternalResourceNode {
    pub node_kind: &'static str,
    pub stable_key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolAcquireResult {
    pub documents: Vec<McpToolDocument>,
    pub tool_facts: Vec<McpToolFact>,
    pub graph_nodes: Vec<McpExternalResourceNode>,
    pub tool_call_count: usize,
    pub redaction_status: RedactionStatus,
}

impl McpToolAcquireResult {
    /// Searches every document's content (the redacted tool-call payload in
    /// `Execute` mode, or the schema description in metadata-only mode).
    pub fn vector_payload_contains(&self, needle: &str) -> bool {
        let needle = needle.to_ascii_lowercase();
        self.documents
            .iter()
            .any(|doc| doc.content.to_ascii_lowercase().contains(&needle))
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

/// A `mcp://server/tool` URI split into its server and tool identity, used
/// as the allowlist key (checked verbatim, no globbing) before any call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolTarget {
    pub server: String,
    pub tool: String,
}

/// Injected by the caller when it wants `Execute` mode to actually invoke
/// the tool (this crate never does so itself — see module docs). Returns
/// the raw, unredacted tool response as text; `resolve_and_acquire` applies
/// redaction before it is ever returned.
pub trait McpToolCaller {
    fn call(&self, target: &McpToolTarget) -> Result<String, McpToolError>;
}

/// Resolves an `mcp://server/tool` source and, in metadata-only mode,
/// indexes only its schema without calling anything. In `Execute` mode,
/// `has_execute_scope` must be true and `(server, tool)` must appear
/// verbatim in `allowlist` (checked by [`validate_execute_allowed`]) before
/// `caller` (if supplied) is invoked; the raw response is always redacted
/// before it lands in `documents`/the vector payload.
pub fn resolve_and_acquire(
    uri: &str,
    mode: McpExecutionMode,
    has_execute_scope: bool,
    allowlist: &[(&str, &str)],
    caller: Option<&dyn McpToolCaller>,
) -> Result<McpToolAcquireResult, McpToolError> {
    let target = parse_mcp_target(uri)?;
    let graph_nodes = vec![McpExternalResourceNode {
        node_kind: "mcp_tool",
        stable_key: format!("{}/{}", target.server, target.tool),
        label: format!("{} ({})", target.tool, target.server),
    }];

    let schema_doc = McpToolDocument {
        content_kind: "structured",
        content: format!("schema for {uri}"),
    };
    let tool_facts = |call_count: usize| {
        vec![
            McpToolFact {
                fact_kind: "mcp_target",
                name: "server".to_string(),
                value: target.server.clone(),
            },
            McpToolFact {
                fact_kind: "mcp_target",
                name: "tool".to_string(),
                value: target.tool.clone(),
            },
            McpToolFact {
                fact_kind: "mcp_call",
                name: "tool_call_count".to_string(),
                value: call_count.to_string(),
            },
        ]
    };

    if mode == McpExecutionMode::MetadataOnly {
        return Ok(McpToolAcquireResult {
            documents: vec![schema_doc],
            tool_facts: tool_facts(0),
            graph_nodes,
            tool_call_count: 0,
            redaction_status: RedactionStatus::Clean,
        });
    }

    validate_execute_allowed(&target, has_execute_scope, allowlist)?;

    let Some(caller) = caller else {
        // Authorized (allowlist passed) but no caller injected: still
        // schema-only content — never silently "succeeds" with fabricated
        // output.
        return Ok(McpToolAcquireResult {
            documents: vec![schema_doc],
            tool_facts: tool_facts(0),
            graph_nodes,
            tool_call_count: 0,
            redaction_status: RedactionStatus::Clean,
        });
    };

    let raw_payload = caller.call(&target)?;
    let (redacted_payload, was_redacted) = redact_mcp_output(&raw_payload);

    Ok(McpToolAcquireResult {
        documents: vec![McpToolDocument {
            content_kind: "tool_output",
            content: redacted_payload,
        }],
        tool_facts: tool_facts(1),
        graph_nodes,
        tool_call_count: 1,
        redaction_status: if was_redacted {
            RedactionStatus::Redacted
        } else {
            RedactionStatus::Clean
        },
    })
}

/// Real allowlist validator — must be called before any invocation.
fn validate_execute_allowed(
    target: &McpToolTarget,
    has_execute_scope: bool,
    allowlist: &[(&str, &str)],
) -> Result<(), McpToolError> {
    if !has_execute_scope {
        return Err(McpToolError {
            code: "auth.scope_required",
            message: "MCP tool call execution requires execute scope".to_string(),
        });
    }
    if allowlist.is_empty() {
        return Err(McpToolError {
            code: "mcp.allowlist_empty",
            message: "no MCP server/tool pairs are allowlisted for execution".to_string(),
        });
    }
    if !allowlist
        .iter()
        .any(|(server, tool)| *server == target.server && *tool == target.tool)
    {
        return Err(McpToolError {
            code: "mcp.tool_denied",
            message: format!(
                "mcp tool `{}/{}` is not allowlisted",
                target.server, target.tool
            ),
        });
    }
    Ok(())
}

fn parse_mcp_target(uri: &str) -> Result<McpToolTarget, McpToolError> {
    let Some(rest) = uri.strip_prefix("mcp://") else {
        return Err(McpToolError {
            code: "mcp.uri_invalid",
            message: "MCP source must use mcp://server/tool".to_string(),
        });
    };
    let mut parts = rest.splitn(2, '/');
    let server = parts.next().filter(|part| !part.is_empty());
    let tool = parts.next().filter(|part| !part.is_empty());
    match (server, tool) {
        (Some(server), Some(tool)) => Ok(McpToolTarget {
            server: server.to_string(),
            tool: tool.to_string(),
        }),
        _ => Err(McpToolError {
            code: "mcp.uri_invalid",
            message: "MCP source must use mcp://server/tool".to_string(),
        }),
    }
}
