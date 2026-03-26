pub mod acp;
pub mod acp_llm;
pub mod context;
pub mod crawl;
pub mod debug;
pub mod embed;
pub mod error;
pub mod events;
pub mod export;
pub mod extract;
pub mod graph;
pub mod ingest;
pub mod jobs;
pub mod map;
pub mod query;
pub mod refresh;
pub mod scrape;
pub mod screenshot;
pub mod search;
pub mod system;
pub mod types;
pub mod watch;

#[cfg(test)]
mod service_context_tests {
    use super::context::ServiceContext;
    use crate::crates::core::config::Config;

    #[tokio::test]
    async fn service_context_resolves_capabilities_for_lite_mode() {
        let cfg = Config::default_lite();
        let ctx = ServiceContext::new(std::sync::Arc::new(cfg))
            .await
            .expect("service context");

        assert!(!ctx.capabilities.export.supported);
        assert!(!ctx.capabilities.graph.supported);
    }

    #[tokio::test]
    async fn service_context_resolves_capabilities_for_full_mode() {
        let cfg = Config::default();
        let ctx = ServiceContext::new(std::sync::Arc::new(cfg))
            .await
            .expect("service context");

        assert!(ctx.capabilities.export.supported);
    }
}
