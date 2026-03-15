use crate::crates::core::config::{Config, McpTransport};
use std::error::Error;

pub async fn run_mcp(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match cfg.mcp_transport {
        McpTransport::Stdio => crate::crates::mcp::run_stdio_server().await,
        McpTransport::Http => {
            crate::crates::mcp::run_http_server(&cfg.mcp_http_host, cfg.mcp_http_port).await
        }
        McpTransport::Both => {
            let host = cfg.mcp_http_host.clone();
            let port = cfg.mcp_http_port;
            tokio::try_join!(
                crate::crates::mcp::run_stdio_server(),
                crate::crates::mcp::run_http_server(&host, port),
            )?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::crates::core::config::{Config, McpTransport};

    #[test]
    fn config_defaults_to_http_transport() {
        let cfg = Config::default();
        assert_eq!(cfg.mcp_transport, McpTransport::Http);
        assert_eq!(cfg.mcp_http_host, "127.0.0.1");
        assert_eq!(cfg.mcp_http_port, 8001);
    }
}
