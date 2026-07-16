//! `MemoryService` — durable agent memory: the full 14-method contract
//! surface (remember/get/search/context/link/update/reinforce/supersede/
//! contradict/pin/archive/forget/review/compact).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §MemoryService. The production implementation uses the typed
//! `axon-api::source` requests directly against authoritative SQLite. Every
//! mutation hands affected `memory://` identities to the canonical source
//! pipeline; this trait never embeds, upserts, deletes, or graph-mirrors
//! lifecycle state itself.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    MemoryArchiveRequest, MemoryCompactRequest, MemoryContextRequest, MemoryContextResult,
    MemoryContradictRequest, MemoryId, MemoryLinkRequest, MemoryPinRequest, MemoryRecord,
    MemoryReinforcement, MemoryRequest, MemoryResult, MemoryReviewRequest, MemoryReviewResult,
    MemorySearchRequest, MemorySearchResult, MemorySupersedeRequest, MemoryUpdateRequest,
};

use crate::context::ServiceContext;
use axon_memory::store::MemoryStore;

#[async_trait]
pub trait MemoryService: Send + Sync {
    async fn remember(&self, request: MemoryRequest) -> anyhow::Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> anyhow::Result<MemoryRecord>;
    async fn search(&self, request: MemorySearchRequest) -> anyhow::Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> anyhow::Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> anyhow::Result<MemoryResult>;
    async fn update(&self, request: MemoryUpdateRequest) -> anyhow::Result<MemoryResult>;
    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> anyhow::Result<MemoryResult>;
    async fn supersede(&self, request: MemorySupersedeRequest) -> anyhow::Result<MemoryResult>;
    async fn contradict(&self, request: MemoryContradictRequest) -> anyhow::Result<MemoryResult>;
    async fn pin(&self, request: MemoryPinRequest) -> anyhow::Result<MemoryResult>;
    async fn archive(&self, request: MemoryArchiveRequest) -> anyhow::Result<MemoryResult>;
    async fn forget(&self, memory_id: MemoryId) -> anyhow::Result<MemoryResult>;
    async fn review(&self, request: MemoryReviewRequest) -> anyhow::Result<MemoryReviewResult>;
    async fn compact(&self, request: MemoryCompactRequest) -> anyhow::Result<MemoryResult>;
}

pub struct MemoryServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl MemoryServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl MemoryService for MemoryServiceImpl {
    async fn remember(&self, request: MemoryRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let result = store.remember(request).await.map_err(store_error)?;
        sync(
            &self.ctx,
            store.as_ref(),
            [result.memory_id.clone()],
            "remember",
        )
        .await?;
        Ok(result)
    }

    async fn get(&self, memory_id: MemoryId) -> anyhow::Result<MemoryRecord> {
        crate::memory::memory_store(&self.ctx)
            .await?
            .get(memory_id.clone())
            .await
            .map_err(store_error)?
            .ok_or_else(|| anyhow::anyhow!("memory {} not found", memory_id.0))
    }

    async fn search(&self, request: MemorySearchRequest) -> anyhow::Result<MemorySearchResult> {
        crate::memory::memory_store(&self.ctx)
            .await?
            .search(request)
            .await
            .map_err(store_error)
    }

    async fn context(&self, request: MemoryContextRequest) -> anyhow::Result<MemoryContextResult> {
        crate::memory::memory_store(&self.ctx)
            .await?
            .context(request)
            .await
            .map_err(store_error)
    }

    async fn link(&self, request: MemoryLinkRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let memory_id = request.memory_id.clone();
        let result = store.link(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "link").await?;
        Ok(result)
    }

    async fn update(&self, request: MemoryUpdateRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let memory_id = request.memory_id.clone();
        let result = store.update(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "update").await?;
        Ok(result)
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let result = store
            .reinforce(memory_id.clone(), signal)
            .await
            .map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "reinforce").await?;
        Ok(result)
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let ids = [request.memory_id.clone(), request.replacement_id.clone()];
        let result = store.supersede(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), ids, "supersede").await?;
        Ok(result)
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let ids = [request.memory_id.clone(), request.conflicting_id.clone()];
        let result = store.contradict(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), ids, "contradict").await?;
        Ok(result)
    }

    async fn pin(&self, request: MemoryPinRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let memory_id = request.memory_id.clone();
        let result = store.pin(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "pin").await?;
        Ok(result)
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let memory_id = request.memory_id.clone();
        let result = store.archive(request).await.map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "archive").await?;
        Ok(result)
    }

    async fn forget(&self, memory_id: MemoryId) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let result = store
            .forget(axon_api::source::MemoryForgetRequest {
                memory_id: memory_id.clone(),
                reason: None,
                timestamp: axon_api::source::Timestamp::from(chrono::Utc::now()),
            })
            .await
            .map_err(store_error)?;
        sync(&self.ctx, store.as_ref(), [memory_id], "forget").await?;
        Ok(result)
    }

    async fn review(&self, request: MemoryReviewRequest) -> anyhow::Result<MemoryReviewResult> {
        crate::memory::memory_store(&self.ctx)
            .await?
            .review(request)
            .await
            .map_err(store_error)
    }

    async fn compact(&self, request: MemoryCompactRequest) -> anyhow::Result<MemoryResult> {
        let store = crate::memory::memory_store(&self.ctx).await?;
        let archived = request
            .archive_sources
            .then(|| request.memory_ids.clone())
            .unwrap_or_default();
        let result = store.compact(request).await.map_err(store_error)?;
        let mut ids = vec![result.memory_id.clone()];
        ids.extend(archived);
        sync(&self.ctx, store.as_ref(), ids, "compact").await?;
        Ok(result)
    }
}

async fn sync<I>(
    ctx: &ServiceContext,
    store: &dyn MemoryStore,
    memory_ids: I,
    operation: &str,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = MemoryId> + Send,
    I::IntoIter: Send,
{
    crate::memory::sync::sync_memory_records(ctx, store, memory_ids, operation).await
}

fn store_error(error: axon_api::source::ApiError) -> anyhow::Error {
    anyhow::anyhow!(error.message)
}

#[path = "memory_service_fake.rs"]
mod fake;
pub use fake::FakeMemoryService;

#[cfg(test)]
#[path = "memory_service_tests.rs"]
mod tests;
