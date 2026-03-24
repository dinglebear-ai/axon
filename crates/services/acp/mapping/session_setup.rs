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

// filter_compatible_mcp_servers and filter_sdk_mcp_servers have been moved to
// mapping/mcp_filters.rs. Re-exported from mapping.rs for backward compatibility.

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
