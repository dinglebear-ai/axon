//! Session setup helpers: MCP server conversion, capability filtering, and
//! session request construction (new vs. load).

use crate::crates::services::types::AcpMcpServerConfig;
use agent_client_protocol::{
    EnvVariable, HttpHeader, LoadSessionRequest, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio, NewSessionRequest, SessionId,
};
use std::error::Error;
use std::path::Path;

use super::validation::validate_session_cwd;
use crate::crates::services::acp::AcpSessionSetupRequest;

pub fn convert_mcp_servers(configs: &[AcpMcpServerConfig]) -> Vec<McpServer> {
    configs
        .iter()
        .map(|cfg| match cfg {
            AcpMcpServerConfig::Stdio {
                name,
                command,
                args,
                env,
            } => {
                let mut server = McpServerStdio::new(name.clone(), command.clone());
                if !args.is_empty() {
                    server = server.args(args.clone());
                }
                if !env.is_empty() {
                    server = server.env(
                        env.iter()
                            .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Stdio(server)
            }
            AcpMcpServerConfig::Http { name, url, headers } => {
                let mut server = McpServerHttp::new(name.clone(), url.clone());
                if !headers.is_empty() {
                    server = server.headers(
                        headers
                            .iter()
                            .map(|(n, v)| HttpHeader::new(n.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Http(server)
            }
            AcpMcpServerConfig::Sse { name, url, headers } => {
                let mut server = McpServerSse::new(name.clone(), url.clone());
                if !headers.is_empty() {
                    server = server.headers(
                        headers
                            .iter()
                            .map(|(n, v)| HttpHeader::new(n.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Sse(server)
            }
        })
        .collect()
}

/// Filter MCP servers to only those whose transport the adapter supports.
///
/// Stdio is always supported (no capability flag needed). Http requires
/// `mcp_capabilities.http = true`. Sse requires `mcp_capabilities.sse = true`.
/// Unsupported servers are logged at WARN and dropped so session setup
/// doesn't fail with an opaque adapter error.
///
/// Note: This function operates on `AcpMcpServerConfig` (before conversion).
/// Task 5 uses `filter_sdk_mcp_servers` instead (post-conversion SDK types).
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
            McpServer::Http(h) => {
                if !http_supported {
                    tracing::warn!(
                        server = %h.name,
                        "ACP: dropping HTTP MCP server — adapter lacks http capability"
                    );
                }
                http_supported
            }
            McpServer::Sse(s) => {
                if !sse_supported {
                    tracing::warn!(
                        server = %s.name,
                        "ACP: dropping SSE MCP server — adapter lacks sse capability"
                    );
                }
                sse_supported
            }
            _ => {
                tracing::warn!("ACP: dropping unknown MCP server transport — not supported by capability filter");
                false
            }
        })
        .cloned()
        .collect()
}

pub fn build_session_setup(
    session_id: Option<&str>,
    cwd: impl AsRef<Path>,
    mcp_servers: &[AcpMcpServerConfig],
) -> Result<AcpSessionSetupRequest, Box<dyn Error>> {
    let cwd = validate_session_cwd(cwd.as_ref())?;
    let sdk_mcp_servers = convert_mcp_servers(mcp_servers);
    match session_id.map(str::trim) {
        Some(sid) if !sid.is_empty() => {
            let mut req = LoadSessionRequest::new(SessionId::new(sid), cwd);
            if !sdk_mcp_servers.is_empty() {
                req = req.mcp_servers(sdk_mcp_servers);
            }
            Ok(AcpSessionSetupRequest::Load(req))
        }
        _ => {
            let mut req = NewSessionRequest::new(cwd);
            if !sdk_mcp_servers.is_empty() {
                req = req.mcp_servers(sdk_mcp_servers);
            }
            Ok(AcpSessionSetupRequest::New(req))
        }
    }
}
