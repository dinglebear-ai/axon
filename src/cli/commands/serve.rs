use crate::core::config::Config;
use crate::services::acp_llm;
use std::error::Error;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    acp_llm::init_warm_pool(cfg);
    crate::mcp::run_unified_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port).await
}
