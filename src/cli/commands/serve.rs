use crate::core::config::Config;
use crate::vector::cache::enforce_core_dump_disabled_for_ask_cache;
use std::error::Error;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    enforce_core_dump_disabled_for_ask_cache(cfg).map_err(|e| -> Box<dyn Error> { e.into() })?;
    crate::mcp::run_unified_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port).await
}
