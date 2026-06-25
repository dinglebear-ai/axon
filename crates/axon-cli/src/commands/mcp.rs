use axon_core::config::{Config, McpTransport};
use axon_vector::cache::enforce_core_dump_disabled_for_ask_cache;
use std::error::Error;

pub async fn run_mcp(cfg: &Config) -> Result<(), Box<dyn Error>> {
    enforce_core_dump_disabled_for_ask_cache(cfg).map_err(|e| -> Box<dyn Error> { e.into() })?;
    match cfg.mcp_transport {
        McpTransport::Stdio => axon_mcp::run_stdio_server(cfg.clone()).await,
        McpTransport::Http => {
            crate::commands::run_unified_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port)
                .await
        }
        McpTransport::Both => {
            let host = cfg.mcp_http_host.clone();
            let port = cfg.mcp_http_port;
            tokio::try_join!(
                axon_mcp::run_stdio_server(cfg.clone()),
                crate::commands::run_unified_server(cfg.clone(), &host, port),
            )?;
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
