use crate::crates::core::config::{Config, McpTransport};
use crate::crates::services::acp_llm;
use std::error::Error;

pub async fn run_mcp(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Pre-warm the ACP adapter session pool. MCP ask/evaluate/suggest calls
    // will check out a warm session instead of paying cold-start per request.
    // NOTE: init_warm_pool uses log_warn (stderr) only — safe for stdio transport.
    acp_llm::init_warm_pool(cfg);

    match cfg.mcp_transport {
        McpTransport::Stdio => crate::crates::mcp::run_stdio_server(cfg.clone()).await,
        McpTransport::Http => {
            crate::crates::mcp::run_http_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port)
                .await
        }
        McpTransport::Both => {
            let host = cfg.mcp_http_host.clone();
            let port = cfg.mcp_http_port;
            tokio::try_join!(
                crate::crates::mcp::run_stdio_server(cfg.clone()),
                crate::crates::mcp::run_http_server(cfg.clone(), &host, port),
            )?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::crates::core::config::{Config, McpTransport};

    #[test]
    fn config_defaults_to_stdio_transport() {
        let cfg = Config::default();
        assert_eq!(cfg.mcp_transport, McpTransport::Stdio);
        assert_eq!(cfg.mcp_http_host, "0.0.0.0");
        assert_eq!(cfg.mcp_http_port, 8001);
    }
}
