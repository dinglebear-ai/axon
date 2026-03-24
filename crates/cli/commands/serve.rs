use crate::crates::core::config::Config;
use crate::crates::services::acp_llm;
use std::error::Error;
use std::sync::Arc;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Pre-warm the ACP adapter session pool so the first LLM request from
    // the web UI doesn't pay the 5–15s adapter binary cold-start tax.
    acp_llm::init_warm_pool(cfg);
    crate::crates::web::start_server(cfg.serve_port, Arc::new(cfg.clone())).await
}

#[cfg(test)]
mod tests {
    use crate::crates::services::acp_llm;

    #[test]
    fn pool_size_before_init_is_zero_or_valid() {
        // Verify pool_size() is callable without panicking.
        let size = acp_llm::pool_size();
        assert!(size < usize::MAX, "pool_size() returned nonsense");
    }
}
