//! Session setup helpers: MCP server conversion, capability filtering, and
//! session request construction (new vs. load).

use crate::crates::services::types::AcpMcpServerConfig;
use agent_client_protocol::{
    EnvVariable, HttpHeader, LoadSessionRequest, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio, NewSessionRequest, SessionId,
};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::crates::services::acp::AcpSessionSetupRequest;

// filter_compatible_mcp_servers and filter_sdk_mcp_servers have been moved to
// mapping/mcp_filters.rs. Re-exported from mapping.rs for backward compatibility.

// ── Error type ──────────────────────────────────────────────────────────────

/// Typed error returned by [`build_session_setup`].
///
/// Each variant captures a specific validation failure so callers can match on
/// the exact failure mode without parsing error strings. Entry points
/// (`prepare_session_setup`, `prepare_session_probe_setup`) box this into
/// `Box<dyn Error>` at their boundary — internal code works with the concrete
/// type.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum SessionSetupError {
    /// The working-directory path is not absolute.
    CwdNotAbsolute { path: PathBuf },
    /// The working-directory path does not exist on the filesystem.
    CwdNotFound { path: PathBuf },
    /// The working-directory path exists but is not a directory.
    CwdNotDir { path: PathBuf },
}

impl fmt::Display for SessionSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CwdNotAbsolute { path } => {
                write!(
                    f,
                    "ACP session cwd must be an absolute path: {}",
                    path.display()
                )
            }
            Self::CwdNotFound { path } => {
                write!(f, "ACP session cwd does not exist: {}", path.display())
            }
            Self::CwdNotDir { path } => {
                write!(
                    f,
                    "ACP session cwd exists but is not a directory: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for SessionSetupError {}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Validate the session working directory and return its canonical `PathBuf`.
///
/// Returns a typed [`SessionSetupError`] rather than `Box<dyn Error>`.
fn validate_cwd(cwd: &Path) -> Result<PathBuf, SessionSetupError> {
    if !cwd.is_absolute() {
        return Err(SessionSetupError::CwdNotAbsolute {
            path: cwd.to_path_buf(),
        });
    }
    if !cwd.exists() {
        return Err(SessionSetupError::CwdNotFound {
            path: cwd.to_path_buf(),
        });
    }
    if !cwd.is_dir() {
        return Err(SessionSetupError::CwdNotDir {
            path: cwd.to_path_buf(),
        });
    }
    Ok(cwd.to_path_buf())
}

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
) -> Result<AcpSessionSetupRequest, SessionSetupError> {
    let cwd = validate_cwd(cwd.as_ref())?;
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
