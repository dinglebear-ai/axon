use crate::crates::core::config::Config;
use crate::crates::services::acp_llm;
use std::error::Error;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    acp_llm::init_warm_pool(cfg);
    crate::crates::mcp::run_unified_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port).await
}
