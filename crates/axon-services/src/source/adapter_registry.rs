use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::cli_tool::CliToolSourceAdapter;
use axon_adapters::feed::FeedSourceAdapter;
use axon_adapters::git::GitSourceAdapter;
use axon_adapters::local::LocalSourceAdapter;
use axon_adapters::mcp_tool::McpToolSourceAdapter;
use axon_adapters::memory::{MemorySourceAdapter, MemorySourceProvider};
use axon_adapters::reddit::RedditSourceAdapter;
use axon_adapters::registry_sources::RegistrySourceAdapter;
use axon_adapters::sessions::SessionSourceAdapter;
use axon_adapters::upload::{UploadSourceAdapter, UploadSourceProvider};
use axon_adapters::youtube::YoutubeSourceAdapter;
use axon_adapters::{SourceAdapter, SourceAdapterRegistry};
use axon_api::source::{ArtifactReadResult, MemoryId, MemoryRecord};
use axon_core::config::Config;
use axon_memory::store::MemoryStore;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::runtime::ServiceJobRuntime;

struct ServiceMemorySourceProvider {
    store: Arc<dyn MemoryStore>,
}

#[async_trait]
impl MemorySourceProvider for ServiceMemorySourceProvider {
    async fn get(
        &self,
        memory_id: MemoryId,
    ) -> axon_adapters::adapter::Result<Option<MemoryRecord>> {
        self.store.get(memory_id).await
    }
}

struct ServiceUploadSourceProvider {
    cfg: Arc<Config>,
    jobs: Arc<dyn ServiceJobRuntime>,
}

#[async_trait]
impl UploadSourceProvider for ServiceUploadSourceProvider {
    async fn get(
        &self,
        source_identity: &str,
    ) -> axon_adapters::adapter::Result<Option<ArtifactReadResult>> {
        let ctx = ServiceContext::from_runtime(Arc::clone(&self.cfg), Arc::clone(&self.jobs));
        crate::uploads::resolve_upload_artifact(&ctx, source_identity).await
    }
}

pub(crate) async fn build_source_adapter_registry(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
) -> anyhow::Result<SourceAdapterRegistry> {
    let memory_store = crate::memory::memory_store(ctx).await?;
    let adapters: Vec<Arc<dyn SourceAdapter>> = vec![
        Arc::new(LocalSourceAdapter::new()),
        Arc::new(UploadSourceAdapter::with_provider(Arc::new(
            ServiceUploadSourceProvider {
                cfg: Arc::clone(&ctx.cfg),
                jobs: Arc::clone(&ctx.jobs),
            },
        ))),
        Arc::new(GitSourceAdapter::new()),
        Arc::clone(&runtime.web_source_adapter),
        Arc::new(FeedSourceAdapter::new()),
        Arc::new(YoutubeSourceAdapter::new()),
        Arc::new(RedditSourceAdapter::new()),
        Arc::new(SessionSourceAdapter::new()),
        Arc::new(RegistrySourceAdapter::new()),
        Arc::new(CliToolSourceAdapter::new()),
        Arc::new(McpToolSourceAdapter::new()),
        Arc::new(MemorySourceAdapter::new(Arc::new(
            ServiceMemorySourceProvider {
                store: memory_store,
            },
        ))),
    ];
    let registry = SourceAdapterRegistry::from_arc_adapters(adapters);
    registry.validate().await.map_err(anyhow::Error::new)?;
    Ok(registry)
}
