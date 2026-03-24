//! MCP server transport capability filters.
//!
//! These functions filter `AcpMcpServerConfig` (pre-conversion) or SDK
//! `McpServer` (post-conversion) lists to only those transports the adapter
//! advertises as supported. Called after `initialize` returns capabilities.

use crate::crates::services::types::AcpMcpServerConfig;
use agent_client_protocol::{McpServer, McpServerHttp, McpServerSse};

/// Filter MCP servers to only those whose transport the adapter supports.
///
/// Stdio is always supported (no capability flag needed). Http requires
/// `http_supported = true`. Sse requires `sse_supported = true`.
/// Unsupported servers are logged at WARN and dropped so session setup
/// doesn't fail with an opaque adapter error.
///
/// Note: This function operates on `AcpMcpServerConfig` (before conversion).
/// `filter_sdk_mcp_servers` is used instead for post-conversion SDK types.
/// Kept for testing and potential future use.
#[cfg_attr(not(test), allow(dead_code))]
pub fn filter_compatible_mcp_servers(
    configs: &[AcpMcpServerConfig],
    http_supported: bool,
    sse_supported: bool,
) -> Vec<AcpMcpServerConfig> {
    configs
        .iter()
        .filter(|cfg| match cfg {
            AcpMcpServerConfig::Stdio { .. } => true,
            AcpMcpServerConfig::Http { name, .. } => {
                if !http_supported {
                    tracing::warn!(
                        server = %name,
                        "ACP: dropping HTTP MCP server — adapter does not advertise http transport support"
                    );
                }
                http_supported
            }
            AcpMcpServerConfig::Sse { name, .. } => {
                if !sse_supported {
                    tracing::warn!(
                        server = %name,
                        "ACP: dropping SSE MCP server — adapter does not advertise sse transport support"
                    );
                }
                sse_supported
            }
        })
        .cloned()
        .collect()
}

/// Filter already-converted SDK `McpServer` objects to transports the adapter supports.
///
/// Used post-initialize when capabilities are known but the session setup request
/// already holds SDK types. Stdio is always kept; Http/Sse require the respective
/// capability flag.
///
/// Called by:
/// - `apply_mcp_capability_filter()` in `runtime.rs` (one-shot mode)
/// - `ensure_turn_session()` in `persistent_conn/turn.rs` (persistent-connection mode)
pub fn filter_sdk_mcp_servers(
    servers: &[McpServer],
    http_supported: bool,
    sse_supported: bool,
) -> Vec<McpServer> {
    servers
        .iter()
        .filter(|s| match s {
            McpServer::Stdio(_) => true,
            McpServer::Http(McpServerHttp { name, .. }) => {
                if !http_supported {
                    tracing::warn!(
                        server = %name,
                        "ACP: dropping HTTP MCP server — adapter lacks http capability"
                    );
                }
                http_supported
            }
            McpServer::Sse(McpServerSse { name, .. }) => {
                if !sse_supported {
                    tracing::warn!(
                        server = %name,
                        "ACP: dropping SSE MCP server — adapter lacks sse capability"
                    );
                }
                sse_supported
            }
            _ => {
                tracing::warn!(
                    "ACP: dropping unknown MCP server transport — not supported by capability filter"
                );
                false
            }
        })
        .cloned()
        .collect()
}
