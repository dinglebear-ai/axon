//! MCP server configuration cache for ACP sessions.
//!
//! Extracted from `sync_mode.rs` to keep that module under the monolith line limit.
//! Uses metadata-aware cache invalidation (`mtime`) so edits to `mcp.json`
//! are picked up automatically without process restart.

use crate::crates::services::types::AcpMcpServerConfig;
use std::env;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ── Cache ─────────────────────────────────────────────────────────────────────

pub(super) struct McpServerCache {
    pub(super) servers: Vec<AcpMcpServerConfig>,
    pub(super) config_path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

pub(super) static MCP_SERVER_CACHE: std::sync::OnceLock<std::sync::Mutex<Option<McpServerCache>>> =
    std::sync::OnceLock::new();

/// Read MCP server configs from `AXON_DATA_DIR/axon/mcp.json` (or
/// `~/.config/axon/mcp.json` fallback). Returns an empty vec on any error.
///
/// Cache invalidation is file-change driven:
/// - if path and mtime are unchanged, return cached servers
/// - if either changes, reload from disk
pub(super) async fn read_axon_mcp_servers() -> Vec<AcpMcpServerConfig> {
    let Some(config_path) = resolve_mcp_config_path() else {
        return vec![];
    };
    let modified_at = file_modified_at(&config_path).await;
    let cache_lock = MCP_SERVER_CACHE.get_or_init(|| std::sync::Mutex::new(None));

    // Check cache under a short lock scope — drop the guard before any await.
    {
        let guard = cache_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cached) = *guard
            && cached.config_path == config_path
            && cached.modified_at == modified_at
        {
            return cached.servers.clone();
        }
    }

    // Cache miss — fetch from disk.
    let servers = fetch_axon_mcp_servers_from_disk(&config_path).await;

    // Update cache.
    {
        let mut guard = cache_lock.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(McpServerCache {
            servers: servers.clone(),
            config_path,
            modified_at,
        });
    }

    servers
}

/// Read and parse MCP server configs from disk. Called only on cache miss.
pub(super) async fn fetch_axon_mcp_servers_from_disk(
    config_path: &Path,
) -> Vec<AcpMcpServerConfig> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AxonConfig {
        #[serde(default)]
        mcp_servers: std::collections::HashMap<String, McpServerEntry>,
    }

    #[derive(serde::Deserialize)]
    struct HeaderEntry {
        name: String,
        value: String,
    }

    #[derive(serde::Deserialize)]
    struct McpServerEntry {
        command: Option<String>,
        args: Option<Vec<String>>,
        env: Option<std::collections::HashMap<String, String>>,
        url: Option<String>,
        /// "http" (default for URL entries) or "sse"
        transport: Option<String>,
        /// HTTP headers for http/sse transports
        #[serde(default)]
        headers: Vec<HeaderEntry>,
    }

    let raw = match tokio::fs::read_to_string(config_path).await {
        Ok(raw) => raw,
        Err(_) => return vec![],
    };

    let config: AxonConfig = match serde_json::from_str(&raw) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!(
                context = "pulse_chat",
                path = %config_path.display(),
                error = %e,
                "failed to parse MCP config",
            );
            return vec![];
        }
    };

    config
        .mcp_servers
        .into_iter()
        .filter_map(|(name, entry)| {
            let url = entry.url.filter(|u| !u.is_empty());
            let command = entry.command.filter(|c| !c.is_empty());
            if url.is_none() && command.is_none() {
                // Skip entries that have neither a URL nor a stdio command.
                return None;
            }
            if let Some(url) = url {
                let headers: Vec<(String, String)> = entry
                    .headers
                    .into_iter()
                    .map(|h| (h.name, h.value))
                    .collect();
                match entry.transport.as_deref() {
                    Some("sse") => Some(AcpMcpServerConfig::Sse { name, url, headers }),
                    None | Some("http") => Some(AcpMcpServerConfig::Http { name, url, headers }),
                    Some(unknown) => {
                        tracing::warn!(
                            server = %name,
                            transport = %unknown,
                            "mcp.json: unknown transport value; treating as http"
                        );
                        Some(AcpMcpServerConfig::Http { name, url, headers })
                    }
                }
            } else {
                let cmd = command.unwrap_or_default();
                // SEC-2: validate command before spawning a child process.
                if !is_safe_mcp_command(&cmd) {
                    tracing::warn!(
                        context = "pulse_chat",
                        server = %name,
                        command = %cmd,
                        "skipping MCP server: command failed safety check",
                    );
                    return None;
                }
                Some(AcpMcpServerConfig::Stdio {
                    name,
                    command: cmd,
                    args: entry.args.unwrap_or_default(),
                    env: entry.env.unwrap_or_default().into_iter().collect(),
                })
            }
        })
        .collect()
}

fn resolve_mcp_config_path() -> Option<PathBuf> {
    if let Some(data_dir) = crate::crates::core::paths::axon_data_dir() {
        Some(data_dir.join("axon/mcp.json"))
    } else if let Ok(home) = env::var("HOME") {
        Some(PathBuf::from(home).join(".config/axon/mcp.json"))
    } else {
        None
    }
}

async fn file_modified_at(path: &Path) -> Option<SystemTime> {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
}

/// Validate that an MCP server command is not a shell interpreter and, if it
/// looks like a path, is absolute.  This blocks trivial command-injection via
/// `mcp.json` entries like `{"command": "bash", "args": ["-c", "evil"]}`.
pub(super) fn is_safe_mcp_command(cmd: &str) -> bool {
    // Reject empty or whitespace-only commands.
    if cmd.trim().is_empty() {
        return false;
    }
    // Reject known shell interpreters by basename.
    let basename = Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd);
    let shell_names = [
        "sh",
        "bash",
        "zsh",
        "fish",
        "dash",
        "ksh",
        "csh",
        "tcsh",
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
    ];
    if shell_names.contains(&basename.to_ascii_lowercase().as_str()) {
        return false;
    }
    // If the command contains a path separator (/ or \) it must be absolute —
    // reject relative paths like `./evil`, `../evil`, or `..\evil`.
    let has_separator = cmd.contains('/') || cmd.contains('\\');
    if has_separator && !Path::new(cmd).is_absolute() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_sse_entry_from_json() {
        let json =
            r#"{"mcpServers": {"my-sse": {"url": "http://localhost/sse", "transport": "sse"}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        tokio::fs::write(&path, json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(&path).await;
        assert_eq!(servers.len(), 1);
        assert!(matches!(servers[0], AcpMcpServerConfig::Sse { .. }));
    }

    #[tokio::test]
    async fn parses_http_entry_with_headers() {
        let json = r#"{"mcpServers": {"my-http": {"url": "http://localhost/mcp", "headers": [{"name": "Authorization", "value": "Bearer tok"}]}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        tokio::fs::write(&path, json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(&path).await;
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            AcpMcpServerConfig::Http { headers, .. } => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0].0, "Authorization");
                assert_eq!(headers[0].1, "Bearer tok");
            }
            _ => panic!("expected Http"),
        }
    }

    #[tokio::test]
    async fn http_url_without_transport_defaults_to_http() {
        let json = r#"{"mcpServers": {"my-http": {"url": "http://localhost/mcp"}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        tokio::fs::write(&path, json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(&path).await;
        assert_eq!(servers.len(), 1);
        assert!(matches!(servers[0], AcpMcpServerConfig::Http { .. }));
    }

    #[tokio::test]
    async fn unknown_transport_falls_back_to_http() {
        let json = r#"{"mcpServers": {"s": {"url": "http://localhost/x", "transport": "ws"}}}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        tokio::fs::write(&path, json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(&path).await;
        assert_eq!(servers.len(), 1);
        assert!(matches!(servers[0], AcpMcpServerConfig::Http { .. }));
    }

    #[test]
    fn is_safe_mcp_command_rejects_shell() {
        assert!(!is_safe_mcp_command("bash"));
        assert!(!is_safe_mcp_command("sh"));
    }

    #[test]
    fn is_safe_mcp_command_accepts_absolute_path() {
        assert!(is_safe_mcp_command("/usr/local/bin/mcp-server"));
    }
}
